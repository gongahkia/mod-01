//! Native editing/navigation over the shared compiler model; no second parser or scene engine.
use crate::{project::NativeProject, syntax::ModlHighlighter, ui_theme};
use eframe::egui::{
    self, Key, Modifiers, RichText,
    text::{CCursor, CCursorRange},
};
use modl_core::{
    FileId, SourceFile, Span,
    ast::{Item, Visibility},
    ir::SymbolKind,
    tooling::ProjectModel,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
};

const MAX_EDITOR_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct SourceEditor {
    identity: String,
    fingerprint: Option<u64>,
    model: Option<ProjectModel>,
    completion_model: Option<ProjectModel>,
    error: String,
    cursor: Range<usize>,
    pending: Option<Range<usize>>,
    back: Vec<(String, Range<usize>)>,
    query: String,
    replacement: String,
    match_case: bool,
    whole_word: bool,
    show_search: bool,
    show_replace: bool,
    focus_search: bool,
    focus_complete: bool,
    line: usize,
    show_complete: bool,
    completion_filter: String,
    completion_index: usize,
    show_graph: bool,
    graph_selected: String,
    message: String,
}

#[derive(Clone, Copy)]
enum EditAction {
    Indent,
    Outdent,
    Comment,
    Duplicate,
    Format,
}

