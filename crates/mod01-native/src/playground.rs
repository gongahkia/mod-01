//! Isolated MODL scratch evaluations using the actual compiler and native console process.
use crate::{
    native_host_path, project::NativeProject, replay, runtime::RuntimeBridge, sprite,
    syntax::ModlHighlighter, ui_theme,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use eframe::egui::{
    self, Color32, ColorImage, Key, RichText, TextureHandle, TextureOptions, text::CCursorRange,
};
use modl_core::{PlaygroundMode, ProjectManifest, playground_source};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

const ENTRY: &str = "scratch/main.modl";
const HISTORY_LIMIT: usize = 64;
const OUTPUT_LIMIT: usize = 8192;

#[derive(Clone)]
struct HistoryEntry {
    mode: PlaygroundMode,
    input: String,
    definitions: String,
    use_context: bool,
    seed: u32,
    output: String,
    failed: bool,
}

struct Prepared {
    command: Value,
    source: String,
    result_name: Option<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Compiling,
    Loading,
    Evaluating,
    Closing,
    Stopping,
}

pub struct Playground {
    runtime: RuntimeBridge,
    compilation: Option<Receiver<Result<Prepared, String>>>,
    phase: Phase,
    input: String,
    definitions: String,
    mode: PlaygroundMode,
    use_context: bool,
    seed: u32,
    pending: Option<HistoryEntry>,
    result_name: Option<String>,
    history: Vec<HistoryEntry>,
    output: String,
    status: String,
    failed: bool,
    source: String,
    show_source: bool,
    focus_input: bool,
    texture: Option<TextureHandle>,
    highlighter: ModlHighlighter,
}

impl Playground {
    pub fn new() -> Self {
        Self {
            runtime: RuntimeBridge::new(native_host_path()),
            compilation: None,
            phase: Phase::Idle,
            input: "sqrt(81.0) + 1.0".to_owned(),
            definitions: String::new(),
            mode: PlaygroundMode::Expression,
            use_context: false,
            seed: modl_core::DEFAULT_SEED,
            pending: None,
            result_name: None,
            history: Vec::new(),
            output: String::new(),
            status: "FRESH ISOLATED MACHINE / NO PROJECT OR SAVE WRITES".to_owned(),
            failed: false,
            source: String::new(),
            show_source: false,
            focus_input: true,
            texture: None,
            highlighter: ModlHighlighter::default(),
        }
    }

    pub fn focus(&mut self) {
        self.focus_input = true;
    }

    pub fn poll(&mut self, context: &egui::Context) {
        if let Some(receiver) = &self.compilation {
            match receiver.try_recv() {
                Ok(prepared) => {
                    self.compilation = None;
                    if self.phase == Phase::Stopping {
                        self.cancelled();
                    } else {
                        match prepared {
                            Ok(prepared) => {
                                self.source = prepared.source;
                                self.result_name = prepared.result_name;
                                match self.runtime.send(prepared.command) {
                                    Ok(()) => {
                                        self.phase = Phase::Loading;
                                        self.status = "STARTING ISOLATED CONSOLE".to_owned();
                                    }
                                    Err(error) => self.finish(error, true),
                                }
                            }
                            Err(error) => self.finish(error, true),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.compilation = None;
                    self.finish("scratch compiler worker stopped".to_owned(), true);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        while let Some(event) = self.runtime.poll() {
            match event["type"].as_str() {
                Some("loaded") if self.phase == Phase::Loading => {
                    match self
                        .runtime
                        .send(json!({"type":"scratch", "input":replay::empty_input()}))
                    {
                        Ok(()) => {
                            self.phase = Phase::Evaluating;
                            self.status = "EVALUATING MODL / ACTUAL CONSOLE CORE".to_owned();
                        }
                        Err(error) => self.finish(error, true),
                    }
                }
                Some("scratch") if self.phase == Phase::Evaluating => {
                    match preview_image(&event) {
                        Ok(image) => {
                            if let Some(texture) = self.texture.as_mut() {
                                texture.set(image, TextureOptions::NEAREST);
                            } else {
                                self.texture = Some(context.load_texture(
                                    "scratch-frame",
                                    image,
                                    TextureOptions::NEAREST,
                                ));
                            }
                            let values = if let Some(name) = &self.result_name {
                                event["locals"]
                                    .get(name)
                                    .map_or_else(|| "Unit".to_owned(), pretty_value)
                            } else {
                                pretty_value(
                                    &json!({"locals": event["locals"], "stateAfterFrame": event["state"]}),
                                )
                            };
                            self.finish(
                                format!(
                                    "{values}\nBOOT WORK {} / FRAME WORK {}\nFRESH SEED {}",
                                    event["bootWorkUnits"],
                                    event["workUnits"],
                                    self.pending.as_ref().map_or(self.seed, |entry| entry.seed)
                                ),
                                false,
                            );
                        }
                        Err(error) => self.finish(error, true),
                    }
                    match self.runtime.send(json!({"type":"stop"})) {
                        Ok(()) => {
                            self.phase = Phase::Closing;
                            self.status = if self.failed {
                                "FAILED / CLOSING ISOLATED MACHINE"
                            } else {
                                "COMPLETE / CLOSING ISOLATED MACHINE"
                            }
                            .to_owned();
                        }
                        Err(error) => self.finish(error, true),
                    }
                }
                Some("stopped") if self.phase == Phase::Closing => {
                    self.phase = Phase::Idle;
                    self.status = if self.failed {
                        "EVALUATION FAILED / PREVIOUS PREVIEW RETAINED"
                    } else {
                        "COMPLETE / ISOLATED MACHINE DISCARDED"
                    }
                    .to_owned();
                }
                Some("stopped") if self.phase == Phase::Stopping => self.cancelled(),
                Some("error") => {
                    if self.phase == Phase::Stopping {
                        self.cancelled();
                    } else {
                        let location = event.get("sourceSpan").map_or_else(String::new, |span| {
                            format!("\nCOMPILED SOURCE BYTES {}..{}", span["start"], span["end"])
                        });
                        self.finish(
                            format!(
                                "{}: {}{location}",
                                event["code"].as_str().unwrap_or("M019100"),
                                event["message"]
                                    .as_str()
                                    .unwrap_or("scratchpad runtime failed")
                            ),
                            true,
                        );
                    }
                }
                _ => {}
            }
        }
        if self.phase != Phase::Idle {
            context.request_repaint_after(Duration::from_millis(8));
        }
    }

    fn evaluate(&mut self, project: Option<&NativeProject>) {
        if self.phase != Phase::Idle {
            return;
        }
        let entry = HistoryEntry {
            mode: self.mode,
            input: self.input.clone(),
            definitions: self.definitions.clone(),
            use_context: self.use_context,
            seed: self.seed,
            output: String::new(),
            failed: false,
        };
        let context = if self.use_context {
            project.cloned()
        } else {
            None
        };
        let request = entry.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        self.pending = Some(entry);
        self.failed = false;
        self.source.clear();
        match playground_source(self.mode, &self.input, &self.definitions) {
            Ok(wrapped) => self.source = wrapped.source,
            Err(error) => {
                self.finish(error.to_string(), true);
                return;
            }
        }
        self.status = "COMPILING MODL / ACTIVE CARTRIDGE UNCHANGED".to_owned();
        match std::thread::Builder::new()
            .name("modl-playground-compiler".to_owned())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let result = prepare(
                    request.mode,
                    &request.input,
                    &request.definitions,
                    request.seed,
                    context.as_ref(),
                );
                let _ = sender.send(result);
            }) {
            Ok(_) => {
                self.compilation = Some(receiver);
                self.phase = Phase::Compiling;
            }
            Err(error) => self.finish(error.to_string(), true),
        }
    }

    fn cancel(&mut self) {
        if self.phase == Phase::Idle || self.phase == Phase::Closing {
            return;
        }
        self.phase = Phase::Stopping;
        self.status = "CANCELLING ISOLATED EVALUATION".to_owned();
        if self.compilation.is_none()
            && let Err(error) = self.runtime.send(json!({"type":"stop"}))
        {
            self.finish(error, true);
        }
    }

    fn cancelled(&mut self) {
        self.phase = Phase::Idle;
        self.pending = None;
        self.status = "CANCELLED / ACTIVE CARTRIDGE UNCHANGED".to_owned();
    }

    fn finish(&mut self, output: String, failed: bool) {
        self.output = bounded_output(&output);
        self.failed = failed;
        self.status = if failed {
            "EVALUATION FAILED / PREVIOUS PREVIEW RETAINED"
        } else {
            "COMPLETE / ISOLATED MACHINE DISCARDED"
        }
        .to_owned();
        self.phase = Phase::Idle;
        if let Some(mut entry) = self.pending.take() {
            entry.output.clone_from(&self.output);
            entry.failed = failed;
            self.history.push(entry);
            if self.history.len() > HISTORY_LIMIT {
                self.history.remove(0);
            }
        }
    }

    pub fn ui(
        &mut self,
        context: &egui::Context,
        open: &mut bool,
        project: Option<&NativeProject>,
    ) -> Option<egui::Rect> {
        let mut evaluate = false;
        let mut cancel = false;
        let mut restore = None;
        if project.is_none() {
            self.use_context = false;
        }
        let input_id = egui::Id::unique("modl-playground-input");
        let definitions_id = egui::Id::unique("modl-playground-definitions");
        let focused =
            context.memory(|memory| memory.has_focus(input_id) || memory.has_focus(definitions_id));
        if focused
            && context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, Key::Enter))
        {
            evaluate = true;
        }
        let window = egui::Window::new("MODL PLAYGROUND / ISOLATED SCRATCHPAD").open(open).default_size([1060.0, 700.0]).vscroll(true).show(context, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.label("Ordinary MODL compiler + production console. Each evaluation restarts its own machine; your cartridge and saves are not changed.");
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.mode, PlaygroundMode::Expression, "EXPRESSION");
                ui.selectable_value(&mut self.mode, PlaygroundMode::Statements, "STATEMENTS");
                if ui.add_enabled(self.phase == Phase::Idle, egui::Button::new("EVALUATE / CTRL+ENTER")).clicked() { evaluate = true; }
                if ui.add_enabled(!matches!(self.phase, Phase::Idle | Phase::Stopping | Phase::Closing), egui::Button::new("CANCEL")).clicked() { cancel = true; }
                if ui.button("DRAW DEMO").clicked() {
                    self.mode = PlaygroundMode::Statements;
                    self.input = "clear(25)\nrect_fill(12, 12, 216, 120, 32)\nfor step in 0..16:\n  circle(120, 72, step * 4, 255)\nprint(\"MODL PLAYGROUND\", 74, 68, 7)".to_owned();
                    self.focus_input = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("SEED"); ui.add(egui::DragValue::new(&mut self.seed).range(0..=u32::MAX));
                ui.add_enabled(project.is_some(), egui::Checkbox::new(&mut self.use_context, "USE CURRENT ASSETS / MODULES"));
                if self.use_context && let Some(project) = project { ui.label(format!("CONTEXT: {}", project.manifest.id)); }
            });
            ui.colored_label(if self.failed { ui_theme::CORAL } else { ui_theme::CYAN }, &self.status);
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    ui_theme::legend(ui, "INPUT / USE SPACES FOR INDENTATION");
                    let highlighter = &mut self.highlighter;
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| highlighter.layout(ui, text.as_str(), width);
                    let mut output = egui::TextEdit::multiline(&mut self.input).id(input_id).code_editor().lock_focus(false).desired_rows(10).desired_width(ui.available_width()).char_limit(modl_core::playground::PLAYGROUND_MAX_SOURCE_BYTES).layouter(&mut layouter).show(ui);
                    if self.focus_input {
                        output.response.request_focus();
                        output.state.cursor.set_char_range(Some(CCursorRange::select_all_str(&self.input)));
                        output.state.store(context, output.response.id);
                        self.focus_input = false;
                    }
                    ui.collapsing("DECLARATIONS / IMPORTS (OPTIONAL)", |ui| {
                        ui.add(egui::TextEdit::multiline(&mut self.definitions).id(definitions_id).code_editor().lock_focus(false).desired_rows(6).desired_width(ui.available_width()).char_limit(modl_core::playground::PLAYGROUND_MAX_SOURCE_BYTES));
                    });
                    if ui.add_enabled(!self.source.is_empty(), egui::Button::new("INSPECT WRAPPED MODL SOURCE")).clicked() { self.show_source = true; }
                    egui::ComboBox::from_id_salt("scratch-history").selected_text(format!("HISTORY / {} ENTRIES", self.history.len())).show_ui(ui, |ui| {
                        for (index, entry) in self.history.iter().enumerate().rev() {
                            let first_line = entry.input.lines().next().unwrap_or("").chars().take(60).collect::<String>();
                            if ui.selectable_label(false, format!("{} / {:?} / {first_line}", index + 1, entry.mode)).clicked() { restore = Some(entry.clone()); }
                        }
                    });
                });
                columns[1].vertical(|ui| {
                    ui_theme::legend(ui, "LAST SUCCESSFUL FRAME / 240 x 144 / SILENT PREVIEW");
                    if let Some(texture) = &self.texture {
                        let scale = (ui.available_width() / 240.0).floor().clamp(1.0, 3.0);
                        ui.image((texture.id(), egui::vec2(240.0 * scale, 144.0 * scale)));
                    } else { ui.label("Evaluate an expression or drawing statements."); }
                    ui_theme::legend(ui, "RESULT / VALUES AT THE FINAL SOURCE BOUNDARY");
                    egui::ScrollArea::both().id_salt("scratch-result").max_height(180.0).show(ui, |ui| { ui.label(RichText::new(&self.output).monospace()); });
                });
            });
        });
        if let Some(entry) = restore {
            self.restore_history(entry);
        }
        if evaluate {
            self.evaluate(project);
        }
        if cancel {
            self.cancel();
        }
        if self.show_source {
            egui::Window::new("SCRATCHPAD / EXACT COMPILER INPUT").open(&mut self.show_source).default_size([760.0, 500.0]).vscroll(true).show(context, |ui| {
                ui.label("This temporary module is compiled as ordinary MODL. The active cartridge is not overwritten.");
                if ui.button("COPY SOURCE").clicked() { context.copy_text(self.source.clone()); }
                let mut source = self.source.as_str();
                ui.add(egui::TextEdit::multiline(&mut source).code_editor().desired_rows(20).desired_width(ui.available_width()));
                });
        }
        if self.phase != Phase::Idle {
            context.request_repaint_after(Duration::from_millis(8));
        }
        window.map(|window| window.response.rect)
    }

    fn restore_history(&mut self, entry: HistoryEntry) {
        self.mode = entry.mode;
        self.input = entry.input;
        self.definitions = entry.definitions;
        self.seed = entry.seed;
        self.use_context = entry.use_context;
        self.output = entry.output;
        self.failed = entry.failed;
        self.source = playground_source(self.mode, &self.input, &self.definitions)
            .map_or_else(|_| String::new(), |wrapped| wrapped.source);
        self.focus_input = true;
        self.status = "HISTORY RESTORED / EVALUATE TO RECREATE ITS FRAME".to_owned();
    }
}

