use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use modl_core::{
    CompileMode, ProjectManifest, compile_project, decode_cartridge, pack_project,
    parse_project_manifest, unpack_cartridge_project,
};

const MAX_PROJECT_FILE: u64 = 16 * 1024 * 1024;
const MAX_PROJECT_TOTAL: usize = 16 * 1024 * 1024;

pub struct NativeProject {
    pub root: Option<PathBuf>,
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
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            decode_cartridge(&bytes).map_err(|error| error.to_string())?;
            let unpacked = unpack_cartridge_project(&bytes).map_err(|error| error.to_string())?;
            return Self::from_parts(None, unpacked.manifest, unpacked.files);
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

    pub fn save(&self) -> Result<(), String> {
        let root = self
            .root
            .as_ref()
            .ok_or("choose a project directory before saving")?;
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        write_atomic(&root.join("cart.toml"), self.manifest_source.as_bytes())?;
        for (path, bytes) in &self.files {
            write_atomic(&root.join(path), bytes)?;
        }
        Ok(())
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
        if name.starts_with('.') || matches!(name.as_ref(), "dist" | "target" | "node_modules") {
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
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn creates_and_resaves_a_native_project() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("mod01-native-project-{}-{nonce}", std::process::id()));
        let mut project = NativeProject::create(&root, "native-test").unwrap();
        project.editor_text = "on draw:\n  clear(2)\n".to_owned();
        project.sync_editor();
        project.save().unwrap();
        let reopened = NativeProject::open(&root).unwrap();
        assert_eq!(reopened.editor_text, project.editor_text);
        fs::remove_dir_all(root).unwrap();
    }
}
