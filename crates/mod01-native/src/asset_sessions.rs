//! Bounded, project-local editor sessions. Files remain authoritative; stale history is never restored.
use crate::{asset_tools::AssetWorkbench, project::NativeProject, sprite::SpriteEditor};
use modl_core::AssetKind;
use serde_json::Value;
use std::{collections::VecDeque, mem::size_of};

const MAX_SESSIONS: usize = 64;
const MAX_RETAINED_BYTES: usize = 64 * 1024 * 1024;

#[derive(PartialEq)]
struct Key {
    member: String,
    name: String,
    path: String,
    kind: Option<AssetKind>,
}

enum Editor {
    Structured(Box<AssetWorkbench>),
    Sprite(Box<SpriteEditor>),
}

struct Session {
    key: Key,
    source: Vec<u8>,
    layered: Option<Vec<u8>>,
    editor: Editor,
    bytes: usize,
}

#[derive(Default)]
pub struct AssetSessions {
    sessions: VecDeque<Session>,
    bytes: usize,
    generation: u64,
}

impl AssetSessions {
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.bytes = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn text_id(&self, project: &NativeProject, name: &str, path: &str) -> eframe::egui::Id {
        eframe::egui::Id::unique((
            "asset-json",
            self.generation(),
            project.member_alias(),
            &project.manifest.id,
            name,
            path,
        ))
    }

    pub fn discard_asset(
        &mut self,
        context: &eframe::egui::Context,
        project: &NativeProject,
        name: &str,
        path: &str,
    ) {
        for index in (0..self.sessions.len()).rev() {
            if self.sessions[index].key.member == project.member_alias()
                && self.sessions[index].key.name == name
            {
                self.remove(index);
            }
        }
        let id = self.text_id(project, name, path);
        if let Some(mut state) = eframe::egui::TextEdit::load_state(context, id) {
            state.clear_undoer();
            state.store(context, id);
        }
    }

    pub fn discard_member(&mut self, alias: &str) {
        for index in (0..self.sessions.len()).rev() {
            if self.sessions[index].key.member == alias {
                self.remove(index);
            }
        }
    }

    pub fn stash_structured(
        &mut self,
        project: &NativeProject,
        name: &str,
        path: &str,
        editor: AssetWorkbench,
    ) -> bool {
        let kind = if editor.is_display() {
            None
        } else {
            Some(editor.kind)
        };
        self.stash(
            project,
            Key {
                member: project.member_alias().to_owned(),
                name: name.to_owned(),
                path: path.to_owned(),
                kind,
            },
            Editor::Structured(Box::new(editor)),
        )
    }

    pub fn stash_sprite(&mut self, project: &NativeProject, editor: SpriteEditor) -> bool {
        let kind = project
            .manifest
            .assets
            .get(&editor.name)
            .map(|asset| asset.kind);
        self.stash(
            project,
            Key {
                member: project.member_alias().to_owned(),
                name: editor.name.clone(),
                path: editor.asset_path.clone(),
                kind,
            },
            Editor::Sprite(Box::new(editor)),
        )
    }

    fn stash(&mut self, project: &NativeProject, key: Key, editor: Editor) -> bool {
        if let Some(index) = self.sessions.iter().position(|session| session.key == key) {
            self.remove(index);
        }
        let Some(source) = project.files.get(&key.path).cloned() else {
            return false;
        };
        let (retained, layered) = match &editor {
            Editor::Structured(editor) => (editor.retained_bytes(), None),
            Editor::Sprite(editor) => (
                editor.retained_bytes(),
                project
                    .files
                    .get(&format!("editor/sprites/{}.m01paint", key.name))
                    .cloned(),
            ),
        };
        let bytes = retained
            .saturating_add(source.capacity())
            .saturating_add(layered.as_ref().map_or(0, Vec::capacity))
            .saturating_add(key.name.capacity())
            .saturating_add(key.member.capacity())
            .saturating_add(key.path.capacity())
            .saturating_add(size_of::<Session>());
        if bytes > MAX_RETAINED_BYTES {
            return false;
        }
        while self.sessions.len() >= MAX_SESSIONS
            || self.bytes.saturating_add(bytes) > MAX_RETAINED_BYTES
        {
            self.remove(0);
        }
        self.bytes += bytes;
        self.sessions.push_back(Session {
            key,
            source,
            layered,
            editor,
            bytes,
        });
        true
    }

    fn remove(&mut self, index: usize) -> Session {
        let session = self.sessions.remove(index).unwrap();
        self.bytes -= session.bytes;
        session
    }