impl SourceEditor {
    pub fn dismiss_completion(&mut self) {
        self.show_complete = false;
        self.focus_complete = false;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut NativeProject,
        highlighter: &mut ModlHighlighter,
    ) {
        self.refresh(project);
        let context = ui.ctx().clone();
        let id = editor_id(project);
        if let Some(state) = egui::TextEdit::load_state(&context, id)
            && let Some(range) = state.cursor.char_range()
        {
            let range = range.as_sorted_char_range();
            self.cursor = range.start.0..range.end.0;
        }
        let focused = context.memory(|memory| memory.has_focus(id));
        let owns_keyboard = ui.is_enabled()
            && context.memory(|memory| {
                memory.focused().is_none_or(|focused| {
                    focused == id || focused == find_query_id() || focused == replace_query_id()
                })
            });
        let mut edit = None;
        let mut next_match = None;
        let mut definition = false;
        let mut back = false;
        context.input_mut(|input| {
            if !owns_keyboard {
                return;
            }
            if self.show_search
                && !self.show_complete
                && input.consume_key(Modifiers::NONE, Key::Escape)
            {
                self.show_search = false;
                self.pending = Some(self.cursor.clone());
            }
            if input.consume_key(Modifiers::COMMAND, Key::F) {
                self.open_search(false);
            }
            if input.consume_key(Modifiers::COMMAND, Key::H) {
                self.open_search(true);
            }
            if input.consume_key(Modifiers::SHIFT, Key::F3) {
                next_match = Some(true);
            } else if input.consume_key(Modifiers::NONE, Key::F3) {
                next_match = Some(false);
            }
            if focused {
                if input.consume_key(Modifiers::SHIFT, Key::Tab) {
                    edit = Some(EditAction::Outdent);
                } else if input.consume_key(Modifiers::NONE, Key::Tab) {
                    edit = Some(EditAction::Indent);
                }
                if input.consume_key(Modifiers::COMMAND, Key::Slash) {
                    edit = Some(EditAction::Comment);
                }
                if input.consume_key(Modifiers::COMMAND, Key::D) {
                    edit = Some(EditAction::Duplicate);
                }
                if input.consume_key(Modifiers::CTRL, Key::Space) {
                    self.open_completion(project);
                }
                if input.consume_key(Modifiers::NONE, Key::F12) {
                    definition = true;
                }
                if input.consume_key(Modifiers::ALT, Key::ArrowLeft) {
                    back = true;
                }
            }
        });
        let mut navigation = None;
        let controls_height = (ui.available_height() * 0.4).clamp(100.0, 240.0);
        egui::ScrollArea::vertical()
            .id_salt("source-controls")
            .max_height(controls_height)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui_theme::legend(ui, &project.selected_file);
                    if ui
                        .button("FIND")
                        .on_hover_text("Ctrl/Cmd+F; F3 next, Shift+F3 previous")
                        .clicked()
                    {
                        self.open_search(false);
                    }
                    if ui.button("REPLACE").on_hover_text("Ctrl/Cmd+H").clicked() {
                        self.open_search(true);
                    }
                    ui.label("LINE");
                    ui.add(
                        egui::DragValue::new(&mut self.line)
                            .range(1..=project.editor_text.lines().count().max(1)),
                    );
                    if ui.button("GO").clicked() {
                        let offset = line_offset(&project.editor_text, self.line.max(1));
                        self.pending = Some(to_chars(&project.editor_text, offset..offset));
                    }
                    if ui.button("DEFINITION").on_hover_text("F12").clicked() {
                        definition = true;
                    }
                    if ui
                        .add_enabled(!self.back.is_empty(), egui::Button::new("BACK"))
                        .on_hover_text("Alt+Left")
                        .clicked()
                    {
                        back = true;
                    }
                    if ui.button("COMPLETE").on_hover_text("Ctrl+Space").clicked() {
                        self.open_completion(project);
                    }
                    if ui.button("FORMAT").clicked() {
                        edit = Some(EditAction::Format);
                    }
                    if ui.button("GRAPH").clicked() {
                        self.show_graph = true;
                    }
                });
                if let Some(model) = &self.model {
                    egui::ComboBox::from_id_salt("source-routines")
                        .selected_text("FUNCTION / STATE NAVIGATION")
                        .show_ui(ui, |ui| {
                            for node in model.nodes.iter().filter(|node| node.span.is_some()) {
                                let span = node.span.unwrap();
                                let path = model
                                    .source(span.file)
                                    .map_or("?", |source| source.path.as_str());
                                if ui
                                    .selectable_label(
                                        false,
                                        format!("{path} / {} ({})", node.label, node.kind),
                                    )
                                    .clicked()
                                {
                                    navigation = Some(span);
                                }
                            }
                        });
                }
                if self.show_search {
                    self.search_bar(ui, project, &mut next_match);
                }
            });
        if let Some(previous) = next_match {
            self.find(project, previous);
        }
        if definition {
            self.go_definition(project);
        }
        if back && let Some((path, range)) = self.back.pop() {
            match project.select_source(&path) {
                Ok(()) => self.pending = Some(range),
                Err(error) => self.message = error,
            }
        }
        if let Some(span) = navigation {
            self.jump(project, span);
        }
        if let Some(action) = edit {
            self.edit(project, &context, action);
        }

        let diagnostic_height = if self.error.is_empty()
            && self
                .model
                .as_ref()
                .is_none_or(|model| model.diagnostics.is_empty())
        {
            32.0
        } else {
            140.0
        };
        let message_height = if self.message.is_empty() { 0.0 } else { 24.0 };
        let height = (ui.available_height() - diagnostic_height - message_height - 12.0).max(60.0);
        let mut changed = false;
        let source_id = editor_id(project);
        egui::ScrollArea::both()
            .id_salt(("source-scroll", &project.selected_file))
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _width: f32| {
                    highlighter.layout(ui, text.as_str(), f32::INFINITY)
                };
                let mut output = egui::TextEdit::multiline(&mut project.editor_text)
                    .id(source_id)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_width(ui.available_width())
                    .desired_rows(30)
                    .layouter(&mut layouter)
                    .show(ui);
                changed = output.response.changed();
                if let Some(range) = output.cursor_range {
                    let range = range.as_sorted_char_range();
                    self.cursor = range.start.0..range.end.0;
                }
                if let Some(range) = self.pending.take() {
                    let count = project.editor_text.chars().count();
                    self.cursor = range.start.min(count)..range.end.min(count);
                    let range = cursor_range(self.cursor.clone());
                    output.state.cursor.set_char_range(Some(range));
                    output.state.store(&context, output.response.id);
                    output.response.request_focus();
                    ui.scroll_to_rect(
                        output
                            .galley
                            .pos_from_cursor(range.primary)
                            .translate(output.galley_pos.to_vec2()),
                        Some(egui::Align::Center),
                    );
                }
            });
        if changed {
            project.dirty = true;
            self.fingerprint = None;
            self.refresh(project);
        }
        let byte = char_byte(&project.editor_text, self.cursor.start);
        let file = SourceFile::new(FileId(0), "editor", &project.editor_text);
        let position = file.line_column(u32::try_from(byte).unwrap_or(u32::MAX));
        ui.label(
            RichText::new(format!(
                "LN {} : COL {}   TAB / SHIFT+TAB: INDENT   CTRL+/: COMMENT",
                position.line + 1,
                position.column + 1
            ))
            .small()
            .color(ui_theme::MUTED),
        );
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        self.diagnostics(ui, project);
        self.completion_window(&context, project);
        self.graph_window(&context, project);
    }

    fn refresh(&mut self, project: &mut NativeProject) {
        if self.identity != project.manifest.id {
            self.identity.clone_from(&project.manifest.id);
            self.completion_model = None;
            self.model = None;
            self.fingerprint = None;
            self.back.clear();
            self.cursor = 0..0;
        }
        project.sync_editor();
        let mut hash = DefaultHasher::new();
        project.manifest_source.hash(&mut hash);
        for (path, bytes) in &project.files {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "modl")
            {
                path.hash(&mut hash);
                bytes.hash(&mut hash);
            }
        }
        let fingerprint = hash.finish();
        if self.fingerprint == Some(fingerprint) {
            return;
        }
        self.fingerprint = Some(fingerprint);
        self.model =
            match modl_core::analyze_project_model(&project.manifest_source, &project.files) {
                Ok(model) => {
                    self.error.clear();
                    if model.diagnostics.is_empty() {
                        self.completion_model = Some(model.clone());
                    }
                    Some(model)
                }
                Err(error) => {
                    self.error = error.to_string();
                    None
                }
            };
    }

    fn open_search(&mut self, replace: bool) {
        self.show_search = true;
        self.show_replace = replace;
        self.focus_search = true;
    }

    fn search_bar(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut NativeProject,
        next: &mut Option<bool>,
    ) {
        let mut replace_one = false;
        let mut replace_all = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("FIND");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .id(find_query_id())
                    .desired_width(160.0),
            );
            if self.focus_search {
                response.request_focus();
                response.scroll_to_me(Some(egui::Align::Center));
                self.focus_search = false;
            }
            if response.gained_focus() {
                response.scroll_to_me(Some(egui::Align::Center));
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                *next = Some(false);
            }
            if ui.button("PREV").clicked() {
                *next = Some(true);
            }
            if ui.button("NEXT").clicked() {
                *next = Some(false);
            }
            ui.checkbox(&mut self.match_case, "CASE");
            ui.checkbox(&mut self.whole_word, "WORD");
            let count = search_matches(
                &project.editor_text,
                &self.query,
                self.match_case,
                self.whole_word,
            )
            .len();
            ui.label(format!("{count} matches"));
            if ui.button("CLOSE").clicked() {
                self.show_search = false;
            }
        });
        if self.show_replace {
            ui.horizontal_wrapped(|ui| {
                ui.label("WITH");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.replacement)
                        .id(replace_query_id())
                        .desired_width(160.0),
                );
                if response.gained_focus() {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
                replace_one = ui.button("REPLACE SELECTION").clicked();
                replace_all = ui.button("REPLACE ALL").clicked();
            });
        }
        if replace_one {
            let selected = char_byte(&project.editor_text, self.cursor.start)
                ..char_byte(&project.editor_text, self.cursor.end);
            if search_matches(
                &project.editor_text,
                &self.query,
                self.match_case,
                self.whole_word,
            )
            .contains(&selected)
            {
                let replacement = self.replacement.clone();
                self.replace_range(project, ui.ctx(), selected, &replacement);
            } else {
                self.find(project, false);
            }
        }
        if replace_all {
            match replace_matches(
                &project.editor_text,
                &self.query,
                &self.replacement,
                self.match_case,
                self.whole_word,
            ) {
                Ok((text, count)) => {
                    if count > 0 {
                        self.replace_range(project, ui.ctx(), 0..project.editor_text.len(), &text);
                    }
                    self.message = format!("REPLACED {count} MATCHES");
                }
                Err(error) => self.message = error,
            }
        }
    }

    fn find(&mut self, project: &NativeProject, previous: bool) {
        let matches = search_matches(
            &project.editor_text,
            &self.query,
            self.match_case,
            self.whole_word,
        );
        let byte = char_byte(
            &project.editor_text,
            if previous {
                self.cursor.start
            } else {
                self.cursor.end
            },
        );
        let found = if previous {
            matches
                .iter()
                .rev()
                .find(|range| range.start < byte)
                .or_else(|| matches.last())
        } else {
            matches
                .iter()
                .find(|range| range.start >= byte)
                .or_else(|| matches.first())
        };
        if let Some(range) = found {
            self.pending = Some(to_chars(&project.editor_text, range.clone()));
            self.message.clear();
        } else {
            self.message = "NO MATCHES".to_owned();
        }
    }

    fn replace_range(
        &mut self,
        project: &mut NativeProject,
        context: &egui::Context,
        bytes: Range<usize>,
        replacement: &str,
    ) {
        if project
            .editor_text
            .len()
            .saturating_sub(bytes.len())
            .saturating_add(replacement.len())
            > MAX_EDITOR_BYTES
        {
            self.message = "EDIT WOULD EXCEED 16 MiB".to_owned();
            return;
        }
        let id = editor_id(project);
        let mut state = egui::TextEdit::load_state(context, id).unwrap_or_default();
        let mut undoer = state.undoer();
        undoer.add_undo(&(
            cursor_range(self.cursor.clone()),
            project.editor_text.clone(),
        ));
        let start = project.editor_text[..bytes.start].chars().count();
        project.editor_text.replace_range(bytes, replacement);
        let selection = start..start + replacement.chars().count();
        state
            .cursor
            .set_char_range(Some(cursor_range(selection.clone())));
        state.set_undoer(undoer);
        state.store(context, id);
        self.pending = Some(selection);
        project.dirty = true;
        self.fingerprint = None;
    }

    fn edit(&mut self, project: &mut NativeProject, context: &egui::Context, action: EditAction) {
        if matches!(action, EditAction::Format) {
            let source = SourceFile::new(FileId(0), &project.selected_file, &project.editor_text);
            match modl_core::format_source(&source) {
                Ok(text) => {
                    self.replace_range(project, context, 0..project.editor_text.len(), &text)
                }
                Err(error) => self.message = format!("FORMAT: {:?}", error.diagnostics),
            }
            return;
        }
        let bytes = char_byte(&project.editor_text, self.cursor.start)
            ..char_byte(&project.editor_text, self.cursor.end);
        let lines = selected_lines(&project.editor_text, bytes);
        let text = transform_lines(&project.editor_text[lines.clone()], action);
        self.replace_range(project, context, lines, &text);
    }

    fn go_definition(&mut self, project: &mut NativeProject) {
        let target = self.model.as_ref().and_then(|model| {
            let source = model
                .sources
                .iter()
                .find(|source| source.path == project.selected_file)?;
            let offset = u32::try_from(char_byte(&project.editor_text, self.cursor.start)).ok()?;
            model
                .definition_at(source.id, offset)
                .or_else(|| model.definition_at(source.id, offset.saturating_sub(1)))
        });
        if let Some(span) = target {
            self.jump(project, span);
        } else {
            self.message =
                "NO RESOLVED SOURCE DEFINITION AT CURSOR / FIX CURRENT DIAGNOSTICS FIRST"
                    .to_owned();
        }
    }

    fn jump(&mut self, project: &mut NativeProject, span: Span) {
        let Some(path) = self
            .model
            .as_ref()
            .and_then(|model| model.source(span.file))
            .map(|source| source.path.clone())
        else {
            return;
        };
        self.back
            .push((project.selected_file.clone(), self.cursor.clone()));
        self.message.clear();
        if self.back.len() > 64 {
            self.back.remove(0);
        }
        match project.select_source(&path) {
            Ok(()) => {
                self.pending = Some(to_chars(
                    &project.editor_text,
                    span.start as usize..span.end as usize,
                ))
            }
            Err(error) => self.message = error,
        }
    }

    fn diagnostics(&mut self, ui: &mut egui::Ui, project: &mut NativeProject) {
        if !self.error.is_empty() {
            ui.colored_label(ui_theme::CORAL, &self.error);
        }
        let mut target = None;
        if let Some(model) = &self.model {
            if model.diagnostics.is_empty() {
                ui.colored_label(ui_theme::CYAN, "COMPILER: SOURCE TYPES CHECKED");
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("source-diagnostics")
                    .max_height(110.0)
                    .show(ui, |ui| {
                        for diagnostic in &model.diagnostics {
                            let span = diagnostic.primary.span;
                            let Some(source) = model.source(span.file) else {
                                continue;
                            };
                            let file = SourceFile::new(source.id, &source.path, &source.text);
                            let position = file.line_column(span.start);
                            if ui
                                .button(
                                    RichText::new(format!(
                                        "{}:{}:{} [{}] {}",
                                        source.path,
                                        position.line + 1,
                                        position.column + 1,
                                        diagnostic.code,
                                        diagnostic.message
                                    ))
                                    .color(ui_theme::CORAL),
                                )
                                .on_hover_text(format!(
                                    "{}\n{}",
                                    diagnostic.primary.message,
                                    diagnostic.notes.join("\n")
                                ))
                                .clicked()
                            {
                                target = Some(span);
                            }
                        }
                    });
            }
        }
        if let Some(span) = target {
            self.jump(project, span);
        }
    }

    fn open_completion(&mut self, project: &NativeProject) {
        let start = char_byte(&project.editor_text, self.cursor.start);
        let end = char_byte(&project.editor_text, self.cursor.end);
        self.completion_filter = if start < end {
            &project.editor_text[start..end]
        } else {
            completion_prefix(&project.editor_text, end)
        }
        .to_owned();
        self.completion_index = 0;
        self.show_complete = true;
        self.focus_complete = true;
    }

    fn completion_window(&mut self, context: &egui::Context, project: &mut NativeProject) {
        if !self.show_complete {
            return;
        }
        let completion_id = egui::Id::unique("native-source-completion-filter");
        let owns_keys = context.memory(|memory| {
            memory.has_focus(completion_id)
                || self.focus_complete && memory.has_focus(editor_id(project))
        });
        if owns_keys && context.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            self.show_complete = false;
            self.pending = Some(self.cursor.clone());
            return;
        }
        let mut open = true;
        let mut chosen = None;
        egui::Window::new("SOURCE COMPLETION / COMPILER API")
            .open(&mut open)
            .default_width(610.0)
            .show(context, |ui| {
                let model = self
                    .model
                    .as_ref()
                    .filter(|model| !model.symbols.is_empty())
                    .or(self.completion_model.as_ref());
                let candidates = completion_candidates(
                    project,
                    model,
                    &self.completion_filter,
                    char_byte(&project.editor_text, self.cursor.start),
                );
                self.completion_index = self
                    .completion_index
                    .min(candidates.len().saturating_sub(1));
                context.input_mut(|input| {
                    if !owns_keys {
                        return;
                    }
                    if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                        self.completion_index =
                            (self.completion_index + 1).min(candidates.len().saturating_sub(1));
                    }
                    if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                        self.completion_index = self.completion_index.saturating_sub(1);
                    }
                    if input.consume_key(Modifiers::NONE, Key::Enter) {
                        chosen = candidates
                            .get(self.completion_index)
                            .map(|item| item.0.clone());
                    }
                });
                let response = ui
                    .add(egui::TextEdit::singleline(&mut self.completion_filter).id(completion_id));
                if self.focus_complete {
                    response.request_focus();
                    self.focus_complete = false;
                }
                ui.label("Choose an identifier; compiler still validates the completed call.");
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (index, (name, detail)) in candidates.iter().take(150).enumerate() {
                            if ui
                                .selectable_label(
                                    self.completion_index == index,
                                    format!("{name}  —  {detail}"),
                                )
                                .clicked()
                            {
                                chosen = Some(name.clone());
                            }
                        }
                    });
            });
        self.show_complete = open;
        if let Some(name) = chosen {
            let start = char_byte(&project.editor_text, self.cursor.start);
            let end = char_byte(&project.editor_text, self.cursor.end);
            let start = if start < end {
                start
            } else {
                end - completion_prefix(&project.editor_text, end).len()
            };
            self.replace_range(project, context, start..end, &name);
            if let Some(range) = &mut self.pending {
                range.start = range.end;
            }
            self.show_complete = false;
        }
    }

    fn graph_window(&mut self, context: &egui::Context, project: &mut NativeProject) {
        if !self.show_graph {
            return;
        }
        let mut open = true;
        let mut target = None;
        egui::Window::new("COMPILER CODE / STATE GRAPH").open(&mut open).default_size([1050.0, 650.0]).show(context, |ui| {
            ui.label("Read-only resolved IR: callbacks → routines → state / assets. Click a node or edge to inspect source.");
            let Some(model) = &self.model else { ui.label("Resolve project errors to build a graph."); return; };
            if !model.diagnostics.is_empty() { ui.label("Invalid source: no executable graph."); return; }
            egui::ScrollArea::both().id_salt("code-graph-canvas").max_height(400.0).show(ui, |ui| {
                let mut rows = [0_usize; 4];
                let mut positions = BTreeMap::new();
                for node in &model.nodes {
                    let column = match node.kind.as_str() { "Callback" => 0, "Function" | "Task" | "Record" | "Enum" => 1, "State" | "Constant" => 2, _ => 3 };
                    positions.insert(node.id.clone(), egui::pos2(20.0 + column as f32 * 250.0, 20.0 + rows[column] as f32 * 68.0)); rows[column] += 1;
                }
                let size = egui::vec2(1020.0, (rows.into_iter().max().unwrap_or(1) as f32 * 68.0 + 40.0).max(150.0));
                let (canvas, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                for edge in &model.edges {
                    let (Some(from), Some(to)) = (positions.get(&edge.from), positions.get(&edge.to)) else { continue; };
                    let start = canvas.min + from.to_vec2() + egui::vec2(105.0, 22.0);
                    let end = canvas.min + to.to_vec2() + egui::vec2(105.0, 22.0);
                    let color = if self.graph_selected == edge.from || self.graph_selected == edge.to { ui_theme::AMBER } else { ui_theme::EDGE };
                    let delta = end - start;
                    if delta.length_sq() > 0.0 {
                        let offset = delta * (105.0 / delta.x.abs()).min(22.0 / delta.y.abs());
                        ui.painter().arrow(start + offset, end - offset - (start + offset), egui::Stroke::new(1.0, color));
                    }
                }
                for node in &model.nodes {
                    let rect = egui::Rect::from_min_size(canvas.min + positions[&node.id].to_vec2(), egui::vec2(210.0, 44.0));
                    if ui.put(rect, egui::Button::new(format!("{} / {}", node.label, node.kind)).selected(self.graph_selected == node.id)).clicked() {
                        self.graph_selected.clone_from(&node.id); target = node.span;
                    }
                }
            });
            egui::ScrollArea::vertical().id_salt("graph-edges").max_height(150.0).show(ui, |ui| {
                for edge in model.edges.iter().filter(|edge| self.graph_selected.is_empty() || edge.from == self.graph_selected || edge.to == self.graph_selected) {
                    let label = |id: &str| model.nodes.iter().find(|node| node.id == id).map_or(id.to_owned(), |node| node.label.clone());
                    if ui.button(format!("{} — {:?} → {}", label(&edge.from), edge.kind, label(&edge.to))).clicked() { target = Some(edge.span); }
                }
            });
        });
        self.show_graph = open;
        if let Some(span) = target {
            self.jump(project, span);
        }
    }
}

