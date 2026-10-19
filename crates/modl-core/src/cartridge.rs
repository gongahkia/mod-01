use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write as _},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    AssetCatalog, AssetKind, CARTRIDGE_FORMAT_REVISION, CompilationOutput, CompileMode, FileId,
    LANGUAGE_REVISION, SourceFile, TokenKind,
    ast::{Item, Module},
    codegen::{SourceOriginMap, SourceOriginRange, compile_with_origins},
    compiler_version, parse,
};

const MAGIC: &[u8; 8] = b"MOD-01\x1a\x02";
pub const CARTRIDGE_CAPACITY_BYTES: usize = 1024 * 1024;
const MAX_UNPACKED_BYTES: usize = 16 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;

/// Resource bounds belong to the container, not a second hardware/format profile.
#[derive(Clone, Copy)]
pub(crate) struct ArchiveLimits {
    pub packed: usize,
    pub unpacked: usize,
    pub entries: usize,
}
pub(crate) const CARTRIDGE_ARCHIVE_LIMITS: ArchiveLimits = ArchiveLimits {
    packed: CARTRIDGE_CAPACITY_BYTES,
    unpacked: MAX_UNPACKED_BYTES,
    entries: MAX_ARCHIVE_ENTRIES,
};
const SHA256_INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];
const SHA256_ROUND: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    #[serde(rename = "format")]
    pub format_revision: u16,
    #[serde(rename = "language")]
    pub language_revision: String,
    pub id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub entry: String,
    pub update_rate: u8,
    #[serde(default = "default_hardware_revision")]
    pub hardware_revision: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compile_on_load: bool,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub display: Option<String>,
    #[serde(default)]
    pub assets: BTreeMap<String, ProjectAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAsset {
    pub kind: AssetKind,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackedManifest {
    pub format_revision: u16,
    pub language_revision: String,
    pub compiler_version: String,
    pub id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub entry: String,
    pub update_rate: u8,
    #[serde(default = "default_hardware_revision")]
    pub hardware_revision: u8,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compile_on_load: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled_program: Option<FileIntegrity>,
    pub label: Option<String>,
    pub thumbnail: Option<String>,
    pub display: Option<String>,
    pub assets: BTreeMap<String, PackedAsset>,
    pub files: BTreeMap<String, FileIntegrity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackedAsset {
    pub kind: AssetKind,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileIntegrity {
    pub bytes: u32,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackedCartridge {
    pub bytes: Vec<u8>,
    pub manifest: PackedManifest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DecodedCartridge {
    pub manifest: PackedManifest,
    pub entries: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnpackedProject {
    pub manifest: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartridgeError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "error[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for CartridgeError {}

/// Parses and validates a materialized compiler/editor metadata view, not an authoring format.
///
/// # Errors
///
/// Returns a stable `M014xxx` error when TOML decoding or manifest validation fails.
pub fn parse_project_manifest(source: &str) -> Result<ProjectManifest, CartridgeError> {
    let manifest: ProjectManifest = toml::from_str(source).map_err(|error| {
        cartridge_error("M014001", format!("invalid cartridge metadata: {error}"))
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Builds one canonical, compressed, source-inspectable `.m01c` byte sequence.
///
/// # Errors
///
/// Returns a stable `M014xxx` error for invalid manifests, missing files, compiler diagnostics,
/// archive collisions, or capacity violations.
pub fn pack_project(
    manifest_source: &str,
    project_files: &BTreeMap<String, Vec<u8>>,
) -> Result<PackedCartridge, CartridgeError> {
    let manifest = parse_project_manifest(manifest_source)?;
    let mut files = project_files.clone();
    for (path, bytes) in &mut files {
        if has_modl_extension(path) {
            *bytes = normalized_source(bytes, path)?.into_bytes();
        }
    }
    let source = crate::encode_workcart(&manifest, &files, &[], &[])?;
    pack_workcart(&source)
}

/// Packs an `M01W/2` work-cart and retains its exact source in the cartridge archive.
///
/// # Errors
///
/// Returns a stable work-cart, compilation, archive, or capacity error.
pub fn pack_workcart(source: &str) -> Result<PackedCartridge, CartridgeError> {
    let project = crate::parse_workcart(source)?;
    if !project.bundle.is_empty() {
        return Err(cartridge_error("M014073", "bundled workcarts require the whole-game packing path; refusing to discard dependencies".to_owned()));
    }
    pack_materialized_project(&project.manifest, &project.files, project.source.as_bytes())
}

fn pack_materialized_project(
    manifest: &ProjectManifest,
    project_files: &BTreeMap<String, Vec<u8>>,
    source: &[u8],
) -> Result<PackedCartridge, CartridgeError> {
    crate::assets::analyze_project_assets(manifest, project_files).require_valid()?;
    let normalized_entry = normalize_project_path(&manifest.entry)?;
    let mut asset_catalog = AssetCatalog::default();
    for (name, asset) in &manifest.assets {
        asset_catalog.insert(name.clone(), asset.kind);
    }
    let compilation = compile_linked_project(
        manifest,
        project_files,
        &asset_catalog,
        CompileMode::Release,
    )?;
    if let Some(diagnostic) = compilation.analysis.diagnostics.first() {
        return Err(cartridge_error(
            "M014003",
            format!(
                "entry source did not compile: error[{}] {}",
                diagnostic.code, diagnostic.message
            ),
        ));
    }
    let generated = compilation.generated.ok_or_else(|| {
        cartridge_error(
            "M014003",
            "entry source produced no release program".to_owned(),
        )
    })?;
    let compiled_program = Some(FileIntegrity {
        bytes: u32::try_from(generated.javascript.len()).unwrap_or(u32::MAX),
        sha256: sha256_hex(generated.javascript.as_bytes()),
    });

    let mut entries = BTreeMap::from([("source/workcart.m01w".to_owned(), source.to_vec())]);
    if !manifest.compile_on_load {
        entries.insert(
            "build/cartridge.js".to_owned(),
            generated.javascript.into_bytes(),
        );
        entries.insert(
            "build/cartridge.js.map".to_owned(),
            generated.source_map_json.into_bytes(),
        );
    }
    let packed_assets = manifest
        .assets
        .iter()
        .map(|(name, asset)| {
            (
                name.clone(),
                PackedAsset {
                    kind: asset.kind,
                    path: asset.path.clone(),
                },
            )
        })
        .collect();

    let files = entries
        .iter()
        .map(|(path, bytes)| {
            (
                path.clone(),
                FileIntegrity {
                    bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                    sha256: sha256_hex(bytes),
                },
            )
        })
        .collect();
    let packed_manifest = PackedManifest {
        format_revision: CARTRIDGE_FORMAT_REVISION,
        language_revision: LANGUAGE_REVISION.to_owned(),
        compiler_version: compiler_version().to_owned(),
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        author: manifest.author.clone(),
        version: manifest.version.clone(),
        entry: normalized_entry,
        update_rate: manifest.update_rate,
        hardware_revision: manifest.hardware_revision,
        compile_on_load: manifest.compile_on_load,
        compiled_program,
        label: manifest.label.clone(),
        thumbnail: manifest.thumbnail.clone(),
        display: manifest.display.clone(),
        assets: packed_assets,
        files,
    };
    let canonical_manifest = serde_json::to_vec(&packed_manifest).map_err(|error| {
        cartridge_error(
            "M014004",
            format!("could not encode packed manifest: {error}"),
        )
    })?;
    insert_unique(&mut entries, "manifest.json".to_owned(), canonical_manifest)?;
    let bytes = encode_archive(&entries)?;
    ensure_capacity(bytes.len())?;
    decode_cartridge(&bytes)?;
    Ok(PackedCartridge {
        bytes,
        manifest: packed_manifest,
    })
}

fn ensure_capacity(length: usize) -> Result<(), CartridgeError> {
    if length > CARTRIDGE_CAPACITY_BYTES {
        return Err(cartridge_error(
            "M014005",
            format!("packed cartridge is {length} bytes; capacity is {CARTRIDGE_CAPACITY_BYTES}"),
        ));
    }
    Ok(())
}

/// Links imported project modules and compiles them through the ordinary typed pipeline.
///
/// Imports use absolute dotted project paths: `import src.math as math` resolves
/// `src/math.modl`. Reachable modules are flattened in dependency order after checking that their
/// top-level names do not collide. This keeps generated JavaScript free of host module loading.
///
/// # Errors
///
/// Returns a stable `M014xxx` error for an invalid manifest, missing module, import cycle, parse
/// failure, unsupported dependency callback, or cross-module top-level name collision.
pub fn compile_project(
    manifest_source: &str,
    project_files: &BTreeMap<String, Vec<u8>>,
    mode: CompileMode,
) -> Result<CompilationOutput, CartridgeError> {
    let manifest = parse_project_manifest(manifest_source)?;
    let mut asset_catalog = AssetCatalog::default();
    for (name, asset) in &manifest.assets {
        asset_catalog.insert(name.clone(), asset.kind);
    }
    compile_linked_project(&manifest, project_files, &asset_catalog, mode)
}

fn compile_linked_project(
    manifest: &ProjectManifest,
    project_files: &BTreeMap<String, Vec<u8>>,
    assets: &AssetCatalog,
    mode: CompileMode,
) -> Result<CompilationOutput, CartridgeError> {
    let entry = normalize_project_path(&manifest.entry)?;
    let linked = link_project_sources(&entry, project_files)?;
    let origins = (mode == CompileMode::Debug).then_some(&linked.origins);
    Ok(compile_with_origins(&linked.source, assets, mode, origins))
}

#[derive(Clone, Debug)]
struct ProjectModule {
    file: FileId,
    text: String,
    syntax: Module,
}

/// Derives navigation, diagnostics and code/state relationships from the actual linked compiler IR.
///
/// # Errors
/// Returns a current metadata/link diagnostic when project structure is invalid.
pub fn analyze_project_model(
    manifest_source: &str,
    project_files: &BTreeMap<String, Vec<u8>>,
) -> Result<crate::tooling::ProjectModel, CartridgeError> {
    let manifest = parse_project_manifest(manifest_source)?;
    let model = crate::tooling::source_model(project_files)?;
    if !model.diagnostics.is_empty() {
        return Ok(model);
    }
    let linked = link_project_sources(&manifest.entry, project_files)?;
    let mut assets = AssetCatalog::default();
    for (name, asset) in &manifest.assets {
        assets.insert(name, asset.kind);
    }
    let analysis = crate::analyze_module(&linked.source, &assets);
    Ok(crate::tooling::ProjectModel::from_analysis(
        model.sources,
        &analysis,
        &linked.origins,
    ))
}

struct LinkedProject {
    source: SourceFile,
    origins: SourceOriginMap,
}

struct RenderedModule {
    text: String,
    offsets: Vec<u32>,
}

fn link_project_sources(
    entry: &str,
    project_files: &BTreeMap<String, Vec<u8>>,
) -> Result<LinkedProject, CartridgeError> {
    let mut modules = BTreeMap::new();
    for (index, (path, bytes)) in project_files
        .iter()
        .filter(|(path, _)| has_modl_extension(path))
        .enumerate()
    {
        let path = normalize_project_path(path)?;
        let text = normalized_source(bytes, &path)?;
        let source = SourceFile::new(
            FileId(u32::try_from(index).unwrap_or(u32::MAX)),
            path.clone(),
            text.clone(),
        );
        let parsed = parse(&source);
        if let Some(diagnostic) = parsed.diagnostics.first() {
            return Err(cartridge_error(
                "M014003",
                format!(
                    "module '{path}' did not parse: error[{}] {}",
                    diagnostic.code, diagnostic.message
                ),
            ));
        }
        modules.insert(
            path,
            ProjectModule {
                file: source.id(),
                text,
                syntax: parsed.module,
            },
        );
    }
    if !modules.contains_key(entry) {
        return Err(cartridge_error(
            "M014002",
            format!("entry source '{entry}' is missing"),
        ));
    }
    let mut visiting = Vec::new();
    let mut visited = BTreeSet::<String>::new();
    let mut order = Vec::new();
    visit_module(entry, &modules, &mut visiting, &mut visited, &mut order)?;
    validate_link_modules(entry, &order, &modules)?;
    let renames = module_renames(&order, &modules);

    let sources = modules
        .iter()
        .map(|(path, module)| SourceFile::new(module.file, path, module.text.clone()))
        .collect::<Vec<_>>();
    let mut linked = String::new();
    let mut ranges = Vec::new();
    for path in order {
        let module = modules.get(&path).expect("visited modules exist");
        let rendered = render_module(&path, module, &modules, &renames)?;
        let linked_start = u32::try_from(linked.len()).unwrap_or(u32::MAX);
        linked.push_str(&rendered.text);
        let linked_end = u32::try_from(linked.len()).unwrap_or(u32::MAX);
        ranges.push(SourceOriginRange {
            linked_start,
            linked_end,
            file: module.file,
            offsets: rendered.offsets,
        });
        if !linked.ends_with('\n') {
            linked.push('\n');
        }
        linked.push('\n');
    }
    Ok(LinkedProject {
        source: SourceFile::new(FileId(u32::MAX), format!("project/{entry}"), linked),
        origins: SourceOriginMap::new(sources, ranges),
    })
}

fn visit_module(
    path: &str,
    modules: &BTreeMap<String, ProjectModule>,
    visiting: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) -> Result<(), CartridgeError> {
    if visited.contains(path) {
        return Ok(());
    }
    if visiting.iter().any(|candidate| candidate == path) {
        visiting.push(path.to_owned());
        return Err(cartridge_error(
            "M014008",
            format!("module import cycle: {}", visiting.join(" -> ")),
        ));
    }
    let module = modules.get(path).ok_or_else(|| {
        cartridge_error("M014008", format!("imported module '{path}' is missing"))
    })?;
    visiting.push(path.to_owned());
    for item in &module.syntax.items {
        if let Item::Import(import) = item {
            let imported = import_path(import.path.iter().map(|part| part.value.as_str()))?;
            visit_module(&imported, modules, visiting, visited, order)?;
        }
    }
    visiting.pop();
    visited.insert(path.to_owned());
    order.push(path.to_owned());
    Ok(())
}

fn validate_link_modules(
    entry: &str,
    order: &[String],
    modules: &BTreeMap<String, ProjectModule>,
) -> Result<(), CartridgeError> {
    for path in order {
        let module = modules.get(path).expect("visited modules exist");
        if path != entry
            && module
                .syntax
                .items
                .iter()
                .any(|item| matches!(item, Item::Callback(_)))
        {
            return Err(cartridge_error(
                "M014009",
                format!("imported module '{path}' declares a system callback"),
            ));
        }
    }
    Ok(())
}

fn module_renames(
    order: &[String],
    modules: &BTreeMap<String, ProjectModule>,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut counts = BTreeMap::<String, usize>::new();
    for path in order {
        for name in module_declarations(&modules[path].syntax) {
            *counts.entry(name).or_default() += 1;
        }
    }
    order
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let names = module_declarations(&modules[path].syntax)
                .into_iter()
                .filter(|name| counts.get(name).copied().unwrap_or_default() > 1)
                .map(|name| {
                    let renamed = format!("__m01m{index}_{name}");
                    (name, renamed)
                })
                .collect();
            (path.clone(), names)
        })
        .collect()
}

fn render_module(
    path: &str,
    module: &ProjectModule,
    modules: &BTreeMap<String, ProjectModule>,
    renames: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<RenderedModule, CartridgeError> {
    let mut replacements = Vec::<(usize, usize, String)>::new();
    let mut aliases = BTreeMap::<String, (String, BTreeSet<String>)>::new();
    for item in &module.syntax.items {
        if let Item::Import(import) = item {
            let imported_path = import_path(import.path.iter().map(|part| part.value.as_str()))?;
            let imported = modules.get(&imported_path).ok_or_else(|| {
                cartridge_error(
                    "M014008",
                    format!("module '{path}' imports missing module '{imported_path}'"),
                )
            })?;
            let alias = import
                .alias
                .as_ref()
                .or_else(|| import.path.last())
                .map(|name| name.value.clone())
                .ok_or_else(|| cartridge_error("M014008", "empty import path".to_owned()))?;
            if aliases
                .insert(
                    alias.clone(),
                    (
                        imported_path,
                        module_exports(&imported.syntax).into_iter().collect(),
                    ),
                )
                .is_some()
            {
                return Err(cartridge_error(
                    "M014009",
                    format!("module '{path}' repeats import alias '{alias}'"),
                ));
            }
            replacements.push((
                usize::try_from(import.span.start).unwrap_or(usize::MAX),
                usize::try_from(import.span.end).unwrap_or(usize::MAX),
                String::new(),
            ));
        }
    }
    let source = SourceFile::new(FileId(0), path, module.text.clone());
    let tokens = crate::lex(&source).tokens;
    for window in tokens.windows(3) {
        let TokenKind::Identifier(alias) = &window[0].kind else {
            continue;
        };
        if !matches!(window[1].kind, TokenKind::Dot) {
            continue;
        }
        let TokenKind::Identifier(member) = &window[2].kind else {
            continue;
        };
        let Some((imported_path, exports)) = aliases.get(alias) else {
            continue;
        };
        if !exports.contains(member) {
            return Err(cartridge_error(
                "M014009",
                format!("module '{imported_path}' has no exported member '{member}'"),
            ));
        }
        replacements.push((
            usize::try_from(window[0].span.start).unwrap_or(usize::MAX),
            usize::try_from(window[2].span.end).unwrap_or(usize::MAX),
            renames
                .get(imported_path)
                .and_then(|names| names.get(member))
                .unwrap_or(member)
                .clone(),
        ));
    }
    add_namespace_replacements(&tokens, &mut replacements, &renames[path], &module.syntax);
    replacements.sort_by_key(|(start, end, _)| (*start, *end));
    render_replacements(path, &module.text, replacements)
}

fn render_replacements(
    path: &str,
    source: &str,
    replacements: Vec<(usize, usize, String)>,
) -> Result<RenderedModule, CartridgeError> {
    let mut output = String::new();
    let mut offsets = vec![0];
    let mut cursor = 0;
    for (start, end, replacement) in replacements {
        if start < cursor || start > end {
            return Err(cartridge_error(
                "M014009",
                format!("module '{path}' produced an invalid link span"),
            ));
        }
        append_mapped_source(&mut output, &mut offsets, source, cursor, start, path)?;
        let Some(_) = source.get(start..end) else {
            return Err(cartridge_error(
                "M014009",
                format!("module '{path}' produced an invalid link span"),
            ));
        };
        output.push_str(&replacement);
        if replacement.is_empty()
            && let Some(offset) = offsets.last_mut()
        {
            *offset = u32::try_from(end).unwrap_or(u32::MAX);
        }
        for index in 1..=replacement.len() {
            let original = start
                + (end - start)
                    .saturating_mul(index)
                    .checked_div(replacement.len())
                    .unwrap_or_default();
            offsets.push(u32::try_from(original).unwrap_or(u32::MAX));
        }
        cursor = end;
    }
    append_mapped_source(
        &mut output,
        &mut offsets,
        source,
        cursor,
        source.len(),
        path,
    )?;
    Ok(RenderedModule {
        text: output,
        offsets,
    })
}

fn append_mapped_source(
    output: &mut String,
    offsets: &mut Vec<u32>,
    source: &str,
    start: usize,
    end: usize,
    path: &str,
) -> Result<(), CartridgeError> {
    let Some(text) = source.get(start..end) else {
        return Err(cartridge_error(
            "M014009",
            format!("module '{path}' produced an invalid link span"),
        ));
    };
    output.push_str(text);
    offsets.extend((start + 1..=end).map(|offset| u32::try_from(offset).unwrap_or(u32::MAX)));
    Ok(())
}

fn add_namespace_replacements(
    tokens: &[crate::Token],
    replacements: &mut Vec<(usize, usize, String)>,
    own_names: &BTreeMap<String, String>,
    syntax: &Module,
) {
    let local_scopes = local_scopes(syntax);
    for (index, token) in tokens.iter().enumerate() {
        let TokenKind::Identifier(name) = &token.kind else {
            continue;
        };
        let Some(linked_name) = own_names.get(name) else {
            continue;
        };
        if replacements.iter().any(|(start, end, _)| {
            *start <= usize::try_from(token.span.start).unwrap_or(usize::MAX)
                && usize::try_from(token.span.end).unwrap_or(usize::MAX) <= *end
        }) {
            continue;
        }
        let preceded_by_dot = tokens[..index]
            .iter()
            .rev()
            .find(|candidate| {
                !matches!(
                    candidate.kind,
                    TokenKind::Comment(_)
                        | TokenKind::Newline
                        | TokenKind::Indent
                        | TokenKind::Dedent
                )
            })
            .is_some_and(|previous| matches!(previous.kind, TokenKind::Dot));
        if preceded_by_dot
            || local_scopes.iter().any(|scope| {
                scope.name == *name
                    && scope.start <= token.span.start
                    && token.span.start < scope.end
            })
        {
            continue;
        }
        replacements.push((
            usize::try_from(token.span.start).unwrap_or(usize::MAX),
            usize::try_from(token.span.end).unwrap_or(usize::MAX),
            linked_name.clone(),
        ));
    }
}

fn module_exports(module: &Module) -> Vec<String> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Constant(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::State(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::Function(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::Task(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::Record(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::Enum(value) if value.visibility == crate::ast::Visibility::Public => {
                Some(value.name.value.clone())
            }
            Item::Constant(_)
            | Item::State(_)
            | Item::Function(_)
            | Item::Task(_)
            | Item::Record(_)
            | Item::Enum(_)
            | Item::Import(_)
            | Item::Callback(_)
            | Item::Assertion(_) => None,
        })
        .collect()
}

fn module_declarations(module: &Module) -> Vec<String> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Constant(value) => Some(value.name.value.clone()),
            Item::State(value) => Some(value.name.value.clone()),
            Item::Function(value) => Some(value.name.value.clone()),
            Item::Task(value) => Some(value.name.value.clone()),
            Item::Record(value) => Some(value.name.value.clone()),
            Item::Enum(value) => Some(value.name.value.clone()),
            Item::Import(_) | Item::Callback(_) | Item::Assertion(_) => None,
        })
        .collect()
}

#[derive(Clone, Debug)]
struct LocalScope {
    name: String,
    start: u32,
    end: u32,
}

fn local_scopes(module: &Module) -> Vec<LocalScope> {
    let mut scopes = Vec::new();
    for item in &module.items {
        match item {
            Item::Function(value) => {
                add_parameters(&mut scopes, &value.parameters, value.span.end);
                add_block_scopes(&mut scopes, &value.body, value.span.end);
            }
            Item::Task(value) => {
                add_parameters(&mut scopes, &value.parameters, value.span.end);
                add_block_scopes(&mut scopes, &value.body, value.span.end);
            }
            Item::Callback(value) => {
                add_parameters(&mut scopes, &value.parameters, value.span.end);
                add_block_scopes(&mut scopes, &value.body, value.span.end);
            }
            Item::Import(_)
            | Item::Constant(_)
            | Item::State(_)
            | Item::Record(_)
            | Item::Enum(_)
            | Item::Assertion(_) => {}
        }
    }
    scopes
}

fn add_parameters(scopes: &mut Vec<LocalScope>, parameters: &[crate::ast::Parameter], end: u32) {
    scopes.extend(parameters.iter().map(|parameter| LocalScope {
        name: parameter.name.value.clone(),
        start: parameter.name.span.start,
        end,
    }));
}

fn add_block_scopes(scopes: &mut Vec<LocalScope>, block: &[crate::ast::Statement], end: u32) {
    use crate::ast::{PatternKind, StatementKind};
    for statement in block {
        match &statement.kind {
            StatementKind::Let { name, .. } => scopes.push(LocalScope {
                name: name.value.clone(),
                start: name.span.start,
                end,
            }),
            StatementKind::If(value) => {
                for branch in &value.branches {
                    add_block_scopes(scopes, &branch.body, branch.span.end);
                }
                if let Some(body) = &value.else_body {
                    add_block_scopes(scopes, body, block_end(body, statement.span.end));
                }
            }
            StatementKind::While { body, .. } => {
                add_block_scopes(scopes, body, block_end(body, statement.span.end));
            }
            StatementKind::For { binding, body, .. } => {
                let body_end = block_end(body, statement.span.end);
                scopes.push(LocalScope {
                    name: binding.value.clone(),
                    start: binding.span.start,
                    end: body_end,
                });
                add_block_scopes(scopes, body, body_end);
            }
            StatementKind::Match(value) => {
                for arm in &value.arms {
                    let body_end = block_end(&arm.body, arm.span.end);
                    match &arm.pattern.kind {
                        PatternKind::Binding(name) => scopes.push(LocalScope {
                            name: name.value.clone(),
                            start: name.span.start,
                            end: body_end,
                        }),
                        PatternKind::Variant { bindings, .. } => {
                            scopes.extend(bindings.iter().map(|name| LocalScope {
                                name: name.value.clone(),
                                start: name.span.start,
                                end: body_end,
                            }));
                        }
                        PatternKind::Wildcard | PatternKind::Literal(_) => {}
                    }
                    add_block_scopes(scopes, &arm.body, body_end);
                }
            }
            StatementKind::Assignment { .. }
            | StatementKind::Expression(_)
            | StatementKind::Return(_)
            | StatementKind::Break
            | StatementKind::Continue
            | StatementKind::Wait(_)
            | StatementKind::Start(_)
            | StatementKind::Assert(_) => {}
        }
    }
}

fn block_end(block: &[crate::ast::Statement], fallback: u32) -> u32 {
    block
        .last()
        .map_or(fallback, |statement| statement.span.end)
}

fn import_path<'a>(parts: impl Iterator<Item = &'a str>) -> Result<String, CartridgeError> {
    let path = format!("{}.modl", parts.collect::<Vec<_>>().join("/"));
    normalize_project_path(&path)
}

/// Decodes an untrusted `.m01c`, enforcing bounds, hashes, ordering, and manifest integrity.
///
/// # Errors
///
/// Returns a stable `M014xxx` error for malformed, oversized, non-canonical, or corrupted input.
pub fn decode_cartridge(bytes: &[u8]) -> Result<DecodedCartridge, CartridgeError> {
    let entries = decode_archive(bytes, CARTRIDGE_ARCHIVE_LIMITS)?;
    let manifest_bytes = entries.get("manifest.json").ok_or_else(|| {
        cartridge_error("M014012", "cartridge has no canonical manifest".to_owned())
    })?;
    let manifest: PackedManifest = serde_json::from_slice(manifest_bytes).map_err(|error| {
        cartridge_error("M014012", format!("packed manifest is invalid: {error}"))
    })?;
    if serde_json::to_vec(&manifest).ok().as_deref() != Some(manifest_bytes) {
        return Err(cartridge_error(
            "M014012",
            "packed manifest is not canonical JSON".to_owned(),
        ));
    }
    validate_packed_manifest(&manifest, &entries)?;
    Ok(DecodedCartridge { manifest, entries })
}

pub(crate) fn decode_archive(
    bytes: &[u8],
    limits: ArchiveLimits,
) -> Result<BTreeMap<String, Vec<u8>>, CartridgeError> {
    if bytes.len() > limits.packed {
        return Err(cartridge_error(
            "M014010",
            "cartridge exceeds the packed capacity".to_owned(),
        ));
    }
    let mut reader = Reader::new(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(cartridge_error(
            "M014010",
            "cartridge magic or revision is invalid".to_owned(),
        ));
    }
    let entry_count = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
    if entry_count == 0 || entry_count > limits.entries {
        return Err(cartridge_error(
            "M014010",
            "cartridge entry count is outside format limits".to_owned(),
        ));
    }
    let mut entries = BTreeMap::new();
    let mut previous_path: Option<String> = None;
    let mut unpacked_bytes = 0_usize;
    for _ in 0..entry_count {
        let path_length = usize::from(reader.u16()?);
        let raw_length = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
        let encoded_length = usize::try_from(reader.u32()?).unwrap_or(usize::MAX);
        let expected_hash: [u8; 32] = reader
            .take(32)?
            .try_into()
            .map_err(|_| cartridge_error("M014010", "invalid entry hash length".to_owned()))?;
        let path = std::str::from_utf8(reader.take(path_length)?)
            .map_err(|_| cartridge_error("M014010", "entry path is not UTF-8".to_owned()))?;
        let path = normalize_archive_path(path)?;
        if previous_path
            .as_ref()
            .is_some_and(|previous| previous >= &path)
        {
            return Err(cartridge_error(
                "M014010",
                "cartridge entries are not strictly sorted".to_owned(),
            ));
        }
        unpacked_bytes = unpacked_bytes
            .checked_add(raw_length)
            .ok_or_else(|| cartridge_error("M014010", "unpacked size overflow".to_owned()))?;
        if unpacked_bytes > limits.unpacked {
            return Err(cartridge_error(
                "M014010",
                "cartridge expands beyond the decoder limit".to_owned(),
            ));
        }
        let encoded = reader.take(encoded_length)?;
        let decoded = rle_decode(encoded, raw_length)?;
        if rle_encode(&decoded) != encoded {
            return Err(cartridge_error(
                "M014010",
                format!("entry '{path}' does not use canonical compression"),
            ));
        }
        if sha256(&decoded) != expected_hash {
            return Err(cartridge_error(
                "M014011",
                format!("integrity hash failed for '{path}'"),
            ));
        }
        previous_path = Some(path.clone());
        entries.insert(path, decoded);
    }
    if !reader.finished() {
        return Err(cartridge_error(
            "M014010",
            "cartridge contains trailing bytes".to_owned(),
        ));
    }
    Ok(entries)
}

/// Reconstructs the Git-friendly project representation from a validated cartridge.
///
/// # Errors
///
/// Returns a stable cartridge error when archive paths cannot be mapped back to project paths.
pub fn unpack_cartridge_project(bytes: &[u8]) -> Result<UnpackedProject, CartridgeError> {
    let source = unpack_workcart(bytes)?;
    crate::workcart_project_view(&source)
}

/// Restores the exact authored work-cart from a validated revision-2 cartridge.
///
/// # Errors
///
/// Returns a stable error when the cartridge is not revision 2 or has no valid work-cart source.
pub fn unpack_workcart(bytes: &[u8]) -> Result<String, CartridgeError> {
    let cartridge = decode_cartridge(bytes)?;
    let source = cartridge
        .entries
        .get("source/workcart.m01w")
        .ok_or_else(|| {
            cartridge_error(
                "M014027",
                "revision-2 cartridge is missing source/workcart.m01w".to_owned(),
            )
        })?;
    let source = std::str::from_utf8(source).map_err(|_| {
        cartridge_error(
            "M014027",
            "revision-2 work-cart source is not valid UTF-8".to_owned(),
        )
    })?;
    crate::parse_workcart(source)?;
    Ok(source.to_owned())
}

/// Validates a cartridge and returns the authoritative recompiled release program.
///
/// # Errors
///
/// Rejects malformed archives, compilation diagnostics, or a generated program that does not match
/// the identity recorded when the cartridge was packed.
pub fn load_cartridge_program(bytes: &[u8]) -> Result<(DecodedCartridge, Vec<u8>), CartridgeError> {
    let cartridge = decode_cartridge(bytes)?;
    let project = unpack_cartridge_project(bytes)?;
    let compilation = compile_project(&project.manifest, &project.files, CompileMode::Release)?;
    if let Some(diagnostic) = compilation.analysis.diagnostics.first() {
        return Err(cartridge_error(
            "M014015",
            format!(
                "cartridge source failed: error[{}] {}",
                diagnostic.code, diagnostic.message
            ),
        ));
    }
    let generated = compilation
        .generated
        .ok_or_else(|| cartridge_error("M014015", "source produced no program".to_owned()))?;
    let program = generated.javascript.into_bytes();
    if !cartridge.manifest.compile_on_load
        && (cartridge.entries.get("build/cartridge.js") != Some(&program)
            || cartridge
                .entries
                .get("build/cartridge.js.map")
                .map(Vec::as_slice)
                != Some(generated.source_map_json.as_bytes()))
    {
        return Err(cartridge_error(
            "M014015",
            "archived build differs from the authoritative source compilation".to_owned(),
        ));
    }
    let expected = cartridge
        .manifest
        .compiled_program
        .as_ref()
        .ok_or_else(|| {
            cartridge_error(
                "M014015",
                "source cartridge has no build identity".to_owned(),
            )
        })?;
    if usize::try_from(expected.bytes).ok() != Some(program.len())
        || expected.sha256 != sha256_hex(&program)
    {
        return Err(cartridge_error(
            "M014015",
            "compiled program does not match the cartridge build identity".to_owned(),
        ));
    }
    Ok((cartridge, program))
}

fn validate_manifest(manifest: &ProjectManifest) -> Result<(), CartridgeError> {
    if manifest.format_revision != CARTRIDGE_FORMAT_REVISION {
        return Err(cartridge_error(
            "M014006",
            format!(
                "cartridge metadata requests format revision {}; expected {CARTRIDGE_FORMAT_REVISION}",
                manifest.format_revision
            ),
        ));
    }
    if manifest.hardware_revision != 2 {
        return Err(cartridge_error(
            "M014006",
            "hardware_revision must be 2".to_owned(),
        ));
    }
    if manifest.language_revision != LANGUAGE_REVISION {
        return Err(cartridge_error(
            "M014006",
            format!(
                "cartridge metadata requests language '{}'; expected '{LANGUAGE_REVISION}'",
                manifest.language_revision
            ),
        ));
    }
    if !valid_cartridge_id(&manifest.id) {
        return Err(cartridge_error(
            "M014006",
            "cartridge id must be 3-64 lowercase ASCII letters, digits, dots, or hyphens"
                .to_owned(),
        ));
    }
    if manifest.title.is_empty()
        || manifest.title.len() > 64
        || manifest.author.is_empty()
        || manifest.author.len() > 64
        || manifest.version.is_empty()
        || manifest.version.len() > 32
    {
        return Err(cartridge_error(
            "M014006",
            "title, author, or version is empty or too long".to_owned(),
        ));
    }
    if manifest.update_rate != 30 && manifest.update_rate != 60 {
        return Err(cartridge_error(
            "M014006",
            "update_rate must be 30 or 60".to_owned(),
        ));
    }
    let entry = normalize_project_path(&manifest.entry)?;
    if !has_modl_extension(&entry) {
        return Err(cartridge_error(
            "M014006",
            "entry must name a .modl source file".to_owned(),
        ));
    }
    for (name, asset) in &manifest.assets {
        if !valid_identifier(name) {
            return Err(cartridge_error(
                "M014006",
                format!("asset name '{name}' is not a MODL identifier"),
            ));
        }
        normalize_project_path(&asset.path)?;
    }
    if let Some(path) = &manifest.label {
        normalize_project_path(path)?;
    }
    if let Some(path) = &manifest.thumbnail {
        normalize_project_path(path)?;
    }
    if let Some(path) = &manifest.display {
        normalize_project_path(path)?;
    }
    Ok(())
}

fn validate_packed_manifest(
    manifest: &PackedManifest,
    entries: &BTreeMap<String, Vec<u8>>,
) -> Result<(), CartridgeError> {
    let invalid = |message: &str| cartridge_error("M014012", message.to_owned());
    if manifest.format_revision != CARTRIDGE_FORMAT_REVISION
        || manifest.hardware_revision != 2
        || manifest.language_revision != LANGUAGE_REVISION
        || manifest.compiler_version != compiler_version()
    {
        return Err(invalid(
            "packed manifest requests an unsupported current contract",
        ));
    }
    let actual_files: BTreeMap<_, _> = entries
        .iter()
        .filter(|(path, _)| path.as_str() != "manifest.json")
        .map(|(path, bytes)| {
            (
                path.clone(),
                FileIntegrity {
                    bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                    sha256: sha256_hex(bytes),
                },
            )
        })
        .collect();
    if actual_files != manifest.files {
        return Err(invalid(
            "packed manifest file inventory differs from archive",
        ));
    }
    let source = entries
        .get("source/workcart.m01w")
        .ok_or_else(|| invalid("missing source/workcart.m01w"))?;
    let source =
        std::str::from_utf8(source).map_err(|_| invalid("workcart source is not UTF-8"))?;
    let project = crate::parse_workcart(source)?;
    let metadata = &project.manifest;
    crate::assets::analyze_project_assets(metadata, &project.files).require_valid()?;
    let assets = metadata
        .assets
        .iter()
        .map(|(name, asset)| {
            (
                name.clone(),
                PackedAsset {
                    kind: asset.kind,
                    path: asset.path.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    if manifest.id != metadata.id
        || manifest.title != metadata.title
        || manifest.author != metadata.author
        || manifest.version != metadata.version
        || manifest.entry != metadata.entry
        || manifest.update_rate != metadata.update_rate
        || manifest.compile_on_load != metadata.compile_on_load
        || manifest.label != metadata.label
        || manifest.thumbnail != metadata.thumbnail
        || manifest.display != metadata.display
        || manifest.assets != assets
    {
        return Err(invalid(
            "packed metadata disagrees with its authoritative workcart",
        ));
    }
    let valid_hash = manifest.compiled_program.as_ref().is_some_and(|compiled| {
        compiled.bytes > 0
            && compiled.bytes <= u32::try_from(MAX_UNPACKED_BYTES).unwrap_or(u32::MAX)
            && compiled.sha256.len() == 64
            && compiled.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    if !valid_hash {
        return Err(invalid("missing compiled program identity"));
    }
    let expected = if manifest.compile_on_load {
        vec!["manifest.json", "source/workcart.m01w"]
    } else {
        vec![
            "build/cartridge.js",
            "build/cartridge.js.map",
            "manifest.json",
            "source/workcart.m01w",
        ]
    };
    if entries.keys().map(String::as_str).collect::<Vec<_>>() != expected {
        return Err(invalid("archive contains unknown or missing entries"));
    }
    Ok(())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

const fn default_hardware_revision() -> u8 {
    2
}

fn insert_unique(
    entries: &mut BTreeMap<String, Vec<u8>>,
    path: String,
    bytes: Vec<u8>,
) -> Result<(), CartridgeError> {
    if entries.contains_key(&path) {
        return Err(cartridge_error(
            "M014007",
            format!("two project files map to archive path '{path}'"),
        ));
    }
    entries.insert(path, bytes);
    Ok(())
}

fn encode_archive(entries: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, CartridgeError> {
    encode_archive_with_limits(entries, CARTRIDGE_ARCHIVE_LIMITS)
}

pub(crate) fn encode_archive_with_limits(
    entries: &BTreeMap<String, Vec<u8>>,
    limits: ArchiveLimits,
) -> Result<Vec<u8>, CartridgeError> {
    if entries.is_empty() || entries.len() > limits.entries {
        return Err(cartridge_error(
            "M014005",
            "archive entry count is outside format limits".to_owned(),
        ));
    }
    let mut output = Vec::new();
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(
        &u32::try_from(entries.len())
            .map_err(|_| cartridge_error("M014005", "too many archive entries".to_owned()))?
            .to_le_bytes(),
    );
    let mut unpacked = 0_usize;
    for (path, raw) in entries {
        unpacked = unpacked.checked_add(raw.len()).ok_or_else(|| {
            cartridge_error("M014005", "archive unpacked size overflow".to_owned())
        })?;
        if unpacked > limits.unpacked {
            return Err(cartridge_error(
                "M014005",
                "archive exceeds its unpacked capacity".to_owned(),
            ));
        }
        let path = normalize_archive_path(path)?;
        let path_bytes = path.as_bytes();
        let encoded = rle_encode(raw);
        output.extend_from_slice(
            &u16::try_from(path_bytes.len())
                .map_err(|_| cartridge_error("M014005", "archive path is too long".to_owned()))?
                .to_le_bytes(),
        );
        output.extend_from_slice(
            &u32::try_from(raw.len())
                .map_err(|_| cartridge_error("M014005", "archive entry is too large".to_owned()))?
                .to_le_bytes(),
        );
        output.extend_from_slice(
            &u32::try_from(encoded.len())
                .map_err(|_| {
                    cartridge_error("M014005", "compressed entry is too large".to_owned())
                })?
                .to_le_bytes(),
        );
        output.extend_from_slice(&sha256(raw));
        output.extend_from_slice(path_bytes);
        output.extend_from_slice(&encoded);
        if output.len() > limits.packed {
            return Err(cartridge_error(
                "M014005",
                "packed archive exceeds its capacity".to_owned(),
            ));
        }
    }
    Ok(output)
}

fn rle_encode(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let run = repeated_length(input, cursor);
        if run >= 4 {
            output.push(0x80 | u8::try_from(run - 1).expect("run is capped to 128"));
            output.push(input[cursor]);
            cursor += run;
            continue;
        }
        let literal_start = cursor;
        cursor += run;
        while cursor < input.len() && cursor - literal_start < 128 {
            let next_run = repeated_length(input, cursor);
            if next_run >= 4 || cursor - literal_start + next_run > 128 {
                break;
            }
            cursor += next_run;
        }
        let length = cursor - literal_start;
        output.push(u8::try_from(length - 1).expect("literal is capped to 128"));
        output.extend_from_slice(&input[literal_start..cursor]);
    }
    output
}

fn rle_decode(input: &[u8], expected_length: usize) -> Result<Vec<u8>, CartridgeError> {
    let mut output = Vec::with_capacity(expected_length.min(MAX_UNPACKED_BYTES));
    let mut cursor = 0;
    while cursor < input.len() {
        let tag = input[cursor];
        cursor += 1;
        let length = usize::from(tag & 0x7f) + 1;
        if output.len().saturating_add(length) > expected_length {
            return Err(cartridge_error(
                "M014010",
                "compressed entry expands past its declared size".to_owned(),
            ));
        }
        if tag & 0x80 == 0 {
            let end = cursor.checked_add(length).ok_or_else(|| {
                cartridge_error("M014010", "compressed literal overflow".to_owned())
            })?;
            let literal = input.get(cursor..end).ok_or_else(|| {
                cartridge_error("M014010", "truncated compressed literal".to_owned())
            })?;
            output.extend_from_slice(literal);
            cursor = end;
        } else {
            let byte = *input
                .get(cursor)
                .ok_or_else(|| cartridge_error("M014010", "truncated compressed run".to_owned()))?;
            cursor += 1;
            output.resize(output.len() + length, byte);
        }
    }
    if output.len() != expected_length {
        return Err(cartridge_error(
            "M014010",
            "compressed entry size does not match its declaration".to_owned(),
        ));
    }
    Ok(output)
}

fn repeated_length(input: &[u8], start: usize) -> usize {
    let byte = input[start];
    let mut end = start + 1;
    while end < input.len() && end - start < 128 && input[end] == byte {
        end += 1;
    }
    end - start
}

fn normalize_project_path(path: &str) -> Result<String, CartridgeError> {
    normalize_path(path, "M014006")
}

fn normalize_archive_path(path: &str) -> Result<String, CartridgeError> {
    normalize_path(path, "M014010")
}

fn normalize_path(path: &str, code: &'static str) -> Result<String, CartridgeError> {
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
        return Err(cartridge_error(
            code,
            format!("path '{path}' is not a canonical relative ASCII path"),
        ));
    }
    Ok(path.to_owned())
}

fn normalized_source(bytes: &[u8], path: &str) -> Result<String, CartridgeError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| cartridge_error("M014002", format!("source '{path}' is not valid UTF-8")))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

fn valid_cartridge_id(value: &str) -> bool {
    (3..=64).contains(&value.len())
        && value.is_ascii()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'-'))
        })
}

fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn has_modl_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "modl")
}

fn cartridge_error(code: &'static str, message: String) -> CartridgeError {
    CartridgeError { code, message }
}

pub(crate) fn sha256_hex(input: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in sha256(input) {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[allow(clippy::many_single_char_names)]
fn sha256(input: &[u8]) -> [u8; 32] {
    let bit_length = u64::try_from(input.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());
    let mut hash = SHA256_INITIAL;
    for block in padded.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (index, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*chunk);
        }
        for index in 16..64 {
            let first = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let second = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(first)
                .wrapping_add(words[index - 7])
                .wrapping_add(second);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;
        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let temporary1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choice)
                .wrapping_add(SHA256_ROUND[index])
                .wrapping_add(words[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary2 = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary1);
            d = c;
            c = b;
            b = a;
            a = temporary1.wrapping_add(temporary2);
        }
        for (slot, value) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut output = [0_u8; 32];
    for (index, word) in hash.into_iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CartridgeError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or_else(|| cartridge_error("M014010", "cartridge offset overflow".to_owned()))?;
        let result = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| cartridge_error("M014010", "cartridge is truncated".to_owned()))?;
        self.cursor = end;
        Ok(result)
    }

    fn u16(&mut self) -> Result<u16, CartridgeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(
            |_| cartridge_error("M014010", "invalid 16-bit field".to_owned()),
        )?))
    }

    fn u32(&mut self) -> Result<u32, CartridgeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| cartridge_error("M014010", "invalid 32-bit field".to_owned()),
        )?))
    }

    const fn finished(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_archive_codec_enforces_container_bounds_without_relaxing_cartridge_limits() {
        use super::{ArchiveLimits, decode_archive, encode_archive_with_limits};
        let entries = std::collections::BTreeMap::from([
            ("cartridges/main.m01c".to_owned(), vec![7; 1024]),
            (
                "source/workcart.m01w".to_owned(),
                b"authored source".to_vec(),
            ),
        ]);
        let limits = ArchiveLimits {
            packed: 4096,
            unpacked: 2048,
            entries: 2,
        };
        let bytes = encode_archive_with_limits(&entries, limits).unwrap();
        assert_eq!(decode_archive(&bytes, limits).unwrap(), entries);
        let exact = ArchiveLimits {
            packed: bytes.len(),
            unpacked: entries.values().map(Vec::len).sum(),
            entries: 2,
        };
        assert_eq!(encode_archive_with_limits(&entries, exact).unwrap(), bytes);
        for restrictive in [
            ArchiveLimits {
                packed: bytes.len() - 1,
                ..exact
            },
            ArchiveLimits {
                unpacked: exact.unpacked - 1,
                ..exact
            },
            ArchiveLimits {
                entries: 1,
                ..exact
            },
        ] {
            assert!(encode_archive_with_limits(&entries, restrictive).is_err());
            assert!(decode_archive(&bytes, restrictive).is_err());
        }
        let large = std::collections::BTreeMap::from([(
            "large".to_owned(),
            vec![0; super::MAX_UNPACKED_BYTES + 1],
        )]);
        assert!(encode_archive_with_limits(&large, super::CARTRIDGE_ARCHIVE_LIMITS).is_err());
        let invalid = std::collections::BTreeMap::from([("../escape".to_owned(), vec![1])]);
        assert!(encode_archive_with_limits(&invalid, limits).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_archive(&trailing, limits).is_err());
        let mut corrupt = bytes;
        corrupt[25] ^= 1;
        assert!(decode_archive(&corrupt, limits).is_err());
    }

    use super::{
        CARTRIDGE_CAPACITY_BYTES, compile_project, decode_cartridge, load_cartridge_program,
        pack_project, rle_decode, rle_encode, sha256_hex, unpack_cartridge_project,
    };
    use crate::CompileMode;
    use std::collections::BTreeMap;

    fn manifest() -> &'static str {
        r#"format = 2
language = "MODL/1"
id = "test.game"
title = "Test Game"
author = "@gongahkia"
version = "1.0.0"
entry = "src/main.modl"
update_rate = 60
display = "assets/display.m01p"

[assets.hero]
kind = "sprite"
path = "assets/hero.m01g"
"#
    }

    fn files(line_ending: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (
                "src/main.modl".to_owned(),
                format!("on draw:{line_ending}  clear(0){line_ending}").into_bytes(),
            ),
            ("assets/hero.m01g".to_owned(), br#"{"revision":2,"kind":"sprite","width":1,"height":1,"frames":[[1]]}"#.to_vec()),
            (
                "assets/display.m01p".to_owned(),
                serde_json::to_vec(&serde_json::json!({"revision":2,"kind":"display","remap":(0..32).collect::<Vec<_>>(),"raster":[]})).unwrap(),
            ),
        ])
    }

    #[test]
    fn hashes_the_standard_sha256_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rle_round_trips_runs_and_literal_boundaries() {
        let input: Vec<_> = (0_u16..600)
            .map(|value| u8::try_from((value / 5) % 251).expect("bounded"))
            .collect();
        let encoded = rle_encode(&input);
        assert_eq!(rle_decode(&encoded, input.len()).expect("decode"), input);
        assert!(rle_decode(&encoded, input.len() - 1).is_err());
    }

    #[test]
    fn packing_is_byte_identical_and_normalizes_source_line_endings() {
        let first = pack_project(manifest(), &files("\n")).expect("pack LF");
        let second = pack_project(manifest(), &files("\r\n")).expect("pack CRLF");
        assert_eq!(first.bytes, second.bytes);
        assert!(first.bytes.len() < CARTRIDGE_CAPACITY_BYTES);
        let decoded = decode_cartridge(&first.bytes).expect("decode");
        assert_eq!(decoded.manifest.title, "Test Game");
        assert_eq!(
            unpack_cartridge_project(&first.bytes).unwrap().files["src/main.modl"],
            b"on draw:\n  clear(0)\n"
        );
        assert!(decoded.entries.contains_key("build/cartridge.js"));
        assert_eq!(decoded.entries.len(), 4);
        assert!(decoded.entries.contains_key("source/workcart.m01w"));
        assert_eq!(decoded.manifest.assets["hero"].path, "assets/hero.m01g");
        assert_eq!(
            decoded.manifest.display.as_deref(),
            Some("assets/display.m01p")
        );
        assert!(
            !decoded
                .entries
                .contains_key("presentation/display/assets/display.m01p")
        );
    }

    #[test]
    fn source_only_cartridges_fit_4k_and_verify_the_recompiled_program() {
        let manifest = manifest().replace(
            "update_rate = 60",
            "update_rate = 60\ncompile_on_load = true",
        );
        let first = pack_project(&manifest, &files("\n")).expect("source cart packs");
        let second = pack_project(&manifest, &files("\n")).expect("source cart repacks");
        assert_eq!(first.bytes, second.bytes);
        assert!(first.bytes.len() <= 4_096, "{} bytes", first.bytes.len());
        let (decoded, program) = load_cartridge_program(&first.bytes).expect("source cart loads");
        assert!(decoded.manifest.compile_on_load);
        assert!(!decoded.entries.contains_key("build/cartridge.js"));
        assert!(
            std::str::from_utf8(&program)
                .expect("program is UTF-8")
                .contains("createCartridge")
        );
        assert!(
            unpack_cartridge_project(&first.bytes)
                .expect("source cart unpacks")
                .manifest
                .contains("compile_on_load = true")
        );
    }

    #[test]
    fn hardware_two_preserves_layered_sprite_source_in_cartridge() {
        let source = manifest().replace(
            "update_rate = 60",
            "update_rate = 60\nhardware_revision = 2",
        );
        let mut project_files = files("\n");
        let sidecar = br#"{"revision":1,"width":1,"height":1,"frames":[{"layers":[{"name":"Base","visible":true,"locked":false,"pixels":"/wAA/w=="}]}]}"#.to_vec();
        project_files.insert("editor/sprites/hero.m01paint".to_owned(), sidecar.clone());
        let packed = pack_project(&source, &project_files).expect("hardware two packs");
        assert_eq!(packed.manifest.hardware_revision, 2);
        let restored = unpack_cartridge_project(&packed.bytes).expect("hardware two unpacks");
        assert!(restored.manifest.contains("hardware_revision = 2"));
        assert_eq!(
            restored.files.get("editor/sprites/hero.m01paint"),
            Some(&sidecar)
        );
    }

    #[test]
    fn detailed_layered_source_respects_the_current_capacity() {
        let source = manifest().replace(
            "update_rate = 60",
            "update_rate = 60\nhardware_revision = 2",
        );
        let mut project_files = files("\n");
        let mut state = 42_u32;
        let pixels: Vec<u8> = (0..128 * 128 * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                state.to_be_bytes()[0]
            })
            .collect();
        let layer =
            serde_json::json!({"name":"Base","visible":true,"locked":false,"pixels":pixels});
        let sidecar = serde_json::to_vec(&serde_json::json!({
            "revision":1,"width":128,"height":128,
            "frames":[{"layers":[layer.clone()]},{"layers":[layer]}],
        }))
        .expect("encode layered source");
        project_files.insert("editor/sprites/hero.m01paint".to_owned(), sidecar);
        let packed =
            pack_project(&source, &project_files).expect("hardware two packs detailed source");
        assert!(packed.bytes.len() > 256 * 1024);
        assert!(packed.bytes.len() < CARTRIDGE_CAPACITY_BYTES);
        assert_eq!(
            decode_cartridge(&packed.bytes)
                .unwrap()
                .manifest
                .hardware_revision,
            2
        );
    }

    #[test]
    fn release_cartridges_exclude_test_only_source_entries() {
        let mut project_files = files("\n");
        project_files.insert(
            "tests/private-check.modl".to_owned(),
            b"on start:\n  assert true\n".to_vec(),
        );
        let packed = pack_project(manifest(), &project_files).expect("project packs");
        let decoded = decode_cartridge(&packed.bytes).expect("cartridge decodes");
        assert!(
            !decoded
                .entries
                .contains_key("source/tests/private-check.modl")
        );
    }

    #[test]
    fn project_imports_link_through_the_typed_pipeline() {
        let mut project_files = files("\n");
        project_files.insert(
            "src/main.modl".to_owned(),
            b"import src.math as math\nstate score: Int = 1\non update:\n  score = math.twice(score)\n"
                .to_vec(),
        );
        project_files.insert(
            "src/math.modl".to_owned(),
            b"fn twice(value: Int) -> Int:\n  return value * 2\n".to_vec(),
        );
        let output = compile_project(manifest(), &project_files, CompileMode::Release)
            .expect("project links");
        assert!(
            output.analysis.diagnostics.is_empty(),
            "{:?}",
            output.analysis.diagnostics
        );
        assert!(
            !output
                .generated
                .expect("project generates")
                .javascript
                .is_empty()
        );
        assert!(pack_project(manifest(), &project_files).is_ok());
    }

    #[test]
    fn debug_projects_map_generated_statements_to_original_modules() {
        let mut project_files = files("\n");
        let main = "import src.math as math\nstate score: Int = 1\non update:\n  score = math.twice(score)\n";
        let math = "fn twice(value: Int) -> Int:\n  return value * 2\n";
        project_files.insert("src/main.modl".to_owned(), main.as_bytes().to_vec());
        project_files.insert("src/math.modl".to_owned(), math.as_bytes().to_vec());
        let output = compile_project(manifest(), &project_files, CompileMode::Debug)
            .expect("debug project links");
        assert!(output.analysis.diagnostics.is_empty());
        let generated = output.generated.expect("debug project generates");
        assert!(
            generated.javascript.contains(&format!(
                "source:\"src/main.modl\",start:{},end:",
                main.find("state score").expect("state exists")
            )),
            "{}",
            generated.javascript
        );
        assert!(generated.javascript.contains(&format!(
            "source:\"src/math.modl\",start:{},end:",
            math.find("return value").expect("return exists")
        )));
        assert!(
            generated
                .javascript
                .contains("enter(\"twice\", {source:\"src/math.modl\"")
        );
        assert!(
            generated
                .javascript
                .contains("enter(\"@update\", {source:\"src/main.modl\"")
        );
        let source_map: serde_json::Value =
            serde_json::from_str(&generated.source_map_json).expect("source map is JSON");
        assert_eq!(
            source_map["sources"],
            serde_json::json!(["src/main.modl", "src/math.modl"])
        );
        assert_eq!(
            source_map["sourcesContent"],
            serde_json::json!([main, math])
        );
        assert_ne!(source_map["mappings"], "");
    }

    #[test]
    fn project_modules_namespace_collisions_and_preserve_local_shadowing() {
        let mut project_files = files("\n");
        project_files.insert(
            "src/main.modl".to_owned(),
            b"import src.left as left\nimport src.right as right\nstate score: Int = left.value(2) + right.value(3)\non draw:\n  clear(0)\n"
                .to_vec(),
        );
        project_files.insert(
            "src/left.modl".to_owned(),
            b"private const offset: Int = 1\npub fn value(value: Int) -> Int:\n  return value + offset\n"
                .to_vec(),
        );
        project_files.insert(
            "src/right.modl".to_owned(),
            b"private const offset: Int = 2\nfn value(input: Int) -> Int:\n  let value = input\n  return value + offset\n"
                .to_vec(),
        );
        let output = compile_project(manifest(), &project_files, CompileMode::Release)
            .expect("namespaced project links");
        assert!(
            output.analysis.diagnostics.is_empty(),
            "{:?}",
            output.analysis.diagnostics
        );
        let names: Vec<_> = output
            .analysis
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect();
        assert!(names.contains(&"__m01m0_offset"));
        assert!(names.contains(&"__m01m1_offset"));
        assert!(names.contains(&"__m01m0_value"));
        assert!(names.contains(&"__m01m1_value"));
        assert!(output.generated.is_some());
    }

    #[test]
    fn project_modules_reject_private_members_and_cycles() {
        let mut private_files = files("\n");
        private_files.insert(
            "src/main.modl".to_owned(),
            b"import src.math as math\non draw:\n  clear(math.secret())\n".to_vec(),
        );
        private_files.insert(
            "src/math.modl".to_owned(),
            b"private fn secret() -> Int:\n  return 1\n".to_vec(),
        );
        let private_error = compile_project(manifest(), &private_files, CompileMode::Release)
            .expect_err("private member is hidden");
        assert_eq!(private_error.code, "M014009");
        assert!(
            private_error
                .message
                .contains("no exported member 'secret'")
        );

        let mut cycle_files = files("\n");
        cycle_files.insert(
            "src/main.modl".to_owned(),
            b"import src.first\non draw:\n  clear(0)\n".to_vec(),
        );
        cycle_files.insert(
            "src/first.modl".to_owned(),
            b"import src.second\nfn first() -> Int:\n  return 1\n".to_vec(),
        );
        cycle_files.insert(
            "src/second.modl".to_owned(),
            b"import src.first\nfn second() -> Int:\n  return 2\n".to_vec(),
        );
        let cycle_error = compile_project(manifest(), &cycle_files, CompileMode::Release)
            .expect_err("cycle is rejected");
        assert_eq!(cycle_error.code, "M014008");
        assert!(
            cycle_error
                .message
                .contains("src/first.modl -> src/second.modl")
        );
    }

    #[test]
    fn packed_cartridges_reconstruct_source_visible_projects() {
        let packed = pack_project(manifest(), &files("\n")).expect("pack");
        let unpacked = unpack_cartridge_project(&packed.bytes).expect("unpack");
        let reconstructed = super::parse_project_manifest(&unpacked.manifest).expect("manifest");
        assert_eq!(reconstructed.id, "test.game");
        assert_eq!(reconstructed.entry, "src/main.modl");
        assert_eq!(
            reconstructed.display.as_deref(),
            Some("assets/display.m01p")
        );
        assert_eq!(unpacked.files, files("\n"));
    }

    #[test]
    fn rejects_obsolete_headers_profiles_and_builds_not_produced_by_the_source() {
        let packed = pack_project(manifest(), &files("\n")).unwrap();
        let mut old_header = packed.bytes.clone();
        old_header[7] = 1;
        assert!(decode_cartridge(&old_header).is_err());
        let decoded = decode_cartridge(&packed.bytes).unwrap();
        for old_profile in [false, true] {
            let mut metadata = decoded.manifest.clone();
            if old_profile {
                metadata.hardware_revision = 1;
            } else {
                metadata.format_revision = 1;
            }
            let mut entries = decoded.entries.clone();
            entries.insert(
                "manifest.json".to_owned(),
                serde_json::to_vec(&metadata).unwrap(),
            );
            let bytes = super::encode_archive(&entries).unwrap();
            assert_eq!(decode_cartridge(&bytes).unwrap_err().code, "M014012");
        }
        let mut entries = decoded.entries;
        let mut metadata = decoded.manifest;
        let forged =
            b"export default () => { throw new Error('untrusted archived build'); };".to_vec();
        let identity = super::FileIntegrity {
            bytes: u32::try_from(forged.len()).unwrap(),
            sha256: sha256_hex(&forged),
        };
        entries.insert("build/cartridge.js".to_owned(), forged);
        metadata
            .files
            .insert("build/cartridge.js".to_owned(), identity.clone());
        metadata.compiled_program = Some(identity);
        entries.insert(
            "manifest.json".to_owned(),
            serde_json::to_vec(&metadata).unwrap(),
        );
        let bytes = super::encode_archive(&entries).unwrap();
        assert!(decode_cartridge(&bytes).is_ok());
        assert_eq!(load_cartridge_program(&bytes).unwrap_err().code, "M014015");
    }

    #[test]
    fn decoding_rejects_mutation_truncation_and_trailing_bytes() {
        let packed = pack_project(manifest(), &files("\n")).expect("pack");
        let mut mutated = packed.bytes.clone();
        let last = mutated.len() - 1;
        mutated[last] ^= 1;
        assert!(decode_cartridge(&mutated).is_err());
        assert!(decode_cartridge(&packed.bytes[..packed.bytes.len() - 1]).is_err());
        let mut trailing = packed.bytes;
        trailing.push(0);
        assert!(decode_cartridge(&trailing).is_err());
    }

    #[test]
    fn bounded_arbitrary_inputs_never_panic() {
        let mut state = 0x4d30_3031_u64;
        for length in 0..512 {
            let mut input = vec![0_u8; length];
            for byte in &mut input {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state.to_le_bytes()[0];
            }
            let _ = decode_cartridge(&input);
        }
    }
}
