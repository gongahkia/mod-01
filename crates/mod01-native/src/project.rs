use base64::{Engine as _, engine::general_purpose::STANDARD};
use modl_core::{
    CompileMode, ProjectManifest, compile_project, decode_cartridge_png,
    game::{PackedGame, decode_game, pack_game_workcart},
    parse_project_manifest, rewrite_workcart,
    runtime_game::prepare_runtime_game,
    workcart::MAX_WORKCART_DOCUMENT_BYTES,
    workcart_project_view,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// A disposable module/asset view backed by one authoritative current workcart.
#[derive(Clone)]
pub struct NativeProject {
    pub root: Option<PathBuf>,
    pub workcart_path: Option<PathBuf>,
    workcart_source: String,
    member_source: String,
    member_alias: String,
    member_ids: BTreeMap<String, String>,
    member_paths: BTreeMap<String, String>,
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
        if !path.is_file() {
            return Err("cartridge is missing or is not an ordinary file".to_owned());
        }
        let file = fs::File::open(&path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let is_source = path
            .extension()
            .is_some_and(|extension| extension == "m01w");
        let limit = if is_source {
            MAX_WORKCART_DOCUMENT_BYTES
        } else {
            modl_core::cartridge_png::MAX_CARTRIDGE_PNG_BYTES
        } as u64;
        if !metadata.is_file() || metadata.len() > limit {
            return Err("cartridge exceeds its source/game container limit".to_owned());
        }
        let mut source = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut source)
            .map_err(|error| error.to_string())?;
        if source.len() as u64 > limit {
            return Err("cartridge exceeds its source/game container limit".to_owned());
        }
        if is_source {
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
        let game = decode_game(&bytes).map_err(|error| error.to_string())?;
        Self::from_source(game.source().to_owned(), None)
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

    pub(crate) fn from_source(
        source: String,
        workcart_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let view = workcart_project_view(&source).map_err(|error| error.to_string())?;
        let manifest = parse_project_manifest(&view.manifest).map_err(|error| error.to_string())?;
        let whole = modl_core::parse_workcart(&source).map_err(|error| error.to_string())?;
        let mut member_ids = BTreeMap::from([("main".to_owned(), manifest.id.clone())]);
        for member in whole.bundle {
            let cart =
                modl_core::parse_workcart(&member.source).map_err(|error| error.to_string())?;
            member_ids.insert(member.alias, cart.manifest.id);
        }
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
            member_source: source.clone(),
            workcart_source: source,
            member_alias: "main".to_owned(),
            member_ids,
            member_paths: BTreeMap::new(),
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

    pub fn member_alias(&self) -> &str {
        &self.member_alias
    }

    pub fn members(&self) -> Vec<(String, String)> {
        self.member_ids
            .iter()
            .map(|(alias, id)| {
                (
                    alias.clone(),
                    if alias == &self.member_alias {
                        self.manifest.id.clone()
                    } else {
                        id.clone()
                    },
                )
            })
            .collect()
    }

    /// Flushes just the selected member into the complete enclosing source. Other
    /// members retain their exact source, and a no-op preserves the whole document.
    pub fn source(&self) -> Result<String, String> {
        let mut files = self.files.clone();
        files.insert(
            self.selected_file.clone(),
            self.editor_text.as_bytes().to_vec(),
        );
        let edited = rewrite_workcart(&self.member_source, &self.manifest_source, &files)
            .map_err(|error| error.to_string())?;
        if self.member_alias == "main" {
            return Ok(edited);
        }
        let mut whole =
            modl_core::parse_workcart(&self.workcart_source).map_err(|error| error.to_string())?;
        let member = whole
            .bundle
            .iter_mut()
            .find(|member| member.alias == self.member_alias)
            .ok_or("selected cartridge is missing")?;
        if member.source == edited {
            return Ok(self.workcart_source.clone());
        }
        member.source = edited;
        modl_core::workcart::encode_workcart_with_bundle(
            &whole.manifest,
            &whole.files,
            &whole.recipes,
            &whole.tests,
            &whole.bundle,
        )
        .map_err(|error| error.to_string())
    }

    /// Switching is transactional and needs valid source structure, not valid MODL
    /// code. In-progress compiler errors remain editable and are saved with the game.
    pub fn select_member(&mut self, alias: &str) -> Result<(), String> {
        if alias == self.member_alias {
            return Ok(());
        }
        let source = self.source()?;
        let whole = modl_core::parse_workcart(&source).map_err(|error| error.to_string())?;
        let member_source = if alias == "main" {
            source.clone()
        } else {
            whole
                .bundle
                .iter()
                .find(|member| member.alias == alias)
                .ok_or("cartridge alias is not in this game")?
                .source
                .clone()
        };
        let view = workcart_project_view(&member_source).map_err(|error| error.to_string())?;
        let manifest = parse_project_manifest(&view.manifest).map_err(|error| error.to_string())?;
        let selected_file = self
            .member_paths
            .get(alias)
            .filter(|path| view.files.contains_key(*path))
            .cloned()
            .unwrap_or_else(|| manifest.entry.clone());
        let editor_text = String::from_utf8(
            view.files
                .get(&selected_file)
                .cloned()
                .ok_or("selected source is missing")?,
        )
        .map_err(|error| error.to_string())?;
        self.member_paths
            .insert(self.member_alias.clone(), self.selected_file.clone());
        self.member_ids
            .insert(self.member_alias.clone(), self.manifest.id.clone());
        self.dirty |= source != self.workcart_source;
        self.workcart_source = source;
        self.member_source = member_source;
        self.member_alias = alias.to_owned();
        self.manifest_source = view.manifest;
        self.manifest = manifest;
        self.files = view.files;
        self.selected_file = selected_file;
        self.editor_text = editor_text;
        Ok(())
    }
    pub fn sync_editor(&mut self) {
        self.files.insert(
            self.selected_file.clone(),
            self.editor_text.as_bytes().to_vec(),
        );
    }

    pub fn add_member(&mut self, alias: &str, source: &str) -> Result<(), String> {
        let source = modl_core::workcart::add_workcart_member(&self.source()?, alias, source)
            .map_err(|error| error.to_string())?;
        self.replace_composition(source, alias)
    }

    pub fn remove_member(&mut self, alias: &str) -> Result<(), String> {
        let source = modl_core::workcart::remove_workcart_member(&self.source()?, alias)
            .map_err(|error| error.to_string())?;
        let selected = if self.member_alias == alias {
            "main".to_owned()
        } else {
            self.member_alias.clone()
        };
        self.replace_composition(source, &selected)
    }

    fn replace_composition(&mut self, source: String, selected: &str) -> Result<(), String> {
        let mut candidate = Self::from_source(source, self.workcart_path.clone())?;
        candidate.root.clone_from(&self.root);
        candidate.member_paths = self.member_paths.clone();
        candidate
            .member_paths
            .insert(self.member_alias.clone(), self.selected_file.clone());
        candidate
            .member_paths
            .retain(|alias, _| candidate.member_ids.contains_key(alias));
        if let Some(path) = candidate
            .member_paths
            .get("main")
            .cloned()
            .filter(|path| candidate.files.contains_key(path))
        {
            candidate.select_source(&path)?;
        }
        candidate.select_member(selected)?;
        candidate.dirty = true;
        *self = candidate;
        Ok(())
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
        let rewritten = self.source()?;
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
        self.game().map(PackedGame::into_bytes)
    }

    pub fn game(&self) -> Result<PackedGame, String> {
        let source = self.source()?;
        pack_game_workcart(&source).map_err(|error| error.to_string())
    }

    pub fn cartridge_png(&self) -> Result<(String, Vec<u8>), String> {
        let game = self.game()?;
        let png = modl_core::cartridge_png::encode_game_png(&game)?;
        Ok((game.entry().manifest.id.clone(), png))
    }

    /// Fresh/disposable entry save convenience; all other members start with empty saves.
    pub fn load_command(&self, debug: bool, save: &[u8]) -> Result<Value, String> {
        let game = self.game()?;
        let saves = BTreeMap::from([(game.entry().manifest.id.clone(), save.to_vec())]);
        Self::game_command(&game, debug, &saves)
    }

    pub fn game_command(
        game: &PackedGame,
        debug: bool,
        saves: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Value, String> {
        let prepared = prepare_runtime_game(
            game,
            if debug {
                CompileMode::Debug
            } else {
                CompileMode::Release
            },
        )
        .map_err(|error| error.to_string())?;
        if saves.iter().any(|(id, bytes)| {
            bytes.len() > 8192
                || !game
                    .catalog()
                    .cartridges()
                    .values()
                    .any(|cart| &cart.manifest.id == id)
        }) {
            return Err(
                "initial saves must belong to admitted game members and fit 8 KiB".to_owned(),
            );
        }
        let saves = saves
            .iter()
            .map(|(id, bytes)| (id.clone(), STANDARD.encode(bytes)))
            .collect::<BTreeMap<_, _>>();
        Ok(json!({
            "type": "load", "game": prepared, "saves": saves,
            "seed": modl_core::DEFAULT_SEED, "parameter": "",
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