fn editor_id(project: &NativeProject) -> egui::Id {
    egui::Id::unique((
        "native-modl-source",
        &project.manifest.id,
        &project.selected_file,
    ))
}
fn find_query_id() -> egui::Id {
    egui::Id::unique("native-source-find-query")
}
fn replace_query_id() -> egui::Id {
    egui::Id::unique("native-source-replace-query")
}
fn cursor_range(range: Range<usize>) -> CCursorRange {
    CCursorRange::two(CCursor::new(range.start), CCursor::new(range.end))
}
fn char_byte(source: &str, index: usize) -> usize {
    source
        .char_indices()
        .nth(index)
        .map_or(source.len(), |pair| pair.0)
}
fn to_chars(source: &str, bytes: Range<usize>) -> Range<usize> {
    let start = bytes.start.min(source.len());
    let end = bytes.end.min(source.len());
    let floor = |mut index: usize| {
        while !source.is_char_boundary(index) {
            index -= 1;
        }
        index
    };
    source[..floor(start)].chars().count()..source[..floor(end)].chars().count()
}
fn line_offset(source: &str, line: usize) -> usize {
    if line <= 1 {
        0
    } else {
        source
            .match_indices('\n')
            .nth(line - 2)
            .map_or(source.len(), |pair| pair.0 + 1)
    }
}
fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn search_matches(
    source: &str,
    query: &str,
    match_case: bool,
    whole_word: bool,
) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let (source, query) = if match_case {
        (source.to_owned(), query.to_owned())
    } else {
        (source.to_ascii_lowercase(), query.to_ascii_lowercase())
    };
    source
        .match_indices(&query)
        .filter_map(|(start, found)| {
            let end = start + found.len();
            (!whole_word
                || (start == 0 || !word(source.as_bytes()[start - 1]))
                    && (end == source.len() || !word(source.as_bytes()[end])))
            .then_some(start..end)
        })
        .collect()
}

