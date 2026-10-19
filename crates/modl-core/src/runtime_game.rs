//! Authoritative, data-only handoff from an admitted game to either production host.
use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;

use crate::{
    CartridgeError, CompileMode, ProjectAsset, compile_project, game::PackedGame,
    unpack_cartridge_project,
};

/// Compiled source transport bound used by both native and headless adapters, per member.
pub const MAX_RUNTIME_JAVASCRIPT_BYTES: usize = 2 * 1024 * 1024;

/// Private construction prevents hosts from accidentally omitting an unused dependency or
/// substituting an enclosing shipping package for a logical member's console ROM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeGame {
    revision: u8,
    identity: String,
    debug: bool,
    members: BTreeMap<String, RuntimeGameMember>,
}

impl RuntimeGame {
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    #[must_use]
    pub fn debug(&self) -> bool {
        self.debug
    }

    #[must_use]
    pub fn members(&self) -> &BTreeMap<String, RuntimeGameMember> {
        &self.members
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeGameMember {
    pub javascript: String,
    pub source_map: String,
    pub symbol_names: BTreeMap<String, String>,
    pub manifest: RuntimeGameManifest,
    /// Base64 data only. Source modules, artwork and auxiliary authoring files are not imports.
    pub files: BTreeMap<String, String>,
    /// This member's canonical logical cartridge, never the enclosing game container.
    pub rom: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeGameManifest {
    pub id: String,
    pub hardware_revision: u8,
    pub update_rate: u8,
    pub display: Option<String>,
    pub assets: BTreeMap<String, ProjectAsset>,
}

/// Compiles every member in one mode from authoritative source, retaining that member's
/// own symbols, source relationships, asset files and ROM. This does not execute JavaScript
/// or read dependency aliases as host filesystem paths. Hosts supply only seed, transfer
/// parameter, input and initial saves separately.
///
/// # Errors
/// Returns alias-qualified compilation/asset errors or M014072 if a compiled member exceeds
/// the bounded host source transport. No partial runtime game is returned.
pub fn prepare_runtime_game(
    game: &PackedGame,
    mode: CompileMode,
) -> Result<RuntimeGame, CartridgeError> {
    let members = game
        .catalog()
        .cartridges()
        .iter()
        .map(|(alias, packed)| {
            let prepare = || {
                let view = unpack_cartridge_project(&packed.bytes)?;
                let compilation = compile_project(&view.manifest, &view.files, mode)?;
                if let Some(diagnostic) = compilation.analysis.diagnostics.first() {
                    return Err(CartridgeError {
                        code: "M014015",
                        message: format!(
                            "cartridge source failed: error[{}] {}",
                            diagnostic.code, diagnostic.message
                        ),
                    });
                }
                let generated = compilation.generated.ok_or_else(|| CartridgeError {
                    code: "M014015",
                    message: "source produced no runtime program".to_owned(),
                })?;
                check_program_size(generated.javascript.len())?;
                let manifest = &packed.manifest;
                let paths = manifest
                    .assets
                    .values()
                    .map(|asset| asset.path.as_str())
                    .chain(manifest.display.as_deref())
                    .collect::<BTreeSet<_>>();
                let files = paths
                    .into_iter()
                    .map(|path| {
                        view.files
                            .get(path)
                            .map(|bytes| (path.to_owned(), STANDARD.encode(bytes)))
                            .ok_or_else(|| CartridgeError {
                                code: "M014010",
                                message: format!("missing runtime asset '{path}'"),
                            })
                    })
                    .collect::<Result<_, _>>()?;
                Ok(RuntimeGameMember {
                    javascript: generated.javascript,
                    source_map: generated.source_map_json,
                    symbol_names: compilation
                        .analysis
                        .symbols
                        .into_iter()
                        .map(|symbol| (format!("s{}", symbol.id.0), symbol.name))
                        .collect(),
                    manifest: RuntimeGameManifest {
                        id: manifest.id.clone(),
                        hardware_revision: manifest.hardware_revision,
                        update_rate: manifest.update_rate,
                        display: manifest.display.clone(),
                        assets: manifest
                            .assets
                            .iter()
                            .map(|(name, asset)| {
                                (
                                    name.clone(),
                                    ProjectAsset {
                                        kind: asset.kind,
                                        path: asset.path.clone(),
                                    },
                                )
                            })
                            .collect(),
                    },
                    files,
                    rom: STANDARD.encode(&packed.bytes),
                })
            };
            prepare()
                .map(|member| (alias.clone(), member))
                .map_err(|error| CartridgeError {
                    code: error.code,
                    message: format!("game member '{alias}': {}", error.message),
                })
        })
        .collect::<Result<_, _>>()?;
    Ok(RuntimeGame {
        revision: 1,
        identity: game.identity().to_owned(),
        debug: mode == CompileMode::Debug,
        members,
    })
}

fn check_program_size(bytes: usize) -> Result<(), CartridgeError> {
    if bytes > MAX_RUNTIME_JAVASCRIPT_BYTES {
        return Err(CartridgeError {
            code: "M014072",
            message: "compiled runtime source exceeds 2 MiB per member".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime_game_tests.rs"]
mod tests;
