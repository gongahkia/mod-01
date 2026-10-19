//! Offline cartridge browser. One bounded background request keeps parsing off the UI thread.
use crate::{gamepad_controller::Navigation, library_preview, library_store, ui_theme};
use eframe::egui::{self, RichText, TextureHandle, TextureOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub path: PathBuf,
    pub bundled: bool,
    pub play: bool,
}

pub enum Action {
    Open(OpenRequest),
    Close,
}

pub(super) enum ConfirmOpen {
    Save,
    Discard,
    Cancel,
}

struct Card {
    path: PathBuf,
    bundled: bool,
    preview: Option<Result<library_preview::Preview, String>>,
}

enum Request {
    Read {
        path: PathBuf,
        art: bool,
        generation: u64,
    },
    Scan(PathBuf),
}
enum Reply {
    Read {
        path: PathBuf,
        art: bool,
        generation: u64,
        result: Result<library_preview::Preview, String>,
    },
    Scan(Result<Scan, String>),
}

#[derive(Default)]
struct Scan {
    paths: Vec<PathBuf>,
    skipped: usize,
    limited: bool,
}

pub struct Library {
    store: Option<library_store::LibraryStore>,
    bundled: Vec<PathBuf>,
    cards: Vec<Card>,
    pub(crate) query: String,
    favourites_only: bool,
    recent_first: bool,
    selected: Option<PathBuf>,
    labels: String,
    labels_dirty: bool,
    scroll_selected: bool,
    cover_path: Option<PathBuf>,
    cover: Option<TextureHandle>,
    status: String,
    generation: u64,
    requests: SyncSender<Request>,
    replies: Receiver<Reply>,
    busy: bool,
    scan_pending: Option<PathBuf>,
    navigation_armed: bool,
}

impl Library {
    pub fn new(path: Option<PathBuf>, bundled: Vec<PathBuf>) -> Self {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::channel();
        thread::Builder::new()
            .name("mod01-library".to_owned())
            .spawn(move || {
                while let Ok(request) = incoming.recv() {
                    let reply = match request {
                        Request::Read {
                            path,
                            art,
                            generation,
                        } => {
                            let result = library_preview::guarded_load(&path, art);
                            Reply::Read {
                                path,
                                art,
                                generation,
                                result,
                            }
                        }
                        Request::Scan(path) => Reply::Scan(scan_folder(&path)),
                    };
                    if outgoing.send(reply).is_err() {
                        break;
                    }
                }
            })
            .expect("start cartridge library worker");
        let mut library = Self {
            store: path.map(library_store::LibraryStore::open),
            bundled,
            cards: Vec::new(),
            query: String::new(),
            favourites_only: false,
            recent_first: false,
            selected: None,
            labels: String::new(),
            labels_dirty: false,
            scroll_selected: false,
            cover_path: None,
            cover: None,
            status: String::new(),
            generation: 0,
            requests,
            replies,
            busy: false,
            scan_pending: None,
            navigation_armed: false,
        };
        library.reconcile();
        library
    }

    pub fn opened(&mut self, path: &Path) -> Result<(), String> {
        if let Some(store) = &mut self.store {
            store.launched(path)?;
            self.reconcile();
        }
        Ok(())
    }

    pub fn enter(&mut self) {
        self.navigation_armed = false;
    }

    fn reconcile(&mut self) {
        let mut origins = BTreeMap::new();
        for path in &self.bundled {
            if let Ok(path) = library_store::cartridge_path(path) {
                origins.insert(path, true);
            }
        }
        if let Some(store) = &self.store {
            for entry in &store.collection().entries {
                origins.entry(entry.path.clone()).or_insert(false);
            }
        }
        let mut previous: BTreeMap<_, _> = std::mem::take(&mut self.cards)
            .into_iter()
            .map(|card| (card.path.clone(), card))
            .collect();
        self.cards = origins
            .into_iter()
            .map(|(path, bundled)| {
                let mut card = previous.remove(&path).unwrap_or_else(|| Card {
                    path: path.clone(),
                    bundled,
                    preview: None,
                });
                card.bundled = bundled;
                card
            })
            .collect();
        let visible = self.visible();
        self.ensure_selection(&visible);
    }

