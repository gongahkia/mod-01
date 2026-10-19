use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use modl_core::{
    CompileMode, ProjectManifest, compile_project, decode_cartridge, decode_cartridge_png,
    pack_project, pack_workcart, parse_project_manifest, rewrite_workcart,
    unpack_cartridge_project, unpack_workcart, workcart_project_view,
};
use serde_json::{Value, json};

const MAX_PROJECT_FILE: u64 = 16 * 1024 * 1024;
const MAX_PROJECT_TOTAL: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct NativeProject {
    pub root: Option<PathBuf>,
    pub workcart_path: Option<PathBuf>,
    workcart_source: Option<String>,
    pub manifest_source: String,
    pub manifest: ProjectManifest,
    pub files: BTreeMap<String, Vec<u8>>,
    pub selected_file: String,
    pub editor_text: String,
    pub dirty: bool,
}

impl NativeProject {
    pub fn open(path: &Path) -> Result<Self, String> {
        if path.is_file() {
            if path
                .extension()
                .is_some_and(|extension| extension == "m01w")
            {
                let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
                if source.len() > MAX_PROJECT_TOTAL {
                    return Err("workcart exceeds 16 MiB".to_owned());
                }
                let view = workcart_project_view(&source).map_err(|error| error.to_string())?;
                let mut project = Self::from_parts(None, view.manifest, view.files)?;
                project.workcart_path = Some(path.to_path_buf());
                project.workcart_source = Some(source);
                return Ok(project);
            }
            let source = fs::read(path).map_err(|error| error.to_string())?;
            let bytes = if path.extension().is_some_and(|extension| extension == "png") {
                decode_cartridge_png(&source)?.cartridge
            } else {
                source
            };
            let decoded = decode_cartridge(&bytes).map_err(|error| error.to_string())?;
            let unpacked = unpack_cartridge_project(&bytes).map_err(|error| error.to_string())?;
            let mut project = Self::from_parts(None, unpacked.manifest, unpacked.files)?;
            if decoded.entries.contains_key("source/workcart.m01w") {
                project.workcart_source =
                    Some(unpack_workcart(&bytes).map_err(|error| error.to_string())?);
            }
            return Ok(project);
        }
        let manifest_source = fs::read_to_string(path.join("cart.toml"))
            .map_err(|error| format!("cannot read cart.toml: {error}"))?;
        let mut files = BTreeMap::new();
        read_tree(path, path, &mut files)?;
        Self::from_parts(Some(path.to_path_buf()), manifest_source, files)
    }

    pub fn create(path: &Path, id: &str) -> Result<Self, String> {
        if path.exists()
            && path
                .read_dir()
                .map_err(|error| error.to_string())?
                .next()
                .is_some()
        {
            return Err("new project directory must be empty".to_owned());
        }
        let source = format!(
            "format = 1\nlanguage = \"MODL/1\"\nid = \"{id}\"\ntitle = \"NEW CARTRIDGE\"\nauthor = \"@author\"\nversion = \"0.1.0\"\nentry = \"src/main.modl\"\nupdate_rate = 60\nhardware_revision = 2\n[assets]\n"
        );
        let mut files = BTreeMap::new();
        files.insert(
            "src/main.modl".to_owned(),
            b"on draw:\n  clear(1)\n".to_vec(),
        );
        let project = Self::from_parts(Some(path.to_path_buf()), source, files)?;
        project.save()?;
        Ok(project)
    }

    fn from_parts(
        root: Option<PathBuf>,
        manifest_source: String,
        files: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, String> {
        let manifest =
            parse_project_manifest(&manifest_source).map_err(|error| error.to_string())?;
        let selected_file = manifest.entry.clone();
        let editor_text = String::from_utf8(
            files
                .get(&selected_file)
                .cloned()
                .ok_or("entry source is missing")?,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            root,
            workcart_path: None,
            workcart_source: None,
            manifest_source,
            manifest,
            files,
            selected_file,
            editor_text,
            dirty: false,
        })
    }

    pub fn select_source(&mut self, path: &str) -> Result<(), String> {
        self.sync_editor();
        let bytes = self.files.get(path).ok_or("source file is missing")?;
        self.editor_text = String::from_utf8(bytes.clone()).map_err(|error| error.to_string())?;
        self.selected_file = path.to_owned();
        Ok(())
    }

    pub fn sync_editor(&mut self) {
        self.files.insert(
            self.selected_file.clone(),
            self.editor_text.as_bytes().to_vec(),
        );
    }

