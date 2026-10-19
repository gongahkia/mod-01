use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    AssetKind, CARTRIDGE_FORMAT_REVISION, CartridgeError, FileId, ProjectAsset, ProjectManifest,
    SourceFile, TokenKind, UnpackedProject, lex, parse_project_manifest,
};

/// The authored work-cart revision supported by this compiler.
pub const WORKCART_FORMAT_REVISION: u16 = 2;

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
    #[serde(default = "default_hardware_revision", skip_serializing_if = "is_one")]
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
    };
    let encoded = toml::to_string_pretty(&document)
        .map_err(|error| workcart_error("M014020", error.to_string()))?;
    parse_workcart(&encoded)?;
    Ok(encoded)
}

/// Returns the legacy compiler project view materialized from an authored `M01W/2` work-cart.
///
/// This is a migration boundary for tools that still operate on manifest and file views. New
/// authoring should retain the original work-cart source and rewrite it with
/// [`rewrite_workcart`] after an edit.
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
    encode_workcart(&manifest, files, &original.recipes, &original.tests)
}

/// Deletes an unreferenced asset from an `M01W/2` work-cart.
///
/// References from authored MODL modules, stored MODL tests, and generation recipes prevent the
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
    encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
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
    1
}
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_one(value: &u8) -> bool {
    *value == 1
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
payload = '''{"revision":1,"kind":"tile_set","tiles":[[0,0,0,0,0,0,0,0]],"flags":[0]}'''

[[asset]]
name = "world"
kind = "map"
path = "assets/world.m01m"
payload = '''{"revision":1,"kind":"map","layers":[]}'''

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
        assert!(project.manifest.contains("format = 1"));
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
payload = '''{"revision":1,"kind":"sprite","frames":[[0,0,0,0,0,0,0,0]]}'''
"#;
        let deleted = delete_workcart_asset(UNUSED, "unused").expect("asset deletes");
        let project = parse_workcart(&deleted).expect("rewritten work-cart parses");
        assert!(!project.manifest.assets.contains_key("unused"));
        assert!(!project.files.contains_key("assets/unused.m01g"));
    }
}