    fn entry(&self, path: &Path) -> Option<&library_store::Entry> {
        self.store
            .as_ref()?
            .collection()
            .entries
            .iter()
            .find(|entry| entry.path == path)
    }
    fn writable(&self) -> bool {
        self.store
            .as_ref()
            .is_some_and(|store| store.error().is_none())
    }
    fn visible(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        let mut visible: Vec<usize> = self
            .cards
            .iter()
            .enumerate()
            .filter_map(|(index, card)| {
                let entry = self.entry(&card.path);
                if self.favourites_only && !entry.is_some_and(|entry| entry.favourite) {
                    return None;
                }
                let mut text = card.path.to_string_lossy().to_lowercase();
                if let Some(Ok(preview)) = &card.preview {
                    text.push_str(
                        &format!(" {} {} {}", preview.title, preview.id, preview.author)
                            .to_lowercase(),
                    );
                }
                if let Some(entry) = entry {
                    text.push_str(&entry.labels.join(" ").to_lowercase());
                }
                terms
                    .iter()
                    .all(|term| text.contains(term))
                    .then_some(index)
            })
            .collect();
        visible.sort_by(|left, right| {
            let a = &self.cards[*left];
            let b = &self.cards[*right];
            let title = |card: &Card| {
                card.preview
                    .as_ref()
                    .and_then(|value| value.as_ref().ok())
                    .map_or_else(
                        || card.path.to_string_lossy().to_lowercase(),
                        |value| value.title.to_lowercase(),
                    )
            };
            let recent = |card: &Card| self.entry(&card.path).map_or(0, |entry| entry.last_used);
            let order = if self.recent_first {
                recent(b).cmp(&recent(a))
            } else {
                std::cmp::Ordering::Equal
            };
            order
                .then_with(|| title(a).cmp(&title(b)))
                .then_with(|| a.path.cmp(&b.path))
        });
        visible
    }

    fn select(&mut self, path: Option<PathBuf>) {
        if self.selected == path {
            return;
        }
        self.selected = path;
        self.labels = self
            .selected
            .as_ref()
            .and_then(|path| self.entry(path))
            .map_or_else(String::new, |entry| entry.labels.join(", "));
        self.labels_dirty = false;
        self.cover = None;
        self.cover_path = None;
        self.scroll_selected = true;
    }
    fn ensure_selection(&mut self, visible: &[usize]) {
        if !visible
            .iter()
            .any(|index| self.selected.as_ref() == Some(&self.cards[*index].path))
        {
            self.select(visible.first().map(|index| self.cards[*index].path.clone()));
        }
    }
    fn selected_card(&self) -> Option<&Card> {
        self.cards
            .iter()
            .find(|card| Some(&card.path) == self.selected.as_ref())
    }

    fn refresh(&mut self) {
        if let Some(store) = &mut self.store {
            self.status = store
                .reload()
                .err()
                .unwrap_or_else(|| "LIBRARY REFRESHED".to_owned());
        }
        self.generation = self.generation.wrapping_add(1);
        for card in &mut self.cards {
            card.preview = None;
        }
        self.cover_path = None;
        self.cover = None;
        self.reconcile();
        self.labels = self
            .selected
            .as_ref()
            .and_then(|path| self.entry(path))
            .map_or_else(String::new, |entry| entry.labels.join(", "));
        self.labels_dirty = false;
    }

    fn register(&mut self, paths: &[PathBuf]) {
        let Some(store) = &mut self.store else {
            return;
        };
        match store.register(paths) {
            Ok(count) => {
                self.status = format!("ADDED {count} CARTRIDGES / SOURCE FILES UNCHANGED");
                self.reconcile();
            }
            Err(error) => self.status = error,
        }
    }

