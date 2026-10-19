use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::bundle::{
    BUNDLE_ENTRY, BundleSource, MAX_BUNDLE_CARTRIDGES, checked_source_size, valid_alias,
};
use crate::{
    AssetKind, CARTRIDGE_FORMAT_REVISION, CartridgeError, FileId, ProjectAsset, ProjectManifest,
    SourceFile, TokenKind, UnpackedProject, lex, parse_project_manifest,
};

/// The authored work-cart revision supported by this compiler.
pub const WORKCART_FORMAT_REVISION: u16 = 2;
/// Bound for a flat source-visible game document, including escaped member source strings.
pub const MAX_WORKCART_DOCUMENT_BYTES: usize = 128 * 1024 * 1024;

type MaterializedFiles = (BTreeMap<String, ProjectAsset>, BTreeMap<String, Vec<u8>>);

/// A parsed source-visible work-cart and its compiler-facing project view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkcartProject {
    /// Exact UTF-8 authoring source, retained when the cartridge is packed.
    pub source: String,
    /// Validated project metadata consumed by the compiler and runtime.
    pub manifest: ProjectManifest,
    /// MODL, asset, and presentation files materialized from the work-cart.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Author-time-only deterministic generation recipes.
    pub recipes: Vec<WorkcartRecipe>,
    /// Named source and replay tests retained in the work-cart.
    pub tests: Vec<WorkcartTest>,
    /// Flat optional local dependencies; the enclosing root cartridge is alias `main`.
    pub bundle: Vec<BundleSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkcartDocument {
    format: u16,
    cartridge: WorkcartCartridge,
    #[serde(default)]
    module: Vec<WorkcartModule>,
    #[serde(default)]
    asset: Vec<WorkcartAsset>,
    #[serde(default)]
    file: Vec<WorkcartFile>,
    #[serde(default)]
    recipe: Vec<WorkcartRecipe>,
    #[serde(default)]
    test: Vec<WorkcartTest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bundle: Vec<BundleSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkcartCartridge {
    language: String,
    id: String,
    title: String,
    author: String,
    version: String,
    entry: String,
    update_rate: u8,
    #[serde(default = "default_hardware_revision")]
    hardware_revision: u8,
    #[serde(default)]
    compile_on_load: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    display: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkcartModule {
    path: String,
    source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkcartAsset {
    name: String,
    kind: AssetKind,
    path: String,
    payload: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkcartFile {
    path: String,
    payload: String,
}

/// A deterministic map recipe retained as work-cart source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "algorithm", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkcartRecipe {
    /// A seeded palette of tile IDs distributed across a bounded map.
    Noise {
        id: String,
        output: String,
        seed: u64,
        width: u16,
        height: u16,
        tile_set: String,
        tiles: Vec<u16>,
    },
    /// A seeded Wave Function Collapse map inferred from a declared source map.
    Wfc {
        id: String,
        output: String,
        seed: u64,
        width: u16,
        height: u16,
        tile_set: String,
        source_map: String,
        pattern_size: u8,
    },
}

impl WorkcartRecipe {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Noise { id, .. } | Self::Wfc { id, .. } => id,
        }
    }

    #[must_use]
    pub fn output(&self) -> &str {
        match self {
            Self::Noise { output, .. } | Self::Wfc { output, .. } => output,
        }
    }

    #[must_use]
    pub const fn dimensions(&self) -> (u16, u16) {
        match self {
            Self::Noise { width, height, .. } | Self::Wfc { width, height, .. } => {
                (*width, *height)
            }
        }
    }
}

/// A source or replay test stored with its authored cartridge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkcartTest {
    pub name: String,
    pub path: String,
    pub kind: WorkcartTestKind,
    pub payload: String,
}

/// Test payload semantics supported by a work-cart.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkcartTestKind {
    Modl,
    Replay,
}

/// Parses and validates a source-visible `M01W/2` work-cart.
///
/// # Errors
///
/// Returns stable `M01402x` errors for malformed work-cart structure, duplicate authored
/// names, invalid paths, invalid assets, or inconsistent references.
pub fn parse_workcart(source: &str) -> Result<WorkcartProject, CartridgeError> {
    parse_workcart_inner(source, true)
}

fn parse_workcart_inner(
    source: &str,
    allow_bundle: bool,
) -> Result<WorkcartProject, CartridgeError> {
    if source.len() > MAX_WORKCART_DOCUMENT_BYTES {
        return Err(workcart_error(
            "M014072",
            "workcart document exceeds 128 MiB".to_owned(),
        ));
    }
    let document: WorkcartDocument = toml::from_str(source)
        .map_err(|error| workcart_error("M014020", format!("invalid .m01w: {error}")))?;
    if document.format != WORKCART_FORMAT_REVISION {
        return Err(workcart_error(
            "M014021",
            format!(
                "work-cart requests format revision {}; expected {WORKCART_FORMAT_REVISION}",
                document.format
            ),
        ));
    }
    if !allow_bundle && !document.bundle.is_empty() {
        return Err(workcart_error(
            "M014073",
            "bundle members cannot contain nested bundles".to_owned(),
        ));
    }
    if document.bundle.len() >= MAX_BUNDLE_CARTRIDGES {
        return Err(workcart_error(
            "M014070",
            "bundle allows at most 31 dependencies plus main".to_owned(),
        ));
    }
    let mut aliases = BTreeSet::new();
    let mut member_bytes = 0;
    for member in &document.bundle {
        if member.alias == BUNDLE_ENTRY
            || !valid_alias(&member.alias)
            || !aliases.insert(&member.alias)
        {
            return Err(workcart_error(
                "M014070",
                format!(
                    "invalid, reserved or duplicate bundle alias '{}'",
                    member.alias
                ),
            ));
        }
        member_bytes =
            checked_source_size(member_bytes, member.source.len()).map_err(|mut error| {
                error.message = format!("bundle cartridge '{}': {}", member.alias, error.message);
                error
            })?;
        parse_workcart_inner(&member.source, false).map_err(|mut error| {
            error.message = format!("bundle cartridge '{}': {}", member.alias, error.message);
            error
        })?;
    }
    let (assets, files) = materialize_files(&document)?;
    let manifest = manifest_from_cartridge(document.cartridge, assets)?;
    if !files.contains_key(&manifest.entry) {
        return Err(workcart_error(
            "M014024",
            format!("entry module '{}' is missing", manifest.entry),
        ));
    }
    validate_presentation_files(&manifest, &files)?;
    validate_recipes(&document.recipe, &manifest.assets)?;
    validate_tests(&document.test)?;
    Ok(WorkcartProject {
        source: source.to_owned(),
        manifest,
        files,
        recipes: document.recipe,
        tests: document.test,
        bundle: document.bundle,
    })
}