    pub fn source_paths(&self) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.ends_with(".modl"))
            .cloned()
            .collect()
    }

    pub fn detach_workcart(&mut self) {
        self.workcart_path = None;
        self.workcart_source = None;
    }

    pub fn save(&self) -> Result<(), String> {
        if let (Some(path), Some(source)) = (&self.workcart_path, &self.workcart_source) {
            let rewritten = rewrite_workcart(source, &self.manifest_source, &self.files)
                .map_err(|error| error.to_string())?;
            let previous = fs::read_to_string(path).map_err(|error| error.to_string())?;
            if previous != rewritten {
                let revisions = workcart_recovery_directory(path);
                fs::create_dir_all(&revisions).map_err(|error| error.to_string())?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_nanos();
                fs::write(revisions.join(format!("{stamp:020}.m01w")), previous)
                    .map_err(|error| error.to_string())?;
                write_atomic(path, rewritten.as_bytes())?;
                retain_workcart_revisions(path)?;
            }
            return Ok(());
        }
        let root = self
            .root
            .as_ref()
            .ok_or("choose a project directory before saving")?;
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        self.retain_previous_revision(root)?;
        write_atomic(&root.join("cart.toml"), self.manifest_source.as_bytes())?;
        for (path, bytes) in &self.files {
            write_atomic(&root.join(path), bytes)?;
        }
        Ok(())
    }

    fn retain_previous_revision(&self, root: &Path) -> Result<(), String> {
        let manifest_path = root.join("cart.toml");
        if !manifest_path.is_file() {
            return Ok(());
        }
        let previous_manifest = fs::read(&manifest_path).map_err(|error| error.to_string())?;
        let mut previous_files = BTreeMap::new();
        read_tree(root, root, &mut previous_files)?;
        if previous_manifest == self.manifest_source.as_bytes() && previous_files == self.files {
            return Ok(());
        }
        let directory = root.join(".mod01/recovery");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let revision = directory.join(format!("{stamp:020}-{}", std::process::id()));
        fs::create_dir_all(&revision).map_err(|error| error.to_string())?;
        write_atomic(&revision.join("cart.toml"), &previous_manifest)?;
        for (path, bytes) in previous_files {
            write_atomic(&revision.join(path), &bytes)?;
        }
        let revisions = Self::revisions_at(root)?;
        for path in revisions.iter().skip(10) {
            fs::remove_dir_all(path).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn revisions(&self) -> Result<Vec<PathBuf>, String> {
        if let Some(path) = &self.workcart_path {
            return list_workcart_revisions(path);
        }
        let root = self.root.as_ref().ok_or("project has no directory")?;
        Self::revisions_at(root)
    }

    fn revisions_at(root: &Path) -> Result<Vec<PathBuf>, String> {
        let directory = root.join(".mod01/recovery");
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let mut revisions = fs::read_dir(directory)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.join("cart.toml").is_file())
            .collect::<Vec<_>>();
        revisions.sort_by(|left, right| right.cmp(left));
        Ok(revisions)
    }

    pub fn check(&self) -> Result<String, String> {
        let result = compile_project(&self.manifest_source, &self.files, CompileMode::Release)
            .map_err(|error| error.to_string())?;
        if result.analysis.diagnostics.is_empty() {
            Ok("CHECK OK".to_owned())
        } else {
            Ok(result
                .analysis
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }

    pub fn pack(&self) -> Result<Vec<u8>, String> {
        if let Some(source) = &self.workcart_source {
            let rewritten = rewrite_workcart(source, &self.manifest_source, &self.files)
                .map_err(|error| error.to_string())?;
            return pack_workcart(&rewritten)
                .map(|packed| packed.bytes)
                .map_err(|error| error.to_string());
        }
        pack_project(&self.manifest_source, &self.files)
            .map(|packed| packed.bytes)
            .map_err(|error| error.to_string())
    }

    pub fn javascript(&self, debug: bool) -> Result<String, String> {
        let result = compile_project(
            &self.manifest_source,
            &self.files,
            if debug {
                CompileMode::Debug
            } else {
                CompileMode::Release
            },
        )
        .map_err(|error| error.to_string())?;
        result
            .generated
            .map(|program| program.javascript)
            .ok_or_else(|| {
                result
                    .analysis
                    .diagnostics
                    .iter()
                    .map(|item| format!("{}: {}", item.code, item.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
    }

    /// One compiler/load contract for Studio, packaged games and native smoke verification.
    pub fn load_command(&self, debug: bool, save: &[u8]) -> Result<Value, String> {
        let rom = self.pack()?;
        let javascript = self.javascript(debug)?;
        let symbol_names = if debug {
            compile_project(&self.manifest_source, &self.files, CompileMode::Debug)
                .map_err(|error| error.to_string())?
                .analysis
                .symbols
                .into_iter()
                .map(|symbol| (format!("s{}", symbol.id.0), symbol.name))
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };
        let files: serde_json::Map<String, Value> = self
            .manifest
            .assets
            .values()
            .map(|asset| asset.path.as_str())
            .chain(self.manifest.display.iter().map(String::as_str))
            .filter_map(|path| {
                self.files
                    .get(path)
                    .map(|bytes| (path.to_owned(), Value::String(STANDARD.encode(bytes))))
            })
            .collect();
        Ok(json!({
            "type": "load", "javascript": javascript,
            "manifest": { "id": self.manifest.id, "hardwareRevision": self.manifest.hardware_revision,
                "updateRate": self.manifest.update_rate, "display": self.manifest.display, "assets": self.manifest.assets },
            "files": files, "rom": STANDARD.encode(rom), "save": STANDARD.encode(save),
            "debug": debug, "symbolNames": symbol_names,
        }))
    }
}

fn workcart_recovery_directory(path: &Path) -> PathBuf {
    path.with_extension("m01w.recovery")
}

fn list_workcart_revisions(path: &Path) -> Result<Vec<PathBuf>, String> {
    let directory = workcart_recovery_directory(path);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut revisions = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "m01w")
        })
        .collect::<Vec<_>>();
    revisions.sort_by(|left, right| right.cmp(left));
    Ok(revisions)
}

fn retain_workcart_revisions(path: &Path) -> Result<(), String> {
    for old in list_workcart_revisions(path)?.iter().skip(10) {
        fs::remove_file(old).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn read_tree(
    root: &Path,
    directory: &Path,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_symlink()
        {
            continue;
        }
        if name.starts_with('.')
            || name.ends_with(".mod01-tmp")
            || matches!(name.as_ref(), "dist" | "target" | "node_modules")
        {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        if metadata.is_dir() {
            read_tree(root, &path, output)?;
        } else if metadata.is_file() {
            if name == "cart.toml" {
                continue;
            }
            if metadata.len() > MAX_PROJECT_FILE {
                return Err("project file exceeds 16 MiB".to_owned());
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            output.insert(relative, bytes);
            if output.values().map(Vec::len).sum::<usize>() > MAX_PROJECT_TOTAL {
                return Err("project exceeds 16 MiB".to_owned());
            }
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("invalid project path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("mod01-tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::NativeProject;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn creates_and_resaves_a_native_project() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mod01-native-project-{}-{nonce}",
            std::process::id()
        ));
        let mut project = NativeProject::create(&root, "native-test").unwrap();
        project.editor_text = "on draw:\n  clear(2)\n".to_owned();
        project.sync_editor();
        project.save().unwrap();
        let reopened = NativeProject::open(&root).unwrap();
        assert_eq!(reopened.editor_text, project.editor_text);
        let revisions = reopened.revisions().unwrap();
        assert_eq!(revisions.len(), 1);
        let previous = NativeProject::open(&revisions[0]).unwrap();
        assert_eq!(previous.editor_text, "on draw:\n  clear(1)\n");
        reopened.save().unwrap();
        assert_eq!(reopened.revisions().unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workcart_round_trips_source_and_keeps_a_recovery_revision() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mod01-native-workcart-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("test.m01w");
        fs::write(
            &path,
            r#"format = 2
[cartridge]
language = "MODL/1"
id = "workcart-test"
title = "WORKCART TEST"
author = "@author"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60
[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(1)\n"
"#,
        )
        .unwrap();
        let mut project = NativeProject::open(&path).unwrap();
        let original = fs::read_to_string(&path).unwrap();
        assert_eq!(
            project.pack().unwrap(),
            modl_core::pack_workcart(&original).unwrap().bytes
        );
        let mut authored = modl_core::parse_workcart(&original).unwrap();
        authored.files.insert(
            "notes.txt".to_owned(),
            b"Keep this auxiliary authoring file".to_vec(),
        );
        authored.tests.push(modl_core::WorkcartTest {
            name: "smoke".to_owned(),
            path: "tests/smoke.modl".to_owned(),
            kind: modl_core::WorkcartTestKind::Modl,
            payload: "on start:\n  assert true\n".to_owned(),
        });
        let with_metadata = modl_core::encode_workcart(
            &authored.manifest,
            &authored.files,
            &authored.recipes,
            &authored.tests,
        )
        .unwrap();
        fs::write(&path, with_metadata).unwrap();
        project = NativeProject::open(&path).unwrap();
        let packed = project.pack().unwrap();
        let binary = root.join("test.m01c");
        fs::write(&binary, &packed).unwrap();
        let from_packed = NativeProject::open(&binary).unwrap();
        assert_eq!(from_packed.pack().unwrap(), packed);
        let retained = modl_core::parse_workcart(
            &modl_core::unpack_workcart(&from_packed.pack().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(retained.tests.len(), 1);
        assert!(retained.files.contains_key("notes.txt"));
        project.editor_text = "on draw:\n  clear(2)\n".to_owned();
        project.sync_editor();
        project.save().unwrap();
        assert_eq!(
            NativeProject::open(&path).unwrap().editor_text,
            project.editor_text
        );
        let revisions = project.revisions().unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(
            NativeProject::open(&revisions[0]).unwrap().editor_text,
            "on draw:\n  clear(1)\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