fn prepare(
    mode: PlaygroundMode,
    input: &str,
    definitions: &str,
    seed: u32,
    context: Option<&NativeProject>,
) -> Result<Prepared, String> {
    let wrapped = playground_source(mode, input, definitions).map_err(|error| error.to_string())?;
    let (mut manifest, mut files) = if let Some(project) = context {
        let mut cloned = project.clone();
        cloned.sync_editor();
        (cloned.manifest, cloned.files)
    } else {
        (
            ProjectManifest {
                format_revision: modl_core::CARTRIDGE_FORMAT_REVISION,
                language_revision: modl_core::LANGUAGE_REVISION.to_owned(),
                id: "local.scratchpad".to_owned(),
                title: "MODL PLAYGROUND".to_owned(),
                author: "@local".to_owned(),
                version: "1.0".to_owned(),
                entry: ENTRY.to_owned(),
                update_rate: 60,
                hardware_revision: 2,
                compile_on_load: true,
                label: None,
                thumbnail: None,
                display: None,
                assets: BTreeMap::new(),
            },
            BTreeMap::new(),
        )
    };
    manifest.id = "local.scratchpad".to_owned();
    manifest.title = "MODL PLAYGROUND".to_owned();
    manifest.entry = ENTRY.to_owned();
    manifest.update_rate = 60;
    manifest.compile_on_load = true;
    files.insert(ENTRY.to_owned(), wrapped.source.as_bytes().to_vec());
    let source = modl_core::encode_workcart(&manifest, &files, &[], &[])
        .map_err(|error| error.to_string())?;
    let project = NativeProject::from_source(source, None)?;
    let mut command = project.load_command(true, &[])?;
    command["seed"] = Value::from(seed);
    Ok(Prepared {
        command,
        source: wrapped.source,
        result_name: wrapped.result_name.map(str::to_owned),
    })
}