fn replace_matches(
    source: &str,
    query: &str,
    replacement: &str,
    match_case: bool,
    whole_word: bool,
) -> Result<(String, usize), String> {
    let matches = search_matches(source, query, match_case, whole_word);
    let count = matches.len();
    let size = count
        .checked_mul(replacement.len())
        .and_then(|size| size.checked_add(source.len().saturating_sub(count * query.len())))
        .filter(|size| *size <= MAX_EDITOR_BYTES)
        .ok_or("REPLACEMENT WOULD EXCEED 16 MiB")?;
    let mut text = String::with_capacity(size);
    let mut cursor = 0;
    for range in matches {
        text.push_str(&source[cursor..range.start]);
        text.push_str(replacement);
        cursor = range.end;
    }
    text.push_str(&source[cursor..]);
    Ok((text, count))
}

fn selected_lines(source: &str, selected: Range<usize>) -> Range<usize> {
    let start = source[..selected.start]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let end = if selected.end > selected.start
        && source.as_bytes().get(selected.end - 1) == Some(&b'\n')
    {
        selected.end
    } else {
        source[selected.end..]
            .find('\n')
            .map_or(source.len(), |offset| selected.end + offset + 1)
    };
    start..end
}

fn transform_lines(text: &str, action: EditAction) -> String {
    if matches!(action, EditAction::Duplicate) {
        return format!(
            "{text}{}{text}",
            if text.ends_with('\n') { "" } else { "\n" }
        );
    }
    let uncomment = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| line.trim_start().starts_with("//"));
    text.split_inclusive('\n')
        .map(|line| match action {
            EditAction::Indent => format!("  {line}"),
            EditAction::Outdent => line
                .strip_prefix("  ")
                .or_else(|| line.strip_prefix(' '))
                .unwrap_or(line)
                .to_owned(),
            EditAction::Comment if !line.trim().is_empty() => {
                let indent = line.len() - line.trim_start_matches(' ').len();
                if uncomment {
                    let tail = &line[indent + 2..];
                    format!(
                        "{}{}",
                        &line[..indent],
                        tail.strip_prefix(' ').unwrap_or(tail)
                    )
                } else {
                    format!("{}// {}", &line[..indent], &line[indent..])
                }
            }
            _ => line.to_owned(),
        })
        .collect()
}