/// Canonically encodes a compiler-facing project as an `M01W/2` work-cart.
///
/// Declared assets, non-test MODL modules, and every other UTF-8 auxiliary file are emitted.
/// Tests and author-time recipes are supplied explicitly so they remain first-class work-cart
/// data rather than hidden editor state.
///
/// # Errors
///
/// Returns a stable error when an authored payload is missing, non-UTF-8, invalid, or cannot be
/// represented by the work-cart contract.
pub fn encode_workcart(
    manifest: &ProjectManifest,
    files: &BTreeMap<String, Vec<u8>>,
    recipes: &[WorkcartRecipe],
    tests: &[WorkcartTest],
) -> Result<String, CartridgeError> {
    encode_workcart_with_bundle(manifest, files, recipes, tests, &[])
}

/// Encodes the current workcart while retaining its flat source-visible dependencies.
///
/// # Errors
/// Returns ordinary workcart errors and qualified invalid/nested/oversized member diagnostics.
pub fn encode_workcart_with_bundle(
    manifest: &ProjectManifest,
    files: &BTreeMap<String, Vec<u8>>,
    recipes: &[WorkcartRecipe],
    tests: &[WorkcartTest],
    bundle: &[BundleSource],
) -> Result<String, CartridgeError> {
    let mut modules = Vec::new();
    let mut module_paths = BTreeSet::new();
    for (path, bytes) in files {
        if path.starts_with("tests/") || !has_extension(path, "modl") {
            continue;
        }
        require_path(path, "module")?;
        module_paths.insert(path.clone());
        modules.push(WorkcartModule {
            path: path.clone(),
            source: utf8_payload(bytes, path, "module")?,
        });
    }
    let mut assets = Vec::new();
    let mut asset_paths = BTreeSet::new();
    for (name, asset) in &manifest.assets {
        let bytes = files.get(&asset.path).ok_or_else(|| {
            workcart_error(
                "M014024",
                format!("asset '{name}' file '{}' is missing", asset.path),
            )
        })?;
        let payload = utf8_payload(bytes, &asset.path, "asset")?;
        let _: serde_json::Value = serde_json::from_str(&payload).map_err(|error| {
            workcart_error(
                "M014023",
                format!("asset '{name}' payload is not valid JSON: {error}"),
            )
        })?;
        asset_paths.insert(asset.path.clone());
        assets.push(WorkcartAsset {
            name: name.clone(),
            kind: asset.kind,
            path: asset.path.clone(),
            payload,
        });
    }
    let mut files_to_embed = Vec::new();
    for (path, bytes) in files {
        if path.starts_with("tests/") || module_paths.contains(path) || asset_paths.contains(path) {
            continue;
        }
        require_path(path, "file")?;
        files_to_embed.push(WorkcartFile {
            payload: utf8_payload(bytes, path, "file")?,
            path: path.clone(),
        });
    }
    let document = WorkcartDocument {
        format: WORKCART_FORMAT_REVISION,
        cartridge: WorkcartCartridge {
            hardware_revision: manifest.hardware_revision,
            language: manifest.language_revision.clone(),
            id: manifest.id.clone(),
            title: manifest.title.clone(),
            author: manifest.author.clone(),
            version: manifest.version.clone(),
            entry: manifest.entry.clone(),
            update_rate: manifest.update_rate,
            compile_on_load: manifest.compile_on_load,
            label: manifest.label.clone(),
            thumbnail: manifest.thumbnail.clone(),
            display: manifest.display.clone(),
        },
        module: modules,
        asset: assets,
        file: files_to_embed,
        recipe: recipes.to_vec(),
        test: tests.to_vec(),
        bundle: bundle.to_vec(),
    };
    let encoded = toml::to_string_pretty(&document)
        .map_err(|error| workcart_error("M014020", error.to_string()))?;
    parse_workcart(&encoded)?;
    Ok(encoded)
}

/// Returns the disposable compiler/editor view of the sole current workcart format.
/// Retain the original source and apply edits with [`rewrite_workcart`].
///
/// # Errors
///
/// Returns work-cart validation or manifest serialization errors.
pub fn workcart_project_view(source: &str) -> Result<UnpackedProject, CartridgeError> {
    let project = parse_workcart(source)?;
    let manifest = toml::to_string(&project.manifest)
        .map_err(|error| workcart_error("M014020", error.to_string()))?;
    Ok(UnpackedProject {
        manifest,
        files: project.files,
    })
}