fn preview_image(event: &Value) -> Result<ColorImage, String> {
    let pixels = STANDARD
        .decode(
            event["pixels"]
                .as_str()
                .ok_or("scratch frame has no pixels")?,
        )
        .map_err(|error| error.to_string())?;
    if pixels.len() != 240 * 144 {
        return Err("scratch frame has the wrong dimensions".to_owned());
    }
    let palette = sprite::palette();
    Ok(ColorImage::new(
        [240, 144],
        pixels
            .iter()
            .map(|index| {
                let [r, g, b] = palette[usize::from(*index)];
                Color32::from_rgb(r, g, b)
            })
            .collect(),
    ))
}

fn pretty_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "unrepresentable value".to_owned())
}
fn bounded_output(output: &str) -> String {
    let mut text = output.chars().take(OUTPUT_LIMIT).collect::<String>();
    if output.chars().count() > OUTPUT_LIMIT {
        text.push_str("\n[DISPLAY TRUNCATED]");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    fn evaluate_native(prepared: Prepared) -> Value {
        let runtime = RuntimeBridge::new(native_host_path());
        let receive = || {
            let deadline = Instant::now() + Duration::from_secs(7);
            loop {
                if let Some(event) = runtime.poll() {
                    return event;
                }
                assert!(Instant::now() < deadline, "scratch host timed out");
                thread::sleep(Duration::from_millis(1));
            }
        };
        runtime.send(prepared.command).unwrap();
        let loaded = receive();
        assert_eq!(loaded["type"], "loaded", "{loaded}");
        runtime
            .send(json!({"type":"scratch", "input":replay::empty_input()}))
            .unwrap();
        let result = receive();
        runtime.send(json!({"type":"stop"})).unwrap();
        assert_eq!(receive()["type"], "stopped");
        result
    }

    #[test]
    fn real_modl_expressions_values_drawing_and_runtime_faults_use_the_native_bridge() {
        for (input, expected) in [
            ("sqrt(81.0) + 1.0", json!(10)),
            ("format_int(255, 16, 4)", json!("00ff")),
            ("text_char(128029)", json!({"__some":true, "value":"🐝"})),
        ] {
            let result = evaluate_native(
                prepare(
                    PlaygroundMode::Expression,
                    input,
                    "",
                    modl_core::DEFAULT_SEED,
                    None,
                )
                .unwrap(),
            );
            assert_eq!(result["type"], "scratch", "{result}");
            assert_eq!(result["locals"]["scratch_result"], expected);
        }
        let result = evaluate_native(prepare(PlaygroundMode::Statements, "clear(32)\npixel(twice(3), 2, 255)", "state score: Int = 0\nfn twice(value: Int) -> Int:\n  return value * 2\non update:\n  score = 99\n", 1, None).unwrap());
        assert_eq!(result["type"], "scratch", "{result}");
        let pixels = STANDARD.decode(result["pixels"].as_str().unwrap()).unwrap();
        assert_eq!(pixels[2 * 240 + 6], 255);
        assert_eq!(pixels[0], 32);
        assert_eq!(result["state"]["score"], 99);
        assert!(result.get("saveCommit").is_none());
        let invalid = prepare(PlaygroundMode::Expression, "sqrt(true)", "", 1, None);
        assert!(invalid.is_err());
        let fault =
            evaluate_native(prepare(PlaygroundMode::Expression, "1 / 0", "", 1, None).unwrap());
        assert_eq!(fault["code"], "M019003");
        let runaway = evaluate_native(
            prepare(
                PlaygroundMode::Statements,
                "while true:\n  continue",
                "",
                1,
                None,
            )
            .unwrap(),
        );
        assert_eq!(runaway["code"], "M019001", "{runaway}");
    }

    #[test]
    fn optional_asset_context_is_a_copy_and_cannot_change_project_or_saves() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/api-tour/cart.m01w");
        let mut context = NativeProject::open(&path).unwrap();
        context
            .editor_text
            .push_str("\n// unsaved draft remains here\n");
        let before = (
            context.manifest_source.clone(),
            context.files.clone(),
            context.editor_text.clone(),
            context.workcart_path.clone(),
            context.dirty,
        );
        let prepared = prepare(
            PlaygroundMode::Statements,
            "sprite(#dot, 0, 0)",
            "",
            2,
            Some(&context),
        )
        .unwrap();
        assert_eq!(
            prepared.command["game"]["members"]["main"]["manifest"]["id"],
            "local.scratchpad"
        );
        assert_eq!(prepared.command["saves"]["local.scratchpad"], "");
        let result = evaluate_native(prepared);
        assert_eq!(result["type"], "scratch", "{result}");
        assert_eq!(
            (
                context.manifest_source,
                context.files,
                context.editor_text,
                context.workcart_path,
                context.dirty
            ),
            before
        );
    }

    #[test]
    fn fresh_evaluations_are_deterministic_and_history_output_is_bounded() {
        let source = "pixel(rng_int(0, 240), rng_int(0, 144), 255)";
        let first =
            evaluate_native(prepare(PlaygroundMode::Statements, source, "", 123, None).unwrap());
        let second =
            evaluate_native(prepare(PlaygroundMode::Statements, source, "", 123, None).unwrap());
        assert_eq!(first, second);
        assert!(bounded_output(&"🐝".repeat(OUTPUT_LIMIT + 1)).ends_with("[DISPLAY TRUNCATED]"));
    }

    #[test]
    fn history_restores_its_source_and_fault_status_without_running_a_machine() {
        let mut playground = Playground::new();
        playground.restore_history(HistoryEntry {
            mode: PlaygroundMode::Expression,
            input: "1 / 0".to_owned(),
            definitions: String::new(),
            seed: 7,
            use_context: false,
            output: "M019003: integer division by zero".to_owned(),
            failed: true,
        });
        assert!(playground.failed);
        assert!(playground.source.contains("scratch_result = (1 / 0)"));
        assert_eq!(playground.seed, 7);
        assert!(playground.phase == Phase::Idle);
        assert!(playground.texture.is_none());
        assert!(playground.history.is_empty());
    }

    #[test]
    fn scratchpad_reflows_after_the_native_window_shrinks_with_the_real_font() {
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        ui_theme::install(&context);
        let mut playground = Playground::new();
        playground.status = "HISTORY RESTORED / EVALUATE TO RECREATE ITS FRAME".to_owned();
        let mut open = true;
        for size in [egui::vec2(1280.0, 800.0), egui::vec2(900.0, 600.0)] {
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut rect = None;
            for _ in 0..6 {
                context
                    .run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| {
                            rect = playground.ui(ui.ctx(), &mut open, None);
                        },
                    )
                    .drop_without_applying_deltas();
            }
            let rect = rect.unwrap();
            assert!(
                rect.width() <= size.x + 1.0 && rect.left() >= -1.0,
                "{size:?}: {rect:?}"
            );
            assert!(
                rect.height() <= size.y + 1.0 && rect.top() >= -1.0,
                "{size:?}: {rect:?}"
            );
        }
    }

    #[test]
    fn cancelling_compilation_cannot_execute_later_and_a_fresh_evaluation_still_completes() {
        let context = egui::Context::default();
        let mut playground = Playground::new();
        playground.evaluate(None);
        assert!(playground.phase == Phase::Compiling);
        playground.cancel();
        let wait = |playground: &mut Playground| {
            let deadline = Instant::now() + Duration::from_secs(8);
            while playground.phase != Phase::Idle {
                playground.poll(&context);
                assert!(Instant::now() < deadline, "{}", playground.status);
                thread::sleep(Duration::from_millis(1));
            }
        };
        wait(&mut playground);
        assert!(playground.history.is_empty());
        assert!(playground.texture.is_none());
        assert!(playground.status.starts_with("CANCELLED"));
        playground.evaluate(None);
        wait(&mut playground);
        assert!(!playground.failed, "{}", playground.output);
        assert_eq!(playground.history.len(), 1);
        assert!(playground.texture.is_some());
        assert!(playground.output.starts_with("10\n"));
        assert!(playground.status.contains("DISCARDED"));
    }
}