fn completion_prefix(source: &str, end: usize) -> &str {
    let prefix = &source[..end];
    let start = prefix
        .rfind(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '#'))
        })
        .map_or(0, |index| {
            index + prefix[index..].chars().next().unwrap().len_utf8()
        });
    &source[start..end]
}

fn completion_candidates(
    project: &NativeProject,
    model: Option<&ProjectModel>,
    prefix: &str,
    cursor: usize,
) -> Vec<(String, String)> {
    let mut items = BTreeMap::new();
    for definition in modl_core::builtin_definitions() {
        items.insert(definition.name.clone(), definition.signature.clone());
    }
    for (name, asset) in &project.manifest.assets {
        items.insert(format!("#{name}"), asset.kind.type_name().to_owned());
    }
    if let Some(model) = model {
        let current = modl_core::parse(&SourceFile::new(
            FileId(0),
            &project.selected_file,
            &project.editor_text,
        ))
        .module;
        for symbol in &model.symbols {
            let Some(span) = symbol.defined_at else {
                continue;
            };
            let Some(source) = model.source(span.file) else {
                continue;
            };
            if source.path == project.selected_file {
                let local = matches!(symbol.kind, SymbolKind::Local | SymbolKind::Parameter);
                let same_routine = current.items.iter().any(|item| {
                    let range = match item {
                        Item::Function(item) => item.span,
                        Item::Task(item) => item.span,
                        Item::Callback(item) => item.span,
                        _ => return false,
                    };
                    range.start <= span.start
                        && span.end <= range.end
                        && range.contains(u32::try_from(cursor).unwrap_or(u32::MAX))
                });
                if !local || same_routine && (span.start as usize) < cursor {
                    items.insert(symbol.name.clone(), symbol.detail.clone());
                }
            }
        }
        for item in &current.items {
            let Item::Import(import) = item else {
                continue;
            };
            let path = format!(
                "{}.modl",
                import
                    .path
                    .iter()
                    .map(|name| name.value.as_str())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            let alias = import
                .alias
                .as_ref()
                .or_else(|| import.path.last())
                .map_or("", |name| name.value.as_str());
            let Some(source) = model.sources.iter().find(|source| source.path == path) else {
                continue;
            };
            let parsed = modl_core::parse(&SourceFile::new(source.id, &source.path, &source.text));
            let public = parsed
                .module
                .items
                .iter()
                .filter_map(|item| {
                    let (visibility, name) = match item {
                        Item::Function(item) => (item.visibility, &item.name),
                        Item::Task(item) => (item.visibility, &item.name),
                        Item::Constant(item) => (item.visibility, &item.name),
                        Item::State(item) => (item.visibility, &item.name),
                        Item::Record(item) => (item.visibility, &item.name),
                        Item::Enum(item) => (item.visibility, &item.name),
                        _ => return None,
                    };
                    (visibility == Visibility::Public).then_some(name.value.as_str())
                })
                .collect::<BTreeSet<_>>();
            for symbol in &model.symbols {
                if matches!(
                    symbol.kind,
                    SymbolKind::Function
                        | SymbolKind::Task
                        | SymbolKind::State
                        | SymbolKind::Constant
                        | SymbolKind::Record
                        | SymbolKind::Enum
                ) && symbol.defined_at.is_some_and(|span| span.file == source.id)
                    && public.contains(symbol.name.as_str())
                {
                    items.insert(format!("{alias}.{}", symbol.name), symbol.detail.clone());
                }
            }
        }
    }
    items
        .into_iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn editor_frame(
        editor: &mut SourceEditor,
        project: &mut NativeProject,
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        let mut highlighter = ModlHighlighter::default();
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, project, &mut highlighter);
                },
            )
            .drop_without_applying_deltas();
    }

    #[test]
    fn native_text_widget_shortcuts_find_select_outdent_and_undo_actual_source() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/blank/cart.m01w");
        let mut project = NativeProject::open(&path).unwrap();
        project.editor_text = "on draw:\n  clear(1)\n".to_owned();
        let original = project.editor_text.clone();
        let context = egui::Context::default();
        let mut editor = SourceEditor {
            pending: Some(9..9),
            ..Default::default()
        };
        editor_frame(&mut editor, &mut project, &context, vec![]);
        // A newly focused text widget installs its focus-lock filter on the following pass.
        editor_frame(&mut editor, &mut project, &context, vec![]);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Tab, Modifiers::SHIFT)],
        );
        assert_eq!(project.editor_text, "on draw:\nclear(1)\n");
        assert!(project.dirty);
        assert!(
            context.memory(|memory| memory.has_focus(editor_id(&project))),
            "source focus lost after block edit"
        );
        let state = egui::TextEdit::load_state(&context, editor_id(&project)).unwrap();
        assert!(
            state.undoer().has_undo(&(
                state.cursor.char_range().unwrap(),
                project.editor_text.clone()
            )),
            "block edit has no undo point"
        );
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND)],
        );
        assert_eq!(project.editor_text, original);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            )],
        );
        assert_eq!(project.editor_text, "on draw:\nclear(1)\n");
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND)],
        );
        assert_eq!(project.editor_text, original);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::F, Modifiers::CTRL | Modifiers::COMMAND)],
        );
        assert!(editor.show_search);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![egui::Event::Text("clear".to_owned())],
        );
        assert_eq!(editor.query, "clear");
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::F3, Modifiers::NONE)],
        );
        assert_eq!(&project.editor_text[editor.cursor.clone()], "clear");
    }

    #[test]
    fn completion_keyboard_accepts_a_prefix_and_replaces_a_selection_without_duplicating_it() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/blank/cart.m01w");
        let mut project = NativeProject::open(&path).unwrap();
        project.editor_text = "on draw:\n  cl\n".to_owned();
        let context = egui::Context::default();
        let mut editor = SourceEditor {
            pending: Some(13..13),
            ..Default::default()
        };
        for _ in 0..2 {
            editor_frame(&mut editor, &mut project, &context, vec![]);
        }
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Space, Modifiers::CTRL | Modifiers::COMMAND)],
        );
        assert!(editor.show_complete);
        assert_eq!(editor.completion_filter, "cl");
        editor_frame(&mut editor, &mut project, &context, vec![]);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::ArrowDown, Modifiers::NONE)],
        );
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert!(!editor.show_complete);
        assert_eq!(project.editor_text, "on draw:\n  clear\n");
        editor_frame(&mut editor, &mut project, &context, vec![]);
        editor.pending = Some(11..16);
        editor_frame(&mut editor, &mut project, &context, vec![]);
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Space, Modifiers::CTRL | Modifiers::COMMAND)],
        );
        assert_eq!(editor.completion_filter, "clear");
        editor_frame(
            &mut editor,
            &mut project,
            &context,
            vec![key(Key::Enter, Modifiers::NONE)],
        );
        assert_eq!(project.editor_text, "on draw:\n  clear\n");
        assert_eq!(editor.pending, Some(16..16));
    }

    #[test]
    fn search_diagnostics_and_messages_fit_the_minimum_native_workspace() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/blank/cart.m01w");
        let mut project = NativeProject::open(&path).unwrap();
        project.editor_text = "on draw:\n  let bad = missing()\n".to_owned();
        let context = egui::Context::default();
        ui_theme::install(&context);
        crate::ui_assets::install_font(&context);
        let mut editor = SourceEditor {
            show_search: true,
            show_replace: true,
            message: "REPLACED 1 MATCH".to_owned(),
            ..Default::default()
        };
        let mut highlighter = ModlHighlighter::default();
        let viewport =
            egui::Rect::from_min_size(egui::pos2(218.0, 110.0), egui::vec2(682.0, 430.0));
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, &mut project, &mut highlighter);
                    assert!(
                        ui.min_rect().bottom() <= viewport.bottom(),
                        "editor overflow: {:?}",
                        ui.min_rect()
                    );
                },
            )
            .drop_without_applying_deltas();
    }

    #[test]
    fn another_tool_window_owns_its_keyboard_without_editing_the_cartridge() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/blank/cart.m01w");
        let mut project = NativeProject::open(&path).unwrap();
        let original = project.editor_text.clone();
        let context = egui::Context::default();
        let mut editor = SourceEditor {
            show_complete: true,
            ..Default::default()
        };
        let mut foreign = String::new();
        let id = egui::Id::unique("other-native-tool-input");
        for events in [
            vec![],
            vec![
                key(Key::F, Modifiers::CTRL | Modifiers::COMMAND),
                key(Key::Enter, Modifiers::NONE),
            ],
        ] {
            context.memory_mut(|memory| memory.request_focus(id));
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.add(egui::TextEdit::multiline(&mut foreign).id(id));
                        editor.show(ui, &mut project, &mut ModlHighlighter::default());
                    },
                )
                .drop_without_applying_deltas();
        }
        assert!(!editor.show_search);
        assert_eq!(project.editor_text, original);
        assert!(editor.show_complete);
    }
    #[test]
    fn find_replace_respects_words_case_unicode_and_replacement_bounds() {
        assert_eq!(
            search_matches("Score score scorer", "score", false, true),
            vec![0..5, 6..11]
        );
        assert_eq!(
            search_matches("Score score", "score", true, true),
            vec![6..11]
        );
        assert!(search_matches("test", "", false, false).is_empty());
        assert_eq!(
            replace_matches("x x", "x", "xx", true, false).unwrap(),
            ("xx xx".to_owned(), 2)
        );
        assert_eq!(
            replace_matches("e 🐝 e", "e", "z", true, true).unwrap().0,
            "z 🐝 z"
        );
        assert!(replace_matches("xx", "x", &"a".repeat(MAX_EDITOR_BYTES), true, false).is_err());
        assert_eq!(to_chars("a🐝z", 1..5), 1..2);
        assert_eq!(char_byte("a🐝z", 2), 5);
    }
    #[test]
    fn block_actions_preserve_indentation_and_selected_newline_boundaries() {
        let source = "fn first():\n  let x = 1\n  let y = 2\nnext\n";
        let start = source.find("  let").unwrap();
        let end = source.find("next").unwrap();
        assert_eq!(selected_lines(source, start..end), start..end);
        let block = &source[start..end];
        assert_eq!(
            transform_lines(
                &transform_lines(block, EditAction::Indent),
                EditAction::Outdent
            ),
            block
        );
        assert_eq!(
            transform_lines(
                &transform_lines(block, EditAction::Comment),
                EditAction::Comment
            ),
            block
        );
        assert_eq!(transform_lines("foo", EditAction::Duplicate), "foo\nfoo");
        assert_eq!(line_offset(source, 2), start);
        assert_eq!(completion_prefix("math.st", 7), "math.st");
    }
    #[test]
    fn completion_uses_the_full_api_assets_and_qualified_public_imports() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let project = NativeProject::open(&path).unwrap();
        let model =
            modl_core::analyze_project_model(&project.manifest_source, &project.files).unwrap();
        for name in ["sqrt", "mem_copy", "text_split", "bit_ushr"] {
            assert!(
                completion_candidates(&project, Some(&model), name, 0)
                    .iter()
                    .any(|item| item.0 == name)
            );
        }
        assert!(
            completion_candidates(&project, Some(&model), "#", 0)
                .iter()
                .all(|item| item.0.starts_with('#'))
        );
        let items = completion_candidates(&project, Some(&model), "motion.", 0);
        assert!(items.iter().any(|item| item.0 == "motion.wrap"));
    }
}