    fn preferences(&mut self, toggle: bool) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let favourite = self.entry(&path).is_some_and(|entry| entry.favourite) ^ toggle;
        let labels: Vec<_> = if self.labels.trim().is_empty() {
            Vec::new()
        } else {
            self.labels.split(',').map(str::to_owned).collect()
        };
        let Some(store) = &mut self.store else {
            return;
        };
        match store.preferences(&path, favourite, &labels) {
            Ok(()) => {
                self.labels = self.entry(&path).unwrap().labels.join(", ");
                self.labels_dirty = false;
                self.status = "LIBRARY PREFERENCES SAVED / CARTRIDGE UNCHANGED".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn poll(&mut self, context: &egui::Context) {
        while let Ok(reply) = self.replies.try_recv() {
            self.busy = false;
            match reply {
                Reply::Read {
                    path,
                    art,
                    generation,
                    mut result,
                } if generation == self.generation => {
                    if art && self.selected.as_ref() == Some(&path) {
                        self.cover_path = Some(path.clone());
                        self.cover = result
                            .as_mut()
                            .ok()
                            .and_then(|preview| preview.image.take())
                            .map(|image| {
                                context.load_texture(
                                    "library-cover",
                                    image,
                                    TextureOptions::NEAREST,
                                )
                            });
                    }
                    // Never retain RGBA images for the whole library: only one selected GPU cover.
                    if let Ok(preview) = &mut result {
                        preview.image = None;
                    }
                    if let Some(card) = self.cards.iter_mut().find(|card| card.path == path) {
                        card.preview = Some(result);
                    }
                }
                Reply::Read { .. } => {}
                Reply::Scan(result) => match result {
                    Ok(scan) => {
                        self.register(&scan.paths);
                        let _ = write!(
                            self.status,
                            " / {} SKIPPED{}",
                            scan.skipped,
                            if scan.limited {
                                " / SCAN LIMIT REACHED"
                            } else {
                                ""
                            }
                        );
                    }
                    Err(error) => self.status = error,
                },
            }
        }
        if !self.busy {
            let request = if let Some(path) = self.scan_pending.take() {
                Some(Request::Scan(path))
            } else {
                self.selected_card()
                    .filter(|card| {
                        card.preview.is_none() || self.cover_path.as_ref() != Some(&card.path)
                    })
                    .map(|card| Request::Read {
                        path: card.path.clone(),
                        art: true,
                        generation: self.generation,
                    })
                    .or_else(|| {
                        self.cards
                            .iter()
                            .find(|card| card.preview.is_none())
                            .map(|card| Request::Read {
                                path: card.path.clone(),
                                art: false,
                                generation: self.generation,
                            })
                    })
            };
            if let Some(request) = request {
                match self.requests.try_send(request) {
                    Ok(()) => self.busy = true,
                    Err(error) => self.status = format!("LIBRARY WORKER: {error}"),
                }
            }
        }
        if self.busy {
            context.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn navigate(&mut self, navigation: Navigation, visible: &[usize]) -> Option<Action> {
        if !self.navigation_armed {
            self.navigation_armed = true;
            return None;
        }
        if visible.is_empty() {
            return navigation.cancel.then_some(Action::Close);
        }
        let current = visible
            .iter()
            .position(|index| self.selected.as_ref() == Some(&self.cards[*index].path))
            .unwrap_or(0);
        let next = if navigation.down {
            (current + 1).min(visible.len() - 1)
        } else if navigation.up {
            current.saturating_sub(1)
        } else {
            current
        };
        self.select(Some(self.cards[visible[next]].path.clone()));
        if navigation.favourite {
            self.preferences(true);
        }
        if navigation.cancel {
            return Some(Action::Close);
        }
        if navigation.confirm || navigation.inspect {
            return self.open_action(!navigation.inspect);
        }
        None
    }
    fn open_action(&self, play: bool) -> Option<Action> {
        let card = self.selected_card()?;
        if !matches!(&card.preview, Some(Ok(_))) {
            return None;
        }
        Some(Action::Open(OpenRequest {
            path: card.path.clone(),
            bundled: card.bundled,
            play,
        }))
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        mut navigation: Navigation,
        can_close: bool,
    ) -> Option<Action> {
        let context = ui.ctx().clone();
        self.poll(&context);
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("CARTRIDGE LIBRARY").color(ui_theme::CREAM));
            ui_theme::legend(ui, "OFFLINE / SOURCE VISIBLE");
            if can_close && ui.button("BACK TO WORKSPACE").clicked() {
                action = Some(Action::Close);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.writable(), egui::Button::new("ADD CARTRIDGES"))
                .clicked()
            {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("MOD-01", &["m01w", "m01c", "png"])
                    .pick_files()
                {
                    self.register(&paths);
                }
            }
            if ui
                .add_enabled(
                    self.writable() && self.scan_pending.is_none(),
                    egui::Button::new("ADD FOLDER"),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.scan_pending = Some(path);
                }
            }
            if ui.button("REFRESH / RELOAD").clicked() {
                self.refresh();
            }
            ui.label(format!(
                "{} CARTS{}",
                self.cards.len(),
                if self.busy { " / READING" } else { "" }
            ));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("SEARCH");
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(260.0)
                    .hint_text("title, author, path, labels"),
            );
            if ui.button("CLEAR").clicked() {
                self.query.clear();
            }
            ui.checkbox(&mut self.favourites_only, "FAVOURITES");
            ui.checkbox(&mut self.recent_first, "RECENT FIRST");
        });
        ui.label(
            RichText::new(
                "ARROWS: SELECT  Z/ENTER/A: PLAY  X/PAD X: SOURCE  Y: FAVOURITE  ESC/B: BACK",
            )
            .small()
            .color(ui_theme::CYAN),
        );
        if let Some(store) = &self.store {
            if let Some(error) = store.error() {
                ui.colored_label(ui_theme::CORAL, format!("PREFERENCES READ-ONLY: {error}"));
            }
            ui.label(
                RichText::new(format!("PREFERENCES: {}", store.path().display()))
                    .small()
                    .color(ui_theme::MUTED),
            );
        } else {
            ui.colored_label(
                ui_theme::AMBER,
                "PREFERENCES UNAVAILABLE: platform data directory not available",
            );
        }
        if !self.status.is_empty() {
            ui.label(RichText::new(&self.status).small().color(ui_theme::AMBER));
        }
        let visible = self.visible();
        self.ensure_selection(&visible);
        if ui.is_enabled() && !context.text_edit_focused() && context.input(|input| input.focused) {
            context.input_mut(|input| {
                navigation.up |= input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                navigation.down |= input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                navigation.confirm |= input.consume_key(egui::Modifiers::NONE, egui::Key::Z)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                navigation.inspect |= input.consume_key(egui::Modifiers::NONE, egui::Key::X);
                navigation.favourite |= input.consume_key(egui::Modifiers::NONE, egui::Key::Y);
                navigation.cancel |= input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
            action = self.navigate(navigation, &visible).or(action);
        } else {
            self.navigation_armed = true;
        }
        ui.separator();
        ui.columns(2, |columns| {
            let left = &mut columns[0];
            egui::ScrollArea::vertical().id_salt("library-list").auto_shrink([false, false]).show(left, |ui| {
                if visible.is_empty() { ui.label("NO MATCHING CARTRIDGES"); }
                for index in visible {
                    let card = &self.cards[index];
                    let title = match &card.preview {
                        Some(Ok(preview)) => preview.title.clone(),
                        Some(Err(_)) => format!("UNAVAILABLE: {}", card.path.file_name().unwrap_or_default().to_string_lossy()),
                        None => card.path.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy().into_owned(),
                    };
                    let favourite = self.entry(&card.path).is_some_and(|entry| entry.favourite);
                    let label = format!("{}{}\n{}", if favourite { "* " } else { "" }, title,
                        if card.bundled { "BUNDLED / COPY ON OPEN" } else { "LOCAL CARTRIDGE" });
                    let response = ui.add_sized([ui.available_width(), 64.0], egui::Button::new(label)
                        .wrap().selected(self.selected.as_ref() == Some(&card.path)));
                    if self.scroll_selected && self.selected.as_ref() == Some(&card.path) {
                        response.scroll_to_me(Some(egui::Align::Center));
                        self.scroll_selected = false;
                    }
                    if response.clicked() { self.select(Some(self.cards[index].path.clone())); }
                }
            });
            egui::ScrollArea::vertical().id_salt("library-details").auto_shrink([false, false]).show(&mut columns[1], |ui| {
                let Some(card) = self.selected_card() else { return; };
                let path = card.path.clone();
                let bundled = card.bundled;
                let ready = matches!(&card.preview, Some(Ok(_)));
                if let Some(Ok(preview)) = &card.preview {
                    ui.heading(&preview.title);
                    ui.label(format!("{} / {}", preview.author, preview.id));
                    if let Some(warning) = &preview.warning { ui.colored_label(ui_theme::AMBER, warning); }
                } else if let Some(Err(error)) = &card.preview {
                    ui.colored_label(ui_theme::CORAL, format!("CARTRIDGE UNAVAILABLE: {error}"));
                } else { ui.label("READING CARTRIDGE..."); }
                if let Some(cover) = &self.cover {
                    let width = ui.available_width().min(360.0);
                    let ratio = cover.size()[1] as f32 / cover.size()[0] as f32;
                    ui.image((cover.id(), egui::vec2(width, width * ratio)));
                } else { ui.label(RichText::new("MOD-01 / CARTRIDGE").color(ui_theme::AMBER)); }
                ui.label(RichText::new(path.to_string_lossy()).small().color(ui_theme::MUTED));
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(ready, egui::Button::new("PLAY")).clicked() { action = self.open_action(true); }
                    if ui.add_enabled(ready, egui::Button::new("INSPECT SOURCE")).clicked() { action = self.open_action(false); }
                });
                let favourite = self.entry(&path).is_some_and(|entry| entry.favourite);
                if ui.add_enabled(self.writable(), egui::Button::new(if favourite { "* FAVOURITE" } else { "+ FAVOURITE" })).clicked() {
                    self.preferences(true);
                }
                ui.label("LABELS (COMMA SEPARATED)");
                self.labels_dirty |= ui.add_enabled(self.writable(), egui::TextEdit::singleline(&mut self.labels).char_limit(300)).changed();
                if ui.add_enabled(self.writable() && self.labels_dirty, egui::Button::new("SAVE LABELS")).clicked() { self.preferences(false); }
                if !bundled && ui.add_enabled(self.writable(), egui::Button::new("FORGET BOOKMARK")).on_hover_text("Removes library preferences only. Never deletes the cartridge or save data.").clicked() {
                    if let Some(store) = &mut self.store {
                        match store.forget(&path) {
                            Ok(()) => { self.status = "BOOKMARK FORGOTTEN / CARTRIDGE AND SAVES UNCHANGED".to_owned(); self.reconcile(); }
                            Err(error) => self.status = error,
                        }
                    }
                }
            });
        });
        context.request_repaint_after(Duration::from_millis(16));
        action
    }
}