/// Canonically applies a compiler-facing project edit to an existing `M01W/2` work-cart.
///
/// Recipes and tests remain unchanged; only the passed manifest and files are rewritten. This
/// makes editor views disposable while the work-cart remains the source of truth.
///
/// # Errors
///
/// Returns work-cart or manifest validation errors, including missing files introduced by an
/// editor update.
pub fn rewrite_workcart(
    source: &str,
    manifest_source: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<String, CartridgeError> {
    let original = parse_workcart(source)?;
    let manifest = parse_project_manifest(manifest_source)?;
    if manifest == original.manifest && files == &original.files {
        return Ok(source.to_owned());
    }
    encode_workcart_with_bundle(
        &manifest,
        files,
        &original.recipes,
        &original.tests,
        &original.bundle,
    )
}

/// Embeds one admitted local cartridge without replacing aliases or reading host paths.
/// Existing member source, recipes, tests and auxiliary files are preserved.
///
/// # Errors
/// Rejects invalid/reserved/duplicate aliases, nesting, duplicate save identities,
/// limits and any invalid member before returning a changed enclosing source.
pub fn add_workcart_member(
    source: &str,
    alias: &str,
    member_source: &str,
) -> Result<String, CartridgeError> {
    let mut project = parse_workcart(source)?;
    if alias == BUNDLE_ENTRY
        || !valid_alias(alias)
        || project.bundle.iter().any(|member| member.alias == alias)
    {
        return Err(workcart_error(
            "M014070",
            "new member needs an unused local alias other than main".to_owned(),
        ));
    }
    if project.bundle.len() + 1 >= MAX_BUNDLE_CARTRIDGES {
        return Err(workcart_error(
            "M014070",
            "game already has 32 cartridges including main".to_owned(),
        ));
    }
    checked_source_size(0, member_source.len())?;
    let member = parse_workcart(member_source)?;
    if !member.bundle.is_empty() {
        return Err(workcart_error(
            "M014073",
            "import one flat cartridge, not a game containing dependencies".to_owned(),
        ));
    }
    project.bundle.push(BundleSource {
        alias: alias.to_owned(),
        source: member_source.to_owned(),
    });
    let output = encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
        &project.bundle,
    )?;
    crate::game::pack_game_workcart(&output)?;
    Ok(output)
}

/// Removes a local dependency only when retained authored modules/tests cannot
/// request it. Computed request targets conservatively prevent removal: this is
/// not an attempt to predict runtime strings. Player saves are never removed.
///
/// # Errors
/// Returns M014075 for retained literal/computed references, M014070 for an
/// absent/reserved alias, and ordinary validation errors without changing input.
pub fn remove_workcart_member(source: &str, alias: &str) -> Result<String, CartridgeError> {
    let mut project = parse_workcart(source)?;
    if alias == BUNDLE_ENTRY || !project.bundle.iter().any(|member| member.alias == alias) {
        return Err(workcart_error(
            "M014070",
            "only a declared dependency can be removed; main is permanent".to_owned(),
        ));
    }
    check_member_removal_references(&project, BUNDLE_ENTRY, alias)?;
    for member in &project.bundle {
        if member.alias != alias {
            check_member_removal_references(
                &parse_workcart(&member.source)?,
                &member.alias,
                alias,
            )?;
        }
    }
    project.bundle.retain(|member| member.alias != alias);
    let output = encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
        &project.bundle,
    )?;
    crate::game::pack_game_workcart(&output)?;
    Ok(output)
}