    fn take(&mut self, project: &NativeProject, key: &Key) -> Option<Editor> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.key == *key)?;
        let session = self.remove(index);
        if project.files.get(&key.path) != Some(&session.source) {
            return None;
        }
        if matches!(&session.editor, Editor::Sprite(_))
            && project
                .files
                .get(&format!("editor/sprites/{}.m01paint", key.name))
                != session.layered.as_ref()
        {
            return None;
        }
        Some(session.editor)
    }

    pub fn take_structured(
        &mut self,
        project: &NativeProject,
        name: &str,
        path: &str,
        kind: Option<AssetKind>,
    ) -> Option<AssetWorkbench> {
        let editor = self.take(
            project,
            &Key {
                member: project.member_alias().to_owned(),
                name: name.to_owned(),
                path: path.to_owned(),
                kind,
            },
        )?;
        if let Editor::Structured(editor) = editor {
            Some(*editor)
        } else {
            None
        }
    }

    pub fn take_sprite(&mut self, project: &NativeProject, name: &str) -> Option<SpriteEditor> {
        let declaration = project.manifest.assets.get(name)?;
        let editor = self.take(
            project,
            &Key {
                member: project.member_alias().to_owned(),
                name: name.to_owned(),
                path: declaration.path.clone(),
                kind: Some(declaration.kind),
            },
        )?;
        if let Editor::Sprite(editor) = editor {
            Some(*editor)
        } else {
            None
        }
    }
}

/// Conservative owned-allocation estimate, including JSON tree/container overhead.
pub fn json_bytes(value: &Value) -> usize {
    size_of::<Value>().saturating_add(match value {
        Value::String(text) => text.capacity(),
        Value::Array(values) => values
            .capacity()
            .saturating_mul(size_of::<Value>())
            .saturating_add(values.iter().map(json_bytes).sum::<usize>()),
        Value::Object(values) => values.iter().fold(0_usize, |bytes, (key, value)| {
            bytes
                .saturating_add(128)
                .saturating_add(key.capacity())
                .saturating_add(json_bytes(value))
        }),
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project() -> NativeProject {
        NativeProject::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/api-tour/cart.m01w"),
        )
        .unwrap()
    }

    #[test]
    fn structured_sessions_restore_history_but_never_cross_asset_paths_or_external_edits() {
        let mut project = project();
        let mut cache = AssetSessions::default();
        let source = std::str::from_utf8(&project.files["assets/loop.m01t"]).unwrap();
        let mut editor = AssetWorkbench::new(AssetKind::Music, source).unwrap();
        editor.replace_document(json!({"kind":"music","framesPerRow":240}));
        project.files.insert(
            "assets/loop.m01t".to_owned(),
            editor.source().unwrap().into_bytes(),
        );
        editor.dirty = false;
        assert!(cache.stash_structured(&project, "loop", "assets/loop.m01t", editor));
        assert!(
            cache
                .take_structured(&project, "loop", "different.m01t", Some(AssetKind::Music))
                .is_none()
        );
        let mut editor = cache
            .take_structured(&project, "loop", "assets/loop.m01t", Some(AssetKind::Music))
            .unwrap();
        editor.undo();
        assert_eq!(editor.document["framesPerRow"], 12);
        editor.redo();
        assert_eq!(editor.document["framesPerRow"], 240);
        cache.stash_structured(&project, "loop", "assets/loop.m01t", editor);
        project
            .files
            .insert("assets/loop.m01t".to_owned(), b"external edit".to_vec());
        assert!(
            cache
                .take_structured(&project, "loop", "assets/loop.m01t", Some(AssetKind::Music))
                .is_none()
        );
        assert_eq!(cache.bytes, 0);
    }

    #[test]
    fn sprite_sessions_restore_pixels_and_undo_but_validate_layered_source_too() {
        let mut project = project();
        let mut cache = AssetSessions::default();
        let mut editor = SpriteEditor::from_indexed(
            "dot".to_owned(),
            "assets/dot.m01g".to_owned(),
            &project.files["assets/dot.m01g"],
            &crate::sprite::palette(),
        )
        .unwrap();
        let original = editor.document.frames[0].layers[0].pixels.clone();
        editor.remember();
        editor.document.frames[0].layers[0].pixels[0] = 255;
        assert!(cache.stash_sprite(&project, editor));
        let mut editor = cache.take_sprite(&project, "dot").unwrap();
        assert_eq!(editor.document.frames[0].layers[0].pixels[0], 255);
        editor.undo();
        assert_eq!(editor.document.frames[0].layers[0].pixels, original);
        cache.stash_sprite(&project, editor);
        project.files.insert(
            "editor/sprites/dot.m01paint".to_owned(),
            b"new layered source".to_vec(),
        );
        assert!(cache.take_sprite(&project, "dot").is_none());
    }

    #[test]
    fn cache_evicts_only_sessions_not_project_files_and_clears_on_new_project() {
        let project = project();
        let before = project.files.clone();
        let mut cache = AssetSessions::default();
        for index in 0..100 {
            let editor = AssetWorkbench::new_display(r#"{"kind":"display"}"#).unwrap();
            cache.stash_structured(
                &project,
                &format!("display{index}"),
                "assets/loop.m01t",
                editor,
            );
        }
        assert_eq!(cache.sessions.len(), MAX_SESSIONS);
        assert!(cache.bytes <= MAX_RETAINED_BYTES);
        assert_eq!(project.files, before);
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert!(cache.sessions.is_empty());
    }
}