fn scan_folder(root: &Path) -> Result<Scan, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("choose a cartridge folder".to_owned());
    }
    let mut scan = Scan::default();
    let mut pending = vec![(root, 0)];
    let mut visited = 0;
    let mut seen = BTreeSet::new();
    while let Some((path, depth)) = pending.pop() {
        if visited >= 4096 || scan.paths.len() >= 1024 {
            scan.limited = true;
            break;
        }
        visited += 1;
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            scan.skipped += 1;
            continue;
        };
        if metadata.file_type().is_symlink() {
            scan.skipped += 1;
            continue;
        }
        if metadata.is_dir() {
            if path.join("cart.m01w").is_file() {
                let cart = path.join("cart.m01w");
                if !fs::symlink_metadata(&cart)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    if seen.insert(cart.clone()) {
                        scan.paths.push(cart);
                    }
                    continue;
                }
            }
            if depth == 8 {
                scan.limited = true;
                continue;
            }
            match fs::read_dir(&path) {
                Ok(entries) => {
                    for entry in entries.take(4097) {
                        if pending.len() + visited >= 4096 {
                            scan.limited = true;
                            break;
                        }
                        match entry {
                            Ok(entry) => pending.push((entry.path(), depth + 1)),
                            Err(_) => scan.skipped += 1,
                        }
                    }
                }
                Err(_) => scan.skipped += 1,
            }
        } else if metadata.is_file()
            && library_store::supported_path(&path)
            && seen.insert(path.clone())
        {
            scan.paths.push(path);
        }
    }
    scan.paths.sort();
    Ok(scan)
}