fn check_member_removal_references(
    project: &WorkcartProject,
    owner: &str,
    removed: &str,
) -> Result<(), CartridgeError> {
    let sources = project
        .files
        .iter()
        .filter(|(path, _)| has_extension(path, "modl"))
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .chain(
            project
                .tests
                .iter()
                .filter(|test| test.kind == WorkcartTestKind::Modl)
                .map(|test| (test.path.as_str(), test.payload.as_bytes())),
        );
    for (path, bytes) in sources {
        let source = std::str::from_utf8(bytes).map_err(|_| {
            workcart_error(
                "M014075",
                format!("cannot prove safe removal: {owner}/{path} is not UTF-8"),
            )
        })?;
        let lexed = lex(&SourceFile::new(FileId(0), path, source));
        if !lexed.diagnostics.is_empty() {
            return Err(workcart_error(
                "M014075",
                format!(
                    "cannot prove safe removal: cartridge '{owner}' module/test '{path}' has lexical errors"
                ),
            ));
        }
        let tokens = lexed
            .tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::Comment(_)))
            .collect::<Vec<_>>();
        for (index, token) in tokens.iter().enumerate() {
            let TokenKind::Identifier(name) = &token.kind else {
                continue;
            };
            if !matches!(name.as_str(), "load_cart" | "reload_assets") {
                continue;
            }
            if !tokens
                .get(index + 1)
                .is_some_and(|token| token.kind == TokenKind::LeftParen)
            {
                return Err(workcart_error(
                    "M014075",
                    format!(
                        "cannot remove '{removed}': {owner}/{path} contains an indirect {name} reference; edit the request first"
                    ),
                ));
            }
            let target = match (tokens.get(index + 2), tokens.get(index + 3)) {
                (Some(target), Some(end))
                    if (name == "load_cart" && end.kind == TokenKind::Comma)
                        || (name == "reload_assets" && end.kind == TokenKind::RightParen) =>
                {
                    if let TokenKind::Text(alias) = &target.kind {
                        Some(alias.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if target.is_none() || target == Some(removed) {
                let line = 1 + source
                    .as_bytes()
                    .iter()
                    .take(token.span.start as usize)
                    .filter(|byte| **byte == b'\n')
                    .count();
                return Err(workcart_error(
                    "M014075",
                    format!(
                        "cannot remove '{removed}': {owner}/{path}:{line} contains {name} with {}",
                        if target.is_some() {
                            "that literal target"
                        } else {
                            "a computed target; edit the request first"
                        }
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Deletes an unreferenced asset from an `M01W/2` work-cart.
///
/// References from authored MODL modules, stored MODL tests, asset payloads and generation recipes prevent the
/// deletion. The returned work-cart is canonical and removes the asset payload when it is not
/// also needed as presentation data.
///
/// # Errors
///
/// Returns `M014028` when the asset does not exist or remains referenced, along with normal
/// work-cart validation and encoding errors.
pub fn delete_workcart_asset(source: &str, name: &str) -> Result<String, CartridgeError> {
    let mut project = parse_workcart(source)?;
    let Some(asset) = project.manifest.assets.get(name).cloned() else {
        return Err(workcart_error(
            "M014028",
            format!("asset '{name}' is not declared"),
        ));
    };
    let references = workcart_asset_references(&project, name);
    if !references.is_empty() {
        return Err(workcart_error(
            "M014028",
            format!(
                "asset '{name}' cannot be deleted; referenced by {}",
                references.join(", ")
            ),
        ));
    }

    project.manifest.assets.remove(name);
    let still_needed = project
        .manifest
        .assets
        .values()
        .any(|candidate| candidate.path == asset.path)
        || [
            &project.manifest.label,
            &project.manifest.thumbnail,
            &project.manifest.display,
        ]
        .into_iter()
        .flatten()
        .any(|path| path == &asset.path);
    if !still_needed {
        project.files.remove(&asset.path);
    }
    encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
        &project.bundle,
    )
}

fn workcart_asset_references(project: &WorkcartProject, name: &str) -> Vec<String> {
    let mut references = Vec::new();
    for (index, (path, payload)) in project
        .files
        .iter()
        .filter(|(path, _)| has_extension(path, "modl"))
        .enumerate()
    {
        let Some(source) = std::str::from_utf8(payload).ok() else {
            continue;
        };
        if source_references_asset(
            FileId(u32::try_from(index).unwrap_or(u32::MAX)),
            path,
            source,
            name,
        ) {
            references.push(format!("module '{path}'"));
        }
    }
    for (index, test) in project.tests.iter().enumerate() {
        if test.kind != WorkcartTestKind::Modl {
            continue;
        }
        let file_id = u32::try_from(index).unwrap_or(u32::MAX);
        if source_references_asset(FileId(file_id), &test.path, &test.payload, name) {
            references.push(format!("test '{}'", test.name));
        }
    }
    for (owner, declaration) in &project.manifest.assets {
        if owner == name || !matches!(declaration.kind, AssetKind::Map | AssetKind::Music) {
            continue;
        }
        let Some(payload) = project.files.get(&declaration.path) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(payload) else {
            continue;
        };
        let is_dependency = match declaration.kind {
            AssetKind::Map => value["layers"].as_array().is_some_and(|layers| {
                layers
                    .iter()
                    .any(|layer| layer["tileSet"].as_str() == Some(name))
            }),
            AssetKind::Music => value["patterns"].as_object().is_some_and(|patterns| {
                patterns.values().any(|pattern| {
                    pattern["rows"].as_array().is_some_and(|rows| {
                        rows.iter().any(|row| {
                            row.as_array().is_some_and(|cells| {
                                cells
                                    .iter()
                                    .any(|cell| cell["sound"].as_str() == Some(name))
                            })
                        })
                    })
                })
            }),
            _ => false,
        };
        if is_dependency {
            references.push(format!("asset '{owner}' ({})", declaration.path));
        }
    }
    for recipe in &project.recipes {
        let is_referenced = match recipe {
            WorkcartRecipe::Noise {
                output, tile_set, ..
            } => output == name || tile_set == name,
            WorkcartRecipe::Wfc {
                output,
                tile_set,
                source_map,
                ..
            } => output == name || tile_set == name || source_map == name,
        };
        if is_referenced {
            references.push(format!("recipe '{}'", recipe.id()));
        }
    }
    references
}

fn source_references_asset(file: FileId, path: &str, source: &str, name: &str) -> bool {
    lex(&SourceFile::new(file, path, source))
        .tokens
        .iter()
        .any(|token| matches!(&token.kind, TokenKind::Asset(candidate) if candidate == name))
}

fn materialize_files(document: &WorkcartDocument) -> Result<MaterializedFiles, CartridgeError> {
    let mut assets = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for module in &document.module {
        require_path(&module.path, "module")?;
        if !has_extension(&module.path, "modl") {
            return Err(workcart_error(
                "M014022",
                format!("module path '{}' must end in .modl", module.path),
            ));
        }
        insert_file(
            &mut files,
            &mut paths,
            &module.path,
            module.source.as_bytes(),
            "module",
        )?;
    }
    for asset in &document.asset {
        require_path(&asset.path, "asset")?;
        if assets
            .insert(
                asset.name.clone(),
                ProjectAsset {
                    kind: asset.kind,
                    path: asset.path.clone(),
                },
            )
            .is_some()
        {
            return Err(workcart_error(
                "M014022",
                format!("asset name '{}' is duplicated", asset.name),
            ));
        }
        let _: serde_json::Value = serde_json::from_str(&asset.payload).map_err(|error| {
            workcart_error(
                "M014023",
                format!("asset '{}' payload is not valid JSON: {error}", asset.name),
            )
        })?;
        insert_file(
            &mut files,
            &mut paths,
            &asset.path,
            asset.payload.as_bytes(),
            "asset",
        )?;
    }
    for file in &document.file {
        require_path(&file.path, "file")?;
        insert_file(
            &mut files,
            &mut paths,
            &file.path,
            file.payload.as_bytes(),
            "file",
        )?;
    }
    Ok((assets, files))
}

fn manifest_from_cartridge(
    cartridge: WorkcartCartridge,
    assets: BTreeMap<String, ProjectAsset>,
) -> Result<ProjectManifest, CartridgeError> {
    let manifest = ProjectManifest {
        format_revision: CARTRIDGE_FORMAT_REVISION,
        language_revision: cartridge.language,
        id: cartridge.id,
        title: cartridge.title,
        author: cartridge.author,
        version: cartridge.version,
        entry: cartridge.entry,
        update_rate: cartridge.update_rate,
        hardware_revision: cartridge.hardware_revision,
        compile_on_load: cartridge.compile_on_load,
        label: cartridge.label,
        thumbnail: cartridge.thumbnail,
        display: cartridge.display,
        assets,
    };
    let manifest_source =
        toml::to_string(&manifest).map_err(|error| workcart_error("M014020", error.to_string()))?;
    parse_project_manifest(&manifest_source)
}

const fn default_hardware_revision() -> u8 {
    2
}

fn validate_presentation_files(
    manifest: &ProjectManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), CartridgeError> {
    for path in [&manifest.label, &manifest.thumbnail, &manifest.display]
        .into_iter()
        .flatten()
    {
        if !files.contains_key(path) {
            return Err(workcart_error(
                "M014024",
                format!("presentation file '{path}' is missing"),
            ));
        }
    }
    Ok(())
}

fn validate_recipes(
    recipes: &[WorkcartRecipe],
    assets: &BTreeMap<String, ProjectAsset>,
) -> Result<(), CartridgeError> {
    let mut ids = BTreeSet::new();
    for recipe in recipes {
        if !ids.insert(recipe.id()) {
            return Err(workcart_error(
                "M014022",
                format!("recipe id '{}' is duplicated", recipe.id()),
            ));
        }
        let (width, height) = recipe.dimensions();
        if width == 0 || height == 0 || width > 256 || height > 256 {
            return Err(workcart_error(
                "M014025",
                format!("recipe '{}' dimensions must be within 1..=256", recipe.id()),
            ));
        }
        let output = assets.get(recipe.output());
        if !matches!(
            output,
            Some(ProjectAsset {
                kind: AssetKind::Map,
                ..
            })
        ) {
            return Err(workcart_error(
                "M014025",
                format!(
                    "recipe '{}' output '{}' is not a map asset",
                    recipe.id(),
                    recipe.output()
                ),
            ));
        }
        match recipe {
            WorkcartRecipe::Noise {
                tile_set, tiles, ..
            } => {
                if tiles.is_empty() {
                    return Err(workcart_error(
                        "M014025",
                        format!("noise recipe '{}' has no tile IDs", recipe.id()),
                    ));
                }
                require_tile_set(tile_set, recipe.id(), assets)?;
            }
            WorkcartRecipe::Wfc {
                tile_set,
                source_map,
                pattern_size,
                ..
            } => {
                if !(2..=8).contains(pattern_size) {
                    return Err(workcart_error(
                        "M014025",
                        format!(
                            "wfc recipe '{}' pattern_size must be within 2..=8",
                            recipe.id()
                        ),
                    ));
                }
                require_tile_set(tile_set, recipe.id(), assets)?;
                if !matches!(
                    assets.get(source_map),
                    Some(ProjectAsset {
                        kind: AssetKind::Map,
                        ..
                    })
                ) {
                    return Err(workcart_error(
                        "M014025",
                        format!(
                            "wfc recipe '{}' source_map '{source_map}' is not a map asset",
                            recipe.id()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn require_tile_set(
    name: &str,
    recipe_id: &str,
    assets: &BTreeMap<String, ProjectAsset>,
) -> Result<(), CartridgeError> {
    if matches!(
        assets.get(name),
        Some(ProjectAsset {
            kind: AssetKind::TileSet,
            ..
        })
    ) {
        Ok(())
    } else {
        Err(workcart_error(
            "M014025",
            format!("recipe '{recipe_id}' tile_set '{name}' is not a tile_set asset"),
        ))
    }
}

fn validate_tests(tests: &[WorkcartTest]) -> Result<(), CartridgeError> {
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for test in tests {
        if !names.insert(&test.name) {
            return Err(workcart_error(
                "M014022",
                format!("test name '{}' is duplicated", test.name),
            ));
        }
        require_path(&test.path, "test")?;
        if !paths.insert(&test.path) {
            return Err(workcart_error(
                "M014022",
                format!("test path '{}' is duplicated", test.path),
            ));
        }
        let valid_path = match test.kind {
            WorkcartTestKind::Modl => has_extension(&test.path, "modl"),
            WorkcartTestKind::Replay => Path::new(&test.path)
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".m01run.json")),
        };
        if !valid_path {
            return Err(workcart_error(
                "M014026",
                format!("test '{}' path does not match its kind", test.name),
            ));
        }
    }
    Ok(())
}

fn insert_file(
    files: &mut BTreeMap<String, Vec<u8>>,
    paths: &mut BTreeSet<String>,
    path: &str,
    bytes: &[u8],
    role: &str,
) -> Result<(), CartridgeError> {
    if !paths.insert(path.to_owned()) {
        return Err(workcart_error(
            "M014022",
            format!("{role} path '{path}' is duplicated"),
        ));
    }
    files.insert(path.to_owned(), bytes.to_vec());
    Ok(())
}

fn require_path(path: &str, role: &str) -> Result<(), CartridgeError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || !path.is_ascii()
        || path.len() > 1024
        || path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        return Err(workcart_error(
            "M014022",
            format!("{role} path '{path}' is not a canonical relative ASCII path"),
        ));
    }
    Ok(())
}

fn workcart_error(code: &'static str, message: String) -> CartridgeError {
    CartridgeError { code, message }
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|value| value == extension)
}

fn utf8_payload(bytes: &[u8], path: &str, role: &str) -> Result<String, CartridgeError> {
    std::str::from_utf8(bytes).map_or_else(
        |_| {
            Err(workcart_error(
                "M014023",
                format!("{role} file '{path}' is not valid UTF-8"),
            ))
        },
        |payload| Ok(payload.to_owned()),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn composition_add_remove_preserves_other_members_and_rejects_ambiguous_or_invalid_imports() {
        let source = bundled_fixture();
        let before = parse_workcart(&source).unwrap();
        let child = format!(
            "# imported exactly\r\n{}",
            CART.replace("workcart-test", "new-child")
                .replace('\n', "\r\n")
        );
        let added = super::add_workcart_member(&source, "bonus", &child).unwrap();
        let after = parse_workcart(&added).unwrap();
        assert_eq!(after.bundle[0], before.bundle[0]);
        assert_eq!(after.bundle[1].source, child);
        assert_eq!(after.files, before.files);
        assert_eq!(after.tests, before.tests);
        assert_eq!(after.recipes, before.recipes);
        assert_eq!(
            crate::game::pack_game_workcart(&added)
                .unwrap()
                .catalog()
                .cartridges()
                .len(),
            3
        );
        let removed = super::remove_workcart_member(&added, "bonus").unwrap();
        assert_eq!(parse_workcart(&removed).unwrap().bundle, before.bundle);
        for alias in ["main", "level", "../escape", "Upper", ""] {
            assert_eq!(
                super::add_workcart_member(&source, alias, &child)
                    .unwrap_err()
                    .code,
                "M014070"
            );
        }
        assert_eq!(
            super::add_workcart_member(&source, "duplicate-id", CART)
                .unwrap_err()
                .code,
            "M014071"
        );
        assert_eq!(
            super::add_workcart_member(&source, "nested", &added)
                .unwrap_err()
                .code,
            "M014073"
        );
        let mut invalid = parse_workcart(&child).unwrap();
        invalid.files.insert(
            "src/main.modl".to_owned(),
            b"on draw:\n  unknown_call()\n".to_vec(),
        );
        let invalid = super::encode_workcart(
            &invalid.manifest,
            &invalid.files,
            &invalid.recipes,
            &invalid.tests,
        )
        .unwrap();
        assert!(super::add_workcart_member(&source, "invalid", &invalid).is_err());
        for alias in ["main", "absent"] {
            assert_eq!(
                super::remove_workcart_member(&source, alias)
                    .unwrap_err()
                    .code,
                "M014070"
            );
        }
    }

    #[test]
    fn composition_removal_protects_modules_tests_and_computed_requests_but_not_comments_or_transfer_text()
     {
        let original = parse_workcart(&bundled_fixture()).unwrap();
        let fixture = |script: &str, test: bool| {
            let mut project = original.clone();
            if test {
                project.tests = vec![super::WorkcartTest {
                    name: "reference".to_owned(),
                    path: "tests/reference.modl".to_owned(),
                    kind: super::WorkcartTestKind::Modl,
                    payload: script.to_owned(),
                }];
            } else {
                project
                    .files
                    .insert("src/main.modl".to_owned(), script.as_bytes().to_vec());
            }
            super::encode_workcart_with_bundle(
                &project.manifest,
                &project.files,
                &project.recipes,
                &project.tests,
                &project.bundle,
            )
            .unwrap()
        };
        for request in [
            "load_cart(\"level\", \"\")",
            "reload_assets(\"level\")",
            "load_cart(cart_parameter(), \"\")",
            "load_cart(\"main\" + cart_parameter(), \"\")",
            "reload_assets((\"level\"))",
        ] {
            for test in [false, true] {
                let source = fixture(&format!("on update:\n  {request}\n"), test);
                let error = super::remove_workcart_member(&source, "level").unwrap_err();
                assert_eq!(error.code, "M014075", "{request}");
                assert!(error.message.contains(if test {
                    "main/tests/reference.modl:2"
                } else {
                    "main/src/main.modl:2"
                }));
            }
        }
        let indirect = fixture("on update:\n  let request = load_cart\n", false);
        assert_eq!(
            super::remove_workcart_member(&indirect, "level")
                .unwrap_err()
                .code,
            "M014075"
        );
        let harmless = fixture(
            "// load_cart(\"level\", \"\")\non update:\n  load_cart(\"main\", \"level\")\non draw:\n  print(\"reload_assets(level)\", 0, 0, 1)\n",
            false,
        );
        assert!(
            parse_workcart(&super::remove_workcart_member(&harmless, "level").unwrap())
                .unwrap()
                .bundle
                .is_empty()
        );
        // Calls inside an imported but unexecuted module are still authored references.
        let mut project = original.clone();
        project.files.insert(
            "src/unused.modl".to_owned(),
            b"fn unused():\n  reload_assets(\"level\")\n".to_vec(),
        );
        let source = super::encode_workcart_with_bundle(
            &project.manifest,
            &project.files,
            &project.recipes,
            &project.tests,
            &project.bundle,
        )
        .unwrap();
        assert!(
            super::remove_workcart_member(&source, "level")
                .unwrap_err()
                .message
                .contains("src/unused.modl")
        );
        // A retained sibling's request prevents removing the destination too.
        let mut sibling = parse_workcart(CART).unwrap();
        sibling.manifest.id = "retained-sibling".to_owned();
        sibling.files.insert(
            "src/main.modl".to_owned(),
            b"on update:\n  load_cart(\"level\", \"\")\n".to_vec(),
        );
        let sibling = super::encode_workcart(
            &sibling.manifest,
            &sibling.files,
            &sibling.recipes,
            &sibling.tests,
        )
        .unwrap();
        let source = super::add_workcart_member(&bundled_fixture(), "sibling", &sibling).unwrap();
        assert!(
            super::remove_workcart_member(&source, "level")
                .unwrap_err()
                .message
                .contains("sibling/src/main.modl:2")
        );
    }

    #[test]
    fn composition_accepts_member_32_and_refuses_member_33_without_replacing_anything() {
        let mut project =
            parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        for index in 1..31 {
            project.manifest.id = format!("compose-{index}");
            project.bundle.push(crate::bundle::BundleSource {
                alias: format!("level-{index}"),
                source: super::encode_workcart(&project.manifest, &project.files, &[], &[])
                    .unwrap(),
            });
        }
        project.manifest.id = "compose-main".to_owned();
        let source = super::encode_workcart_with_bundle(
            &project.manifest,
            &project.files,
            &[],
            &[],
            &project.bundle,
        )
        .unwrap();
        project.manifest.id = "compose-last".to_owned();
        let last = super::encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
        let source = super::add_workcart_member(&source, "last", &last).unwrap();
        assert_eq!(
            crate::game::pack_game_workcart(&source)
                .unwrap()
                .catalog()
                .cartridges()
                .len(),
            32
        );
        assert_eq!(
            super::add_workcart_member(&source, "excess", &last)
                .unwrap_err()
                .code,
            "M014070"
        );
    }

    fn bundled_fixture() -> String {
        let root = parse_workcart(CART).unwrap();
        let child = format!(
            "# EXACT CHILD COMMENTS\r\n{}",
            CART.replace("workcart-test", "child-game")
                .replace('\n', "\r\n")
        );
        super::encode_workcart_with_bundle(
            &root.manifest,
            &root.files,
            &root.recipes,
            &root.tests,
            &[crate::bundle::BundleSource {
                alias: "level".to_owned(),
                source: child,
            }],
        )
        .unwrap()
    }

    #[test]
    fn flat_bundle_members_survive_noop_editor_rewrite_asset_deletion_and_generation() {
        let source = bundled_fixture();
        let original = parse_workcart(&source).unwrap();
        assert!(
            original.bundle[0]
                .source
                .starts_with("# EXACT CHILD COMMENTS\r\n")
        );
        let manifest = toml::to_string(&original.manifest).unwrap();
        assert_eq!(
            crate::rewrite_workcart(&source, &manifest, &original.files).unwrap(),
            source
        );
        let mut files = original.files.clone();
        files.insert(
            "src/main.modl".to_owned(),
            b"on draw:\n  clear(26)\n".to_vec(),
        );
        let edited = crate::rewrite_workcart(&source, &manifest, &files).unwrap();
        let after = parse_workcart(&edited).unwrap();
        assert_eq!(after.bundle, original.bundle);
        assert_eq!(after.recipes, original.recipes);
        assert_eq!(after.tests, original.tests);
        let generated = crate::materialize_workcart_recipe(&edited, "noise-world").unwrap();
        assert_eq!(parse_workcart(&generated).unwrap().bundle, original.bundle);
        let mut manifest = original.manifest.clone();
        manifest.assets.insert(
            "spare".to_owned(),
            crate::ProjectAsset {
                kind: crate::AssetKind::TileSet,
                path: "assets/spare.m01g".to_owned(),
            },
        );
        files.insert(
            "assets/spare.m01g".to_owned(),
            files["assets/tiles.m01g"].clone(),
        );
        let with_spare = super::encode_workcart_with_bundle(
            &manifest,
            &files,
            &original.recipes,
            &original.tests,
            &original.bundle,
        )
        .unwrap();
        let deleted = crate::delete_workcart_asset(&with_spare, "spare").unwrap();
        let after = parse_workcart(&deleted).unwrap();
        assert_eq!(after.bundle, original.bundle);
        assert!(!after.manifest.assets.contains_key("spare"));
        // Until all game adapters are connected, the single-cart API must refuse
        // bundles rather than accidentally emit only main and lose dependencies.
        assert_eq!(crate::pack_workcart(&source).unwrap_err().code, "M014073");
    }

    #[test]
    fn flat_bundle_rejects_nested_reserved_duplicate_and_excess_members_without_recursing() {
        let root = parse_workcart(CART).unwrap();
        let member = crate::bundle::BundleSource {
            alias: "level".to_owned(),
            source: CART.to_owned(),
        };
        for members in [
            vec![crate::bundle::BundleSource {
                alias: "main".to_owned(),
                ..member.clone()
            }],
            vec![crate::bundle::BundleSource {
                alias: "../outside".to_owned(),
                ..member.clone()
            }],
            vec![member.clone(), member.clone()],
            vec![member.clone(); 32],
        ] {
            assert_eq!(
                super::encode_workcart_with_bundle(
                    &root.manifest,
                    &root.files,
                    &root.recipes,
                    &root.tests,
                    &members
                )
                .unwrap_err()
                .code,
                "M014070"
            );
        }
        let nested = crate::bundle::BundleSource {
            alias: "outer".to_owned(),
            source: bundled_fixture(),
        };
        let failure = super::encode_workcart_with_bundle(
            &root.manifest,
            &root.files,
            &root.recipes,
            &root.tests,
            &[nested],
        )
        .unwrap_err();
        assert_eq!(failure.code, "M014073");
        assert!(failure.message.contains("outer"));
    }

    #[test]
    fn bundle_parser_keeps_current_member_schema_and_qualifies_unavailable_source() {
        let root = parse_workcart(CART).unwrap();
        for child in [
            "not a workcart".to_owned(),
            CART.replacen("format = 2", "format = 1", 1),
        ] {
            let failure = super::encode_workcart_with_bundle(
                &root.manifest,
                &root.files,
                &root.recipes,
                &root.tests,
                &[crate::bundle::BundleSource {
                    alias: "broken".to_owned(),
                    source: child,
                }],
            )
            .unwrap_err();
            assert!(failure.message.contains("broken"));
        }
        let encoded = super::encode_workcart_with_bundle(
            &root.manifest,
            &root.files,
            &root.recipes,
            &root.tests,
            &[],
        )
        .unwrap();
        assert!(
            !encoded.contains("bundle ="),
            "ordinary canonical source must not change"
        );
    }

    use crate::{
        decode_cartridge, delete_workcart_asset, encode_workcart, pack_workcart, rewrite_workcart,
        unpack_cartridge_project, unpack_workcart, workcart_project_view,
    };

    use super::{WORKCART_FORMAT_REVISION, WorkcartRecipe, parse_workcart};

    const CART: &str = r#"format = 2

[cartridge]
language = "MODL/1"
id = "workcart-test"
title = "WORKCART TEST"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60
label = "presentation/label.svg"

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(25)\n"

[[asset]]
name = "tiles"
kind = "tile_set"
path = "assets/tiles.m01g"
payload = '''{"revision":2,"kind":"tile_set","tiles":[[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]],"flags":[0]}'''

[[asset]]
name = "world"
kind = "map"
path = "assets/world.m01m"
payload = '''{"revision":1,"kind":"map","layers":[{"width":1,"height":1,"cells":[0],"tileSet":"tiles"}]}'''

[[file]]
path = "presentation/label.svg"
payload = "<svg/>"

[[recipe]]
algorithm = "noise"
id = "noise-world"
output = "world"
seed = 1
width = 30
height = 18
tile_set = "tiles"
tiles = [0]

[[test]]
name = "smoke"
kind = "modl"
path = "tests/smoke.modl"
payload = "on start:\n  assert true\n"
"#;

    #[test]
    fn parses_a_source_visible_workcart() {
        let project = parse_workcart(CART).expect("work-cart parses");
        assert_eq!(WORKCART_FORMAT_REVISION, 2);
        assert_eq!(project.manifest.id, "workcart-test");
        assert_eq!(project.files.len(), 4);
        assert_eq!(project.files["src/main.modl"], b"on draw:\n  clear(25)\n");
        assert_eq!(project.source, CART);
        assert!(matches!(
            project.recipes.as_slice(),
            [WorkcartRecipe::Noise { .. }]
        ));
        assert_eq!(project.tests.len(), 1);
    }

    #[test]
    fn rejects_duplicate_paths_and_invalid_recipe_references() {
        let duplicate = CART.replace(
            "path = \"presentation/label.svg\"\npayload = \"<svg/>\"",
            "path = \"assets/world.m01m\"\npayload = \"<svg/>\"",
        );
        assert_eq!(parse_workcart(&duplicate).unwrap_err().code, "M014022");
        let invalid = CART.replace("output = \"world\"", "output = \"tiles\"");
        assert_eq!(parse_workcart(&invalid).unwrap_err().code, "M014025");
    }

    #[test]
    fn rejects_unknown_workcart_revisions() {
        assert_eq!(
            parse_workcart(&CART.replacen("format = 2", "format = 1", 1))
                .unwrap_err()
                .code,
            "M014021"
        );
    }

    #[test]
    fn packs_and_restores_exact_workcart_source() {
        let packed = pack_workcart(CART).expect("work-cart packs");
        let decoded = decode_cartridge(&packed.bytes).expect("packed cartridge decodes");
        assert_eq!(decoded.manifest.format_revision, WORKCART_FORMAT_REVISION);
        assert_eq!(decoded.entries["source/workcart.m01w"], CART.as_bytes());
        assert_eq!(
            unpack_workcart(&packed.bytes).expect("work-cart unpacks"),
            CART
        );
        let project = unpack_cartridge_project(&packed.bytes).expect("project view unpacks");
        assert!(project.manifest.contains("format = 2"));
        assert_eq!(project.files["src/main.modl"], b"on draw:\n  clear(25)\n");
    }

    #[test]
    fn canonical_writer_preserves_the_compiler_facing_workcart_contents() {
        let original = parse_workcart(CART).expect("original work-cart parses");
        let encoded = encode_workcart(
            &original.manifest,
            &original.files,
            &original.recipes,
            &original.tests,
        )
        .expect("work-cart writes");
        let reparsed = parse_workcart(&encoded).expect("encoded work-cart parses");
        assert_eq!(reparsed.manifest, original.manifest);
        assert_eq!(reparsed.files, original.files);
        assert_eq!(reparsed.recipes, original.recipes);
        assert_eq!(reparsed.tests, original.tests);
    }

    #[test]
    fn canonical_writer_preserves_auxiliary_utf8_files() {
        let original = parse_workcart(CART).expect("original work-cart parses");
        let mut files = original.files.clone();
        files.insert(
            "presentation/cartridge.json".to_owned(),
            br#"{"year":1999,"players":1}"#.to_vec(),
        );
        let encoded = encode_workcart(
            &original.manifest,
            &files,
            &original.recipes,
            &original.tests,
        )
        .expect("work-cart writes");
        let reparsed = parse_workcart(&encoded).expect("encoded work-cart parses");
        assert_eq!(
            reparsed.files["presentation/cartridge.json"],
            br#"{"year":1999,"players":1}"#
        );
    }

    #[test]
    fn compiler_views_can_be_rewritten_without_losing_workcart_metadata() {
        let view = workcart_project_view(CART).expect("work-cart view materializes");
        let rewritten =
            rewrite_workcart(CART, &view.manifest, &view.files).expect("work-cart view rewrites");
        let project = parse_workcart(&rewritten).expect("rewritten work-cart parses");
        assert_eq!(project.recipes.len(), 1);
        assert_eq!(project.tests.len(), 1);
        assert_eq!(project.files, view.files);
    }

    #[test]
    fn asset_deletion_refuses_module_test_and_recipe_references() {
        let module_reference = CART.replace("clear(25)", "draw #tiles, 0, 0");
        let error = delete_workcart_asset(&module_reference, "tiles").unwrap_err();
        assert_eq!(error.code, "M014028");
        assert!(error.message.contains("module 'src/main.modl'"));

        let test_reference = CART.replace("assert true", "draw #tiles, 0, 0");
        let error = delete_workcart_asset(&test_reference, "tiles").unwrap_err();
        assert_eq!(error.code, "M014028");
        assert!(error.message.contains("test 'smoke'"));

        let error = delete_workcart_asset(CART, "world").unwrap_err();
        assert_eq!(error.code, "M014028");
        assert!(error.message.contains("recipe 'noise-world'"));
    }

    #[test]
    fn asset_deletion_protects_map_tiles_and_notes_in_every_music_pattern() {
        let mut project =
            parse_workcart(include_str!("../../../examples/api-tour/cart.m01w")).unwrap();
        project.files.insert(
            project.manifest.entry.clone(),
            b"on draw:\n  clear(0)\n".to_vec(),
        );
        project.recipes.clear();
        project.tests.clear();
        let source = encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
        for (target, owner) in [("tiles", "room"), ("beep", "loop")] {
            let error = delete_workcart_asset(&source, target).unwrap_err();
            assert_eq!(error.code, "M014028");
            assert!(
                error.message.contains(&format!("asset '{owner}'")),
                "{error}"
            );
        }
        let without_music = delete_workcart_asset(&source, "loop").unwrap();
        let without_sound = delete_workcart_asset(&without_music, "beep").unwrap();
        let after = parse_workcart(&without_sound).unwrap();
        assert!(!after.manifest.assets.contains_key("loop"));
        assert!(!after.manifest.assets.contains_key("beep"));
        assert_eq!(
            after.files["assets/tiles.m01g"],
            project.files["assets/tiles.m01g"]
        );
        assert_eq!(
            after.files["src/motion.modl"],
            project.files["src/motion.modl"]
        );
    }

    #[test]
    fn asset_deletion_removes_an_unreferenced_payload() {
        const UNUSED: &str = r#"format = 2

[cartridge]
language = "MODL/1"
id = "unused-asset"
title = "UNUSED ASSET"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(25)\n"

[[asset]]
name = "unused"
kind = "sprite"
path = "assets/unused.m01g"
payload = '''{"revision":2,"kind":"sprite","width":8,"height":1,"frames":[[0,0,0,0,0,0,0,0]]}'''
"#;
        let deleted = delete_workcart_asset(UNUSED, "unused").expect("asset deletes");
        let project = parse_workcart(&deleted).expect("rewritten work-cart parses");
        assert!(!project.manifest.assets.contains_key("unused"));
        assert!(!project.files.contains_key("assets/unused.m01g"));
    }
}
