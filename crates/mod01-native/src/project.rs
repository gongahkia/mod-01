use base64::{Engine as _, engine::general_purpose::STANDARD};
use modl_core::{
    CompileMode, ProjectManifest, compile_project, decode_cartridge_png, pack_workcart,
    parse_project_manifest, rewrite_workcart, unpack_workcart, workcart_project_view,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const MAX_PROJECT_TOTAL: u64 = 16 * 1024 * 1024;

/// A disposable module/asset view backed by one authoritative current workcart.
#[derive(Clone)]
pub struct NativeProject {
    pub root: Option<PathBuf>,
    pub workcart_path: Option<PathBuf>,
    workcart_source: String,
    pub manifest_source: String,
    pub manifest: ProjectManifest,
    pub files: BTreeMap<String, Vec<u8>>,
    pub selected_file: String,
    pub editor_text: String,
    pub dirty: bool,
}

impl NativeProject {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = if path.is_dir() {
            path.join("cart.m01w")
        } else {
            path.to_path_buf()
        };
        if fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_PROJECT_TOTAL
        {
            return Err("cartridge source exceeds 16 MiB".to_owned());
        }
        let source = fs::read(&path).map_err(|error| error.to_string())?;
        if path
            .extension()
            .is_some_and(|extension| extension == "m01w")
        {
            return Self::from_source(
                String::from_utf8(source).map_err(|error| error.to_string())?,
                Some(path),
            );
        }
        let bytes = if path.extension().is_some_and(|extension| extension == "png") {
            decode_cartridge_png(&source)?.cartridge
        } else {
            source
        };
        Self::from_source(
            unpack_workcart(&bytes).map_err(|error| error.to_string())?,
            None,
        )
    }

    pub fn create(path: &Path, id: &str) -> Result<Self, String> {
        if path.is_file()
            || (path.is_dir()
                && fs::read_dir(path)
                    .map_err(|error| error.to_string())?
                    .next()
                    .is_some())
        {
            return Err("new workcart path/directory must be empty".to_owned());
        }
        let metadata = format!(
            "format = 2\nlanguage = \"MODL/1\"\nid = {}\ntitle = \"NEW CARTRIDGE\"\nauthor = \"@author\"\nversion = \"0.1.0\"\nentry = \"src/main.modl\"\nupdate_rate = 60\nhardware_revision = 2\n",
            serde_json::to_string(id).map_err(|error| error.to_string())?
        );
        let manifest = parse_project_manifest(&metadata).map_err(|error| error.to_string())?;
        let files = BTreeMap::from([(
            "src/main.modl".to_owned(),
            b"on draw:\n  clear(1)\n".to_vec(),
        )]);
        let source = modl_core::encode_workcart(&manifest, &files, &[], &[])
            .map_err(|error| error.to_string())?;
        let output = if path
            .extension()
            .is_some_and(|extension| extension == "m01w")
        {
            path.to_path_buf()
        } else {
            path.join("cart.m01w")
        };
        let project = Self::from_source(source, Some(output))?;
        project.save()?;
        Ok(project)
    }

    fn from_source(source: String, workcart_path: Option<PathBuf>) -> Result<Self, String> {
        let view = workcart_project_view(&source).map_err(|error| error.to_string())?;
        let manifest = parse_project_manifest(&view.manifest).map_err(|error| error.to_string())?;
        let selected_file = manifest.entry.clone();
        let editor_text = String::from_utf8(
            view.files
                .get(&selected_file)
                .cloned()
                .ok_or("entry source is missing")?,
        )
        .map_err(|error| error.to_string())?;
        let root = workcart_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        Ok(Self {
            root,
            workcart_path,
            workcart_source: source,
            manifest_source: view.manifest,
            manifest,
            files: view.files,
            selected_file,
            editor_text,
            dirty: false,
        })
    }

    pub fn select_source(&mut self, path: &str) -> Result<(), String> {
        self.sync_editor();
        self.editor_text = String::from_utf8(
            self.files
                .get(path)
                .cloned()
                .ok_or("source file is missing")?,
        )
        .map_err(|error| error.to_string())?;
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

    pub fn save(&self) -> Result<(), String> {
        let path = self
            .workcart_path
            .as_ref()
            .ok_or("choose a workcart path before saving")?;
        let rewritten = rewrite_workcart(&self.workcart_source, &self.manifest_source, &self.files)
            .map_err(|error| error.to_string())?;
        let previous = match fs::read_to_string(path) {
            Ok(source) => Some(source),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if previous.as_ref() == Some(&rewritten) {
            return Ok(());
        }
        if let Some(previous) = previous {
            let directory = workcart_recovery_directory(path);
            fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            write_atomic(
                &directory.join(format!("{stamp:020}.m01w")),
                previous.as_bytes(),
            )?;
        }
        write_atomic(path, rewritten.as_bytes())?;
        retain_workcart_revisions(path)
    }

    pub fn revisions(&self) -> Result<Vec<PathBuf>, String> {
        list_workcart_revisions(
            self.workcart_path
                .as_ref()
                .ok_or("project has no workcart path")?,
        )
    }

    pub fn check(&self) -> Result<String, String> {
        let compilation = compile_project(&self.manifest_source, &self.files, CompileMode::Release)
            .map_err(|error| error.to_string())?;
        if compilation.analysis.diagnostics.is_empty() {
            self.pack()?;
            Ok("CHECK OK".to_owned())
        } else {
            Ok(compilation
                .analysis
                .diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }

    pub fn pack(&self) -> Result<Vec<u8>, String> {
        let source = rewrite_workcart(&self.workcart_source, &self.manifest_source, &self.files)
            .map_err(|error| error.to_string())?;
        pack_workcart(&source)
            .map(|packed| packed.bytes)
            .map_err(|error| error.to_string())
    }

    pub fn javascript(&self, debug: bool) -> Result<String, String> {
        let compilation = compile_project(
            &self.manifest_source,
            &self.files,
            if debug {
                CompileMode::Debug
            } else {
                CompileMode::Release
            },
        )
        .map_err(|error| error.to_string())?;
        compilation
            .generated
            .map(|program| program.javascript)
            .ok_or_else(|| {
                compilation
                    .analysis
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
    }

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

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("invalid workcart path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let name = path
        .file_name()
        .ok_or("workcart has no filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.{stamp}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(temporary, path).map_err(|error| error.to_string())
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