impl crate::StudioApp {
    pub(super) fn enter_library(&mut self) {
        if self.show_library {
            return;
        }
        self.library_was_paused = self.player_paused;
        if self.running {
            self.set_player_paused(true);
        }
        self.live_audio.stop();
        self.show_library = true;
        self.library.enter();
    }

    pub(super) fn library_action(&mut self, action: Action) {
        match action {
            Action::Open(request) => self.request_library_open(request),
            Action::Close if self.project.is_some() => {
                self.show_library = false;
                if self.running && !self.library_was_paused {
                    self.set_player_paused(false);
                }
            }
            Action::Close => {}
        }
    }

    fn unsaved_project(&self) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| project.dirty || self.manifest_text != project.manifest_source)
            || self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            || self.asset_dirty
            || self
                .asset_workbench
                .as_ref()
                .is_some_and(|workbench| workbench.dirty)
    }

    pub(super) fn request_library_open(&mut self, request: OpenRequest) {
        if self.unsaved_project() {
            self.enter_library();
            self.pending_library_open = Some(request);
        } else {
            self.replace_from_library(request);
        }
    }

    fn replace_from_library(&mut self, request: OpenRequest) {
        // Validate/read before stopping the current session. A missing bookmark cannot discard work.
        match crate::project::NativeProject::open(&request.path) {
            Ok(mut project) => {
                if request.bundled {
                    project.root = None;
                    project.workcart_path = None;
                }
                self.stop();
                self.player_mode = false;
                self.show_library = false;
                self.pending_library_open = None;
                self.manifest_text = project.manifest_source.clone();
                self.status = format!("OPENED {}", project.manifest.title);
                self.project = Some(project);
                self.reset_asset_editors();
                self.view = crate::View::Code;
                let preferences = self.library.opened(&request.path);
                if request.play {
                    self.run();
                }
                if let Err(error) = preferences {
                    let _ = write!(self.status, " / LIBRARY: {error}");
                }
            }
            Err(error) => self.status = format!("OPEN FAILED: {error}"),
        }
    }

    pub(super) fn library_confirmation(&mut self, context: &egui::Context) {
        let Some(request) = self.pending_library_open.clone() else {
            return;
        };
        let mut choice = None;
        let response = egui::Modal::new(egui::Id::unique("library-unsaved-confirmation")).show(context, |ui| {
            ui.set_width(480.0_f32.min(context.content_rect().width() - 60.0));
            ui.heading("KEEP UNSAVED CARTRIDGE CHANGES?");
            ui.label("Opening another cartridge replaces the current workspace. Library preferences never save cartridge edits.");
            ui.label(request.path.to_string_lossy());
            ui.label(&self.status);
            ui.horizontal_wrapped(|ui| {
                if ui.button("SAVE & OPEN").clicked() { choice = Some(ConfirmOpen::Save); }
                if ui.button("DISCARD & OPEN").clicked() { choice = Some(ConfirmOpen::Discard); }
                if ui.button("CANCEL").clicked() { choice = Some(ConfirmOpen::Cancel); }
            });
        });
        if response.should_close() {
            choice = Some(ConfirmOpen::Cancel);
        }
        if let Some(choice) = choice {
            self.confirm_library_open(choice);
        }
    }

    pub(super) fn confirm_library_open(&mut self, choice: ConfirmOpen) {
        let Some(request) = self.pending_library_open.clone() else {
            return;
        };
        match choice {
            ConfirmOpen::Save => {
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| self.manifest_text != project.manifest_source)
                {
                    self.apply_manifest();
                }
                // Invalid/unapplied manifest text must not be silently discarded by ordinary SAVE.
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| self.manifest_text != project.manifest_source)
                {
                    return;
                }
                self.save();
                if !self.unsaved_project() {
                    self.replace_from_library(request);
                }
            }
            ConfirmOpen::Discard => self.replace_from_library(request),
            ConfirmOpen::Cancel => self.pending_library_open = None,
        }
    }
}
