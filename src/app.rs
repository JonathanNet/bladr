//! The viewer UI: one canvas, a fading heads-up display, and a decoder worker.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Key, Margin, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};

use crate::decode::{DecodeEvent, Decoder};
use crate::exif::{self, ExifData};
use crate::focus::{self, FocusData};
use crate::i18n::{self, Lang};
use crate::library::{self, Photo};
use crate::settings::{self, Settings};
use crate::update::{self, UpdateInfo};

const HUD_IDLE_SECS: f64 = 2.6;
const HINT_SECS: f64 = 9.0;
const SLIDESHOW_SECS: f64 = 3.2;
const MAX_CACHE: usize = 7;
const PREFETCH: isize = 2;
const WHEEL_POINTS_PER_IMAGE: f32 = 50.0;

struct Tex {
    handle: TextureHandle,
    /// Size in texture pixels.
    size: Vec2,
    /// Full-resolution dimensions after EXIF orientation.
    source: [u32; 2],
    /// EXIF orientation code applied to the pixels.
    orientation: u16,
    /// True when the pixels are a RAW file's embedded preview.
    raw: bool,
}

/// A folder scan running on its own thread. Network shares can take a long
/// time to enumerate, so the UI must stay responsive.
struct ScanJob {
    dir: PathBuf,
    focus: Option<PathBuf>,
    rx: Receiver<Result<Vec<Photo>, String>>,
}

#[derive(Default)]
struct Snapshot {
    left: bool,
    right: bool,
    home: bool,
    end: bool,
    space: bool,
    fullscreen: bool,
    fullscreen_f11: bool,
    escape: bool,
    zoom_in: bool,
    zoom_out: bool,
    fit: bool,
    actual: bool,
    info: bool,
    open_folder: bool,
    open_file: bool,
    open_path_prompt: bool,
    about: bool,
    options: bool,
    focus_toggle: bool,
    toggle_language: bool,
    dropped: Vec<PathBuf>,
    hovered_files: bool,
    pointer: Option<Pos2>,
    mouse_moved: bool,
    time: f64,
    focused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Failed,
}

pub struct BladrApp {
    folder: Option<PathBuf>,
    photos: Vec<Photo>,
    index: usize,
    textures: HashMap<usize, Tex>,
    shown: usize,
    has_shown: bool,
    decoder: Decoder,
    failed: HashSet<usize>,

    /// Points per texture pixel; only used while `fit` is false.
    zoom: f32,
    /// Points per texture pixel currently on screen (updated while drawing).
    last_scale: f32,
    fit: bool,
    pan: Vec2,

    fullscreen: bool,
    start_fullscreen: bool,
    maximize_stage: u8,
    hud_pinned: bool,
    hud_alpha: f32,
    cursor_hidden: bool,
    last_activity: f64,
    hint_until: f64,

    slideshow_next: Option<f64>,
    wheel_acc: f32,
    last_wheel: f64,
    error: Option<(String, f64)>,
    title: String,
    lang: Lang,
    hint_scheduled: bool,

    scan: Option<ScanJob>,
    path_prompt: Option<String>,
    path_prompt_error: String,
    path_prompt_focus: bool,
    about_open: bool,
    options_open: bool,
    settings: Settings,
    settings_error: Option<String>,
    update_check: Option<Receiver<Result<Option<UpdateInfo>, String>>>,
    update_download: Option<Receiver<Result<PathBuf, String>>>,
    update_status: UpdateStatus,
    update_available: Option<UpdateInfo>,
    update_downloading: bool,
    update_error: Option<String>,
    incoming: Receiver<PathBuf>,
    exif_visible: bool,
    exif_cache: HashMap<usize, Option<ExifData>>,
    exif_rx: Option<Receiver<(usize, Option<ExifData>)>>,
    focus_visible: bool,
    focus_cache: HashMap<usize, Option<FocusData>>,
    focus_rx: Option<Receiver<(usize, Option<FocusData>)>>,
}

impl BladrApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        start_fullscreen: bool,
        incoming: Receiver<PathBuf>,
    ) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::BLACK;
        visuals.window_fill = Color32::from_rgb(16, 18, 22);
        visuals.override_text_color = Some(Color32::from_rgb(225, 228, 233));
        cc.egui_ctx.set_visuals(visuals);

        let lang = Lang::initial();
        i18n::set_decoder_lang(lang);
        let settings = settings::load();
        let start_fullscreen = start_fullscreen || settings.start_fullscreen;

        let mut app = Self {
            folder: None,
            photos: Vec::new(),
            index: 0,
            textures: HashMap::new(),
            shown: 0,
            has_shown: false,
            decoder: Decoder::spawn(),
            failed: HashSet::new(),
            zoom: 1.0,
            last_scale: 0.0,
            fit: true,
            pan: Vec2::ZERO,
            fullscreen: false,
            start_fullscreen,
            maximize_stage: 0,
            hud_pinned: false,
            hud_alpha: 1.0,
            cursor_hidden: false,
            last_activity: 0.0,
            hint_until: 0.0,
            slideshow_next: None,
            wheel_acc: 0.0,
            last_wheel: 0.0,
            error: None,
            title: "Bladr".to_owned(),
            lang,
            hint_scheduled: false,
            scan: None,
            path_prompt: None,
            path_prompt_error: String::new(),
            path_prompt_focus: false,
            about_open: false,
            options_open: false,
            settings,
            settings_error: None,
            update_check: None,
            update_download: None,
            update_status: UpdateStatus::Idle,
            update_available: None,
            update_downloading: false,
            update_error: None,
            incoming,
            exif_visible: false,
            exif_cache: HashMap::new(),
            exif_rx: None,
            focus_visible: false,
            focus_cache: HashMap::new(),
            focus_rx: None,
        };

        if app.settings.auto_update
            && settings::now_unix().saturating_sub(app.settings.last_check) > 6 * 3600
        {
            app.start_update_check();
        }

        match initial {
            Some(path) => app.open_path(&path),
            None => {
                if let Some(last) = load_last_folder() {
                    app.load_folder(&last, None);
                }
            }
        }

        app
    }

    fn set_language(&mut self, lang: Lang) {
        if self.lang == lang {
            return;
        }
        self.lang = lang;
        i18n::set_decoder_lang(lang);
        i18n::save(lang);
        self.error = None;
    }

    fn start_update_check(&mut self) {
        if self.update_check.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_check = Some(rx);
        self.update_status = UpdateStatus::Checking;
        self.update_error = None;
        self.settings.last_check = settings::now_unix();
        self.persist_settings();
        std::thread::spawn(move || {
            let _ = tx.send(update::check(env!("CARGO_PKG_VERSION")));
        });
    }

    fn start_update_download(&mut self) {
        let Some(info) = self.update_available.clone() else {
            return;
        };
        if self.update_download.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_download = Some(rx);
        self.update_downloading = true;
        self.update_error = None;
        std::thread::spawn(move || {
            let _ = tx.send(update::download(&info));
        });
    }

    fn poll_update(&mut self) {
        let check = self
            .update_check
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(result) = check {
            self.update_check = None;
            match result {
                Ok(Some(info)) => {
                    self.update_status = UpdateStatus::Idle;
                    self.update_available = Some(info);
                }
                Ok(None) => self.update_status = UpdateStatus::UpToDate,
                Err(error) => {
                    self.update_status = UpdateStatus::Failed;
                    self.update_error = Some(error);
                }
            }
        }

        let download = self
            .update_download
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(result) = download {
            self.update_download = None;
            self.update_downloading = false;
            match result {
                Ok(path) => match update::run_installer(&path) {
                    Ok(()) => std::process::exit(0),
                    Err(error) => self.update_error = Some(error),
                },
                Err(error) => self.update_error = Some(error),
            }
        }
    }

    fn poll_incoming(&mut self, ctx: &egui::Context) {
        while let Ok(path) = self.incoming.try_recv() {
            if path.as_os_str().is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                continue;
            }
            self.about_open = false;
            self.options_open = false;
            self.update_available = None;
            self.open_path(&path);
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn request_exif(&mut self) {
        if !self.exif_visible || self.exif_rx.is_some() {
            return;
        }
        if self.exif_cache.contains_key(&self.index) {
            return;
        }
        let Some(photo) = self.photos.get(self.index) else {
            return;
        };
        let index = self.index;
        let path = photo.path.clone();
        let (tx, rx) = mpsc::channel();
        self.exif_rx = Some(rx);
        std::thread::spawn(move || {
            let data = exif::read(&path);
            let _ = tx.send((index, data));
        });
    }

    fn poll_exif(&mut self) {
        let result = self.exif_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((index, data)) = result {
            self.exif_rx = None;
            self.exif_cache.insert(index, data);
            self.request_exif();
        }
    }

    fn persist_settings(&mut self) {
        match settings::save(&self.settings) {
            Ok(()) => self.settings_error = None,
            Err(error) => self.settings_error = Some(error),
        }
    }

    fn request_focus(&mut self) {
        if !self.focus_visible || self.focus_rx.is_some() {
            return;
        }
        if self.focus_cache.contains_key(&self.index) {
            return;
        }
        let Some(photo) = self.photos.get(self.index) else {
            return;
        };
        let index = self.index;
        let path = photo.path.clone();
        let (tx, rx) = mpsc::channel();
        self.focus_rx = Some(rx);
        std::thread::spawn(move || {
            let data = focus::read(&path);
            let _ = tx.send((index, data));
        });
    }

    fn poll_focus(&mut self) {
        let result = self.focus_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((index, data)) = result {
            self.focus_rx = None;
            self.focus_cache.insert(index, data);
            self.request_focus();
        }
    }

    fn open_path(&mut self, path: &Path) {        // A network path such as \\server\share often cannot be probed with
        // `is_dir` before the share is authenticated, so treat anything
        // without a file extension (or with a trailing separator) as a folder.
        let text = path.to_string_lossy();
        let looks_like_folder =
            path.is_dir() || path.extension().is_none() || text.ends_with('\\') || text.ends_with('/');

        if looks_like_folder {
            self.load_folder(path, None);
        } else if let Some(parent) = path.parent() {
            self.load_folder(parent, Some(path));
        }
    }

    fn load_folder(&mut self, dir: &Path, focus: Option<&Path>) {
        let dir = dir.to_path_buf();
        let focus = focus.map(Path::to_path_buf);
        let (tx, rx) = mpsc::channel();
        let scan_dir = dir.clone();
        let lang = self.lang;

        std::thread::spawn(move || {
            let result = library::scan_folder(&scan_dir)
                .map_err(|err| library::describe_scan_error(&scan_dir, &err, lang));
            let _ = tx.send(result);
        });

        self.scan = Some(ScanJob { dir, focus, rx });
        self.last_activity = 0.0;
    }

    fn poll_scan(&mut self, now: f64) {
        let Some(job) = self.scan.as_ref() else {
            return;
        };
        let event = job.rx.try_recv();

        match event {
            Ok(Ok(photos)) => {
                let job = self.scan.take().expect("scan job");
                self.apply_photos(job.dir, photos, job.focus.as_deref());
            }
            Ok(Err(message)) => {
                self.scan = None;
                self.error = Some((message, now + 14.0));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.scan = None;
                self.error = Some((i18n::tr(self.lang, "scan.interrupted").to_owned(), now + 6.0));
            }
        }
    }

    fn apply_photos(&mut self, dir: PathBuf, photos: Vec<Photo>, focus: Option<&Path>) {
        self.folder = Some(dir.clone());
        self.photos = photos;
        self.index = focus
            .and_then(|p| library::index_of(&self.photos, p))
            .unwrap_or(0);
        self.textures.clear();
        self.exif_cache.clear();
        self.exif_rx = None;
        self.focus_cache.clear();
        self.focus_rx = None;
        self.failed.clear();
        self.shown = self.index;
        self.has_shown = false;
        self.fit = true;
        self.pan = Vec2::ZERO;
        self.zoom = 1.0;
        self.last_scale = 0.0;
        self.error = None;
        self.slideshow_next = None;
        self.hint_until = 0.0;
        self.hint_scheduled = false;
        self.last_activity = 0.0;
        save_last_folder(&dir);
    }

    fn open_path_prompt(&mut self) {
        self.path_prompt = Some(String::new());
        self.path_prompt_error.clear();
        self.path_prompt_focus = true;
    }

    fn submit_path_prompt(&mut self) {
        let Some(text) = self.path_prompt.take() else {
            return;
        };
        let trimmed = text.trim().trim_matches('"');
        if trimmed.is_empty() {
            self.path_prompt = Some(text);
            self.path_prompt_error = i18n::tr(self.lang, "path.empty_error").to_owned();
            return;
        }
        self.open_path(Path::new(trimmed));
    }

    fn desired_title(&self) -> String {
        match self.photos.get(self.index) {
            Some(photo) => format!(
                "{} — Bladr ({}/{})",
                photo.name,
                self.index + 1,
                self.photos.len()
            ),
            None => format!("Bladr — {}", i18n::tr(self.lang, "app.subtitle")),
        }
    }

    fn navigate(&mut self, delta: isize, now: f64) {
        if self.photos.len() < 2 {
            return;
        }
        let len = self.photos.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(len) as usize;
        self.fit = true;
        self.pan = Vec2::ZERO;
        self.error = None;
        self.last_activity = now;
        if self.slideshow_next.is_some() {
            self.slideshow_next = Some(now + SLIDESHOW_SECS);
        }
        self.request_exif();
        self.request_focus();
    }

    fn jump_to(&mut self, index: usize, now: f64) {
        if self.photos.is_empty() || index >= self.photos.len() {
            return;
        }
        self.index = index;
        self.fit = true;
        self.pan = Vec2::ZERO;
        self.error = None;
        self.last_activity = now;
        if self.slideshow_next.is_some() {
            self.slideshow_next = Some(now + SLIDESHOW_SECS);
        }
        self.request_exif();
        self.request_focus();
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context, now: f64) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        self.last_activity = now;
        if !self.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
            self.cursor_hidden = false;
        }
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        if self.fullscreen == on {
            return;
        }
        self.fullscreen = on;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
        if !on {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
            self.cursor_hidden = false;
        }
    }

    fn zoom_by(&mut self, factor: f32, anchor: Option<Pos2>, rect: Rect) {
        let old = self.effective_zoom(rect).max(0.001);
        let new = (old * factor).clamp(0.02, 12.0);
        if (new - old).abs() < f32::EPSILON {
            return;
        }
        if let Some(pointer) = anchor {
            let center = rect.center() + self.pan;
            let offset = pointer - center;
            self.pan += offset * (1.0 - new / old);
        }
        self.zoom = new;
        self.fit = false;
    }

    fn effective_zoom(&self, rect: Rect) -> f32 {
        if self.fit {
            if let Some(tex) = self.textures.get(&self.index) {
                return fit_scale(rect, tex);
            }
            if let Some(tex) = self.textures.get(&self.shown) {
                return fit_scale(rect, tex);
            }
        }
        self.zoom
    }

    fn displayed_tex(&self) -> Option<(usize, &Tex)> {
        if let Some(tex) = self.textures.get(&self.index) {
            return Some((self.index, tex));
        }
        if self.has_shown {
            if let Some(tex) = self.textures.get(&self.shown) {
                return Some((self.shown, tex));
            }
        }
        None
    }

    fn poll_decoder(&mut self, ctx: &egui::Context, _now: f64) {
        while let Some(event) = self.decoder.try_recv() {
            match event {
                DecodeEvent::Loaded(decoded) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [decoded.width as usize, decoded.height as usize],
                        &decoded.rgba,
                    );
                    let handle = ctx.load_texture(
                        format!("photo-{}-{}x{}", decoded.index, decoded.width, decoded.height),
                        color,
                        TextureOptions {
                            magnification: egui::TextureFilter::Linear,
                            minification: egui::TextureFilter::Linear,
                            wrap_mode: egui::TextureWrapMode::ClampToEdge,
                            mipmap_mode: Some(egui::TextureFilter::Linear),
                        },
                    );
                    let tex = Tex {
                        handle,
                        size: Vec2::new(decoded.width as f32, decoded.height as f32),
                        source: [decoded.source_width, decoded.source_height],
                        orientation: decoded.orientation,
                        raw: decoded.raw,
                    };
                    self.textures.insert(decoded.index, tex);
                    if decoded.index == self.index {
                        self.shown = decoded.index;
                        self.has_shown = true;
                    }
                    self.evict();
                }
                DecodeEvent::Failed { index, message } => {
                    self.failed.insert(index);
                    if index == self.index {
                        // Keep the explanation visible instead of falling back
                        // to an endless "loading" state.
                        self.error = Some((message, f64::INFINITY));
                    }
                }
            }
        }
    }

    fn evict(&mut self) {
        if self.textures.len() <= MAX_CACHE {
            return;
        }
        let mut candidates: Vec<(usize, usize)> = self
            .textures
            .keys()
            .copied()
            .filter(|&i| i != self.index && i != self.shown)
            .map(|i| (i, i.abs_diff(self.index)))
            .collect();
        candidates.sort_by_key(|(_, distance)| std::cmp::Reverse(*distance));
        for (index, _) in candidates {
            if self.textures.len() <= MAX_CACHE {
                break;
            }
            self.textures.remove(&index);
        }
    }

    fn prefetch(&mut self) {
        if self.photos.is_empty() {
            self.decoder.set_wanted(std::iter::empty());
            return;
        }

        let len = self.photos.len() as isize;
        let mut wanted = HashSet::new();
        for offset in -PREFETCH..=PREFETCH {
            let idx = ((self.index as isize + offset).rem_euclid(len)) as usize;
            wanted.insert(idx);
        }
        self.decoder.set_wanted(wanted.iter().copied());

        // Current first, then the neighbours, so the image on screen wins.
        for offset in [0isize, 1, -1, 2, -2] {
            let idx = ((self.index as isize + offset).rem_euclid(len)) as usize;
            if self.textures.contains_key(&idx) || self.failed.contains(&idx) {
                continue;
            }
            let Some(photo) = self.photos.get(idx) else {
                continue;
            };
            self.decoder.request(idx, photo.path.clone());
        }
    }

    fn snapshot(&self, ctx: &egui::Context) -> Snapshot {
        ctx.input(|i| Snapshot {
            left: i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp),
            right: i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown),
            home: i.key_pressed(Key::Home),
            end: i.key_pressed(Key::End),
            space: i.key_pressed(Key::Space),
            fullscreen: i.key_pressed(Key::F),
            fullscreen_f11: i.key_pressed(Key::F11),
            escape: i.key_pressed(Key::Escape),
            zoom_in: i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
            zoom_out: i.key_pressed(Key::Minus),
            fit: i.key_pressed(Key::Num0),
            actual: i.key_pressed(Key::Num1),
            info: i.key_pressed(Key::I),
            open_folder: i.key_pressed(Key::O) && !i.modifiers.ctrl && !i.modifiers.command,
            open_file: i.key_pressed(Key::O) && (i.modifiers.ctrl || i.modifiers.command),
            open_path_prompt: i.key_pressed(Key::L) && (i.modifiers.ctrl || i.modifiers.command),
            about: i.key_pressed(Key::F1),
            options: i.key_pressed(Key::F2),
            focus_toggle: i.key_pressed(Key::F3),
            toggle_language: i.key_pressed(Key::L) && i.modifiers.alt,
            dropped: i
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect(),
            hovered_files: !i.raw.hovered_files.is_empty(),
            pointer: i.pointer.hover_pos(),
            mouse_moved: i.pointer.delta() != Vec2::ZERO || i.pointer.any_click(),
            time: i.time,
            focused: i.focused,
        })
    }

    fn handle_input(&mut self, ctx: &egui::Context, rect: Rect, snap: &Snapshot) {
        let now = snap.time;

        // While the path prompt is open, its own text field owns the keyboard.
        if self.path_prompt.is_some() {
            return;
        }

        if self.update_available.is_some() {
            if snap.escape {
                self.update_available = None;
            }
            return;
        }

        if self.options_open {
            if snap.escape || snap.options {
                self.options_open = false;
            }
            return;
        }

        if snap.options {
            self.options_open = true;
            self.about_open = false;
            return;
        }

        if self.about_open {
            if snap.escape || snap.about {
                self.about_open = false;
            }
            return;
        }

        if snap.about {
            self.about_open = true;
            return;
        }

        if snap.toggle_language {
            let next = if self.lang == Lang::Da {
                Lang::En
            } else {
                Lang::Da
            };
            self.set_language(next);
            self.last_activity = now;
        }

        if !snap.dropped.is_empty() {
            if let Some(path) = snap.dropped.first() {
                self.open_path(path);
            }
            return;
        }

        if snap.open_path_prompt {
            self.open_path_prompt();
            return;
        }

        if snap.mouse_moved {
            self.last_activity = now;
            if self.cursor_hidden {
                ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
                self.cursor_hidden = false;
            }
        }

        if snap.open_folder {
            self.pick_folder();
        }
        if snap.open_file {
            self.pick_file();
        }

        if snap.escape {
            if self.slideshow_next.take().is_some() {
                // First Escape stops the slideshow.
            } else if self.fullscreen {
                self.set_fullscreen(ctx, false);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.last_activity = now;
        }

        if snap.fullscreen || snap.fullscreen_f11 {
            self.toggle_fullscreen(ctx, now);
        }

        if snap.left {
            self.navigate(-1, now);
        }
        if snap.right {
            self.navigate(1, now);
        }
        if snap.home {
            self.jump_to(0, now);
        }
        if snap.end && !self.photos.is_empty() {
            self.jump_to(self.photos.len() - 1, now);
        }

        if snap.space {
            self.slideshow_next = match self.slideshow_next {
                Some(_) => None,
                None => Some(now + SLIDESHOW_SECS),
            };
            self.last_activity = now;
        }

        if snap.info {
            self.exif_visible = !self.exif_visible;
            self.hud_pinned = self.exif_visible;
            if self.exif_visible {
                self.request_exif();
            }
            self.last_activity = now;
        }

        if snap.focus_toggle {
            self.focus_visible = !self.focus_visible;
            if self.focus_visible {
                self.request_focus();
            }
            self.last_activity = now;
        }

        if snap.fit {
            self.fit = true;
            self.pan = Vec2::ZERO;
        }
        if snap.actual {
            self.fit = false;
            self.zoom = 1.0 / ctx.pixels_per_point();
            self.pan = Vec2::ZERO;
        }
        if snap.zoom_in {
            self.zoom_by(1.15, snap.pointer, rect);
        }
        if snap.zoom_out {
            self.zoom_by(1.0 / 1.15, snap.pointer, rect);
        }

        self.handle_wheel(ctx, rect, snap);

        if let Some(next) = self.slideshow_next {
            if now >= next {
                self.navigate(1, now);
                self.slideshow_next = Some(now + SLIDESHOW_SECS);
            }
        }
    }

    fn handle_wheel(&mut self, ctx: &egui::Context, rect: Rect, snap: &Snapshot) {
        let mut zoom_notches = 0.0f32;
        let mut scroll_points = 0.0f32;

        ctx.input(|i| {
            for event in &i.events {
                let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                } = event
                else {
                    continue;
                };
                if modifiers.ctrl || modifiers.command {
                    zoom_notches += match unit {
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Point => delta.y / 40.0,
                        egui::MouseWheelUnit::Page => delta.y * 3.0,
                    };
                } else {
                    scroll_points += match unit {
                        egui::MouseWheelUnit::Line => delta.y * WHEEL_POINTS_PER_IMAGE,
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Page => delta.y * 400.0,
                    };
                }
            }
        });

        if zoom_notches != 0.0 {
            self.zoom_by(1.2f32.powf(zoom_notches), snap.pointer, rect);
            self.last_activity = snap.time;
        }

        if scroll_points != 0.0 {
            if snap.time - self.last_wheel > 0.8 {
                self.wheel_acc = 0.0;
            }
            self.last_wheel = snap.time;
            self.wheel_acc += scroll_points;
            self.last_activity = snap.time;
        }

        // One wheel notch is 50 points. Accumulate so precision trackpads also
        // move exactly one image per deliberate gesture, and cap a burst so a
        // free-spinning wheel cannot skip half the folder in one frame.
        let mut steps = 0i32;
        while self.wheel_acc.abs() >= WHEEL_POINTS_PER_IMAGE && steps.abs() < 3 {
            let dir = self.wheel_acc.signum();
            self.wheel_acc -= dir * WHEEL_POINTS_PER_IMAGE;
            steps -= dir as i32; // content down (wheel up) => previous image
        }
        if steps != 0 {
            self.navigate(steps as isize, snap.time);
        }
    }

    fn pick_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title(i18n::tr(self.lang, "dialog.open_folder"))
            .pick_folder()
        {
            self.load_folder(&dir, None);
        }
    }

    fn pick_file(&mut self) {
        if let Some(file) = rfd::FileDialog::new()
            .set_title(i18n::tr(self.lang, "dialog.open_image"))
            .add_filter(i18n::tr(self.lang, "dialog.images"), library::SUPPORTED_EXTENSIONS)
            .pick_file()
        {
            self.open_path(&file);
        }
    }

    fn draw(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());

                if self.photos.is_empty() {
                    if self.scan.is_none() {
                        self.draw_empty_state(ctx);
                    }
                    return;
                }

                let margin = if self.fullscreen { 4.0 } else { 18.0 };
                let avail = rect.shrink(margin);

                let display = self
                    .displayed_tex()
                    .map(|(idx, tex)| {
                        (
                            idx,
                            tex.handle.id(),
                            tex.size,
                            tex.source,
                            tex.orientation,
                        )
                    });

                if let Some((idx, texture_id, size, _source, orientation)) = display {
                    let fit = (avail.width() / size.x)
                        .min(avail.height() / size.y)
                        .min(1.0);
                    let scale = if self.fit { fit } else { self.zoom };
                    self.last_scale = scale;

                    let draw_size = size * scale;
                    let max_pan = Vec2::new(
                        ((draw_size.x - avail.width()) * 0.5).max(0.0),
                        ((draw_size.y - avail.height()) * 0.5).max(0.0),
                    );
                    self.pan.x = self.pan.x.clamp(-max_pan.x, max_pan.x);
                    self.pan.y = self.pan.y.clamp(-max_pan.y, max_pan.y);

                    let center = avail.center() + self.pan;
                    let image_rect = Rect::from_center_size(center, draw_size);

                    ui.painter().image(
                        texture_id,
                        image_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    self.draw_focus(ui, image_rect, idx, orientation);

                    let zoomed_in = draw_size.x > avail.width() + 1.0
                        || draw_size.y > avail.height() + 1.0;
                    if zoomed_in {
                        if response.dragged() {
                            self.pan += response.drag_delta();
                            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                        } else if response.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::Grab);
                        }
                    }

                    if response.double_clicked() {
                        self.toggle_fullscreen(ctx, snap.time);
                    }
                    if self.fullscreen && response.secondary_clicked() {
                        self.set_fullscreen(ctx, false);
                    }

                    if !self.fullscreen {
                        let lang = self.lang;
                        response.context_menu(|ui| {
                            if ui.button(i18n::tr(lang, "menu.fullscreen")).clicked() {
                                self.toggle_fullscreen(ctx, snap.time);
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.fit")).clicked() {
                                self.fit = true;
                                self.pan = Vec2::ZERO;
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.actual")).clicked() {
                                self.fit = false;
                                self.zoom = 1.0 / ctx.pixels_per_point();
                                self.pan = Vec2::ZERO;
                                ui.close_menu();
                            }
                            let focus_key = if self.focus_visible {
                                "menu.focus_hide"
                            } else {
                                "menu.focus_show"
                            };
                            if ui.button(i18n::tr(lang, focus_key)).clicked() {
                                self.focus_visible = !self.focus_visible;
                                if self.focus_visible {
                                    self.request_focus();
                                }
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(i18n::tr(lang, "menu.open_folder")).clicked() {
                                self.pick_folder();
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.open_file")).clicked() {
                                self.pick_file();
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.open_path")).clicked() {
                                self.open_path_prompt();
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(i18n::tr(lang, "menu.options")).clicked() {
                                self.options_open = true;
                                self.about_open = false;
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.about")).clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                            if ui.button(i18n::tr(lang, "menu.close")).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }

                    self.draw_arrows(ui, avail, snap.time);
                } else if self.error.is_none() {
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        i18n::tr(self.lang, "overlay.loading"),
                        FontId::proportional(15.0),
                        Color32::from_gray(150),
                    );
                }
            });

        self.draw_hud(ctx, snap);
        self.draw_overlays(ctx, snap);
    }

    fn draw_arrows(&mut self, ui: &mut egui::Ui, avail: Rect, now: f64) {
        if self.photos.len() < 2 || self.hud_alpha < 0.12 {
            return;
        }
        let alpha = self.hud_alpha;

        for (id, dir, x, glyph_dir) in [
            ("prev-arrow", -1isize, avail.left() + 30.0, -1.0f32),
            ("next-arrow", 1isize, avail.right() - 30.0, 1.0f32),
        ] {
            let center = Pos2::new(x, avail.center().y);
            let hit = Rect::from_center_size(center, Vec2::splat(56.0));
            let response = ui.interact(hit, Id::new(id), Sense::click());
            let hot = response.hovered();
            if hot {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let painter = ui.painter();
            let bg_alpha = if hot { 200.0 } else { 105.0 };
            painter.circle_filled(
                center,
                22.0,
                Color32::from_rgba_unmultiplied(20, 22, 28, (bg_alpha * alpha) as u8),
            );
            let stroke = Stroke::new(
                2.0_f32,
                Color32::from_rgba_unmultiplied(235, 238, 242, (225.0 * alpha) as u8),
            );
            let tip = center + Vec2::new(glyph_dir * 5.0, 0.0);
            painter.line_segment([tip + Vec2::new(-glyph_dir * 5.0, -8.0), tip], stroke);
            painter.line_segment([tip, tip + Vec2::new(-glyph_dir * 5.0, 8.0)], stroke);

            if response.clicked() {
                self.navigate(dir, now);
            }
        }
    }

    fn draw_focus(&self, ui: &egui::Ui, image_rect: Rect, index: usize, orientation: u16) {
        if !self.focus_visible {
            return;
        }
        let data = match self.focus_cache.get(&index) {
            Some(Some(data)) => data,
            Some(None) => {
                ui.painter().text(
                    image_rect.center_bottom() - Vec2::new(0.0, 24.0),
                    Align2::CENTER_BOTTOM,
                    i18n::tr(self.lang, "focus.none"),
                    FontId::proportional(12.0),
                    Color32::from_gray(190),
                );
                return;
            }
            None => {
                ui.painter().text(
                    image_rect.center_bottom() - Vec2::new(0.0, 24.0),
                    Align2::CENTER_BOTTOM,
                    i18n::tr(self.lang, "focus.loading"),
                    FontId::proportional(12.0),
                    Color32::from_gray(190),
                );
                return;
            }
        };

        for point in &data.points {
            let (mut cx, mut cy, mut width, mut height) =
                (point.x, point.y, point.width, point.height);
            match orientation {
                3 => {
                    cx = 1.0 - cx;
                    cy = 1.0 - cy;
                }
                6 => {
                    let (old_x, old_y) = (cx, cy);
                    cx = 1.0 - old_y;
                    cy = old_x;
                    std::mem::swap(&mut width, &mut height);
                }
                8 => {
                    let (old_x, old_y) = (cx, cy);
                    cx = old_y;
                    cy = 1.0 - old_x;
                    std::mem::swap(&mut width, &mut height);
                }
                _ => {}
            }

            let center = Pos2::new(
                image_rect.left() + cx * image_rect.width(),
                image_rect.top() + cy * image_rect.height(),
            );
            let rect = Rect::from_center_size(
                center,
                Vec2::new(
                    (width * image_rect.width()).max(12.0),
                    (height * image_rect.height()).max(12.0),
                ),
            )
            .intersect(image_rect);

            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                continue;
            }

            ui.painter().rect_filled(
                rect,
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(255, 70, 60, 24),
            );
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(2.0_f32, Color32::from_rgb(255, 76, 66)),
                StrokeKind::Inside,
            );
        }
    }

    fn draw_hud(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        if self.photos.is_empty() {
            return;
        }
        let lang = self.lang;
        let active = self.hud_pinned
            || snap.time - self.last_activity < HUD_IDLE_SECS
            || self.slideshow_next.is_some();
        let alpha = ctx.animate_bool_with_time(Id::new("hud-alpha"), active, 0.28);
        self.hud_alpha = alpha;
        if alpha < 0.02 {
            return;
        }

        let Some(photo) = self.photos.get(self.index) else {
            return;
        };
        let name = shorten(&photo.name, 56);
        let folder = self.folder.as_deref().map(short_folder).unwrap_or_default();
        let dims = self
            .textures
            .get(&self.index)
            .map(|t| {
                let w = t.source[0].to_string();
                let h = t.source[1].to_string();
                let key = if t.raw { "hud.raw_dims" } else { "hud.dims" };
                i18n::tf(lang, key, &[("w", &w), ("h", &h)])
            })
            .unwrap_or_else(|| "…".to_owned());
        let size = library::human_size(photo.size);
        let counter = format!("{} / {}", self.index + 1, self.photos.len());

        let white = |a: f32| Color32::from_rgba_unmultiplied(240, 242, 245, (a * 255.0) as u8);
        let muted = |a: f32| Color32::from_rgba_unmultiplied(170, 176, 184, (a * 255.0) as u8);
        let fill = |a: f32| Color32::from_rgba_unmultiplied(12, 14, 18, (a * 228.0) as u8);

        let screen = ctx.screen_rect();

        egui::Area::new(Id::new("hud-left"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(screen.left_top() + Vec2::new(18.0, 16.0))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                egui::Frame::NONE
                    .fill(fill(alpha))
                    .corner_radius(CornerRadius::same(11))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(492.0);
                        ui.label(RichText::new(name).size(16.0).strong().color(white(alpha)));
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!(
                                "{folder}   ·   {counter}   ·   {dims}   ·   {size}"
                            ))
                            .size(12.0)
                            .color(muted(alpha * 0.95)),
                        );
                        if self.exif_visible {
                            ui.add_space(5.0);
                            match self.exif_cache.get(&self.index) {
                                Some(Some(data)) => {
                                    for line in data.lines() {
                                        ui.label(
                                            RichText::new(line)
                                                .size(11.5)
                                                .color(muted(alpha * 0.88)),
                                        );
                                    }
                                }
                                Some(None) => {
                                    ui.label(
                                        RichText::new(i18n::tr(lang, "exif.none"))
                                            .size(11.0)
                                            .color(muted(alpha * 0.75)),
                                    );
                                }
                                None => {
                                    ui.label(
                                        RichText::new(i18n::tr(lang, "exif.loading"))
                                            .size(11.0)
                                            .color(muted(alpha * 0.75)),
                                    );
                                }
                            }
                        }
                    });
            });

        let zoom_label = self.zoom_label(ctx);
        let slideshow = self.slideshow_next.is_some();
        let right_width = 190.0;

        egui::Area::new(Id::new("hud-right"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(Pos2::new(
                screen.right() - 18.0 - right_width,
                screen.top() + 16.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(right_width);
                egui::Frame::NONE
                    .fill(fill(alpha))
                    .corner_radius(CornerRadius::same(11))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(right_width - 28.0);
                        ui.horizontal(|ui| {
                            if slideshow {
                                ui.label(
                                    RichText::new(i18n::tr(lang, "hud.auto"))
                                        .size(11.0)
                                        .strong()
                                        .color(Color32::from_rgb(110, 190, 255)),
                                );
                                ui.add_space(10.0);
                            }
                            ui.label(RichText::new(zoom_label).size(12.0).color(muted(alpha)));
                        });
                    });
            });
    }

    fn zoom_label(&self, ctx: &egui::Context) -> String {
        let ppp = ctx.pixels_per_point();
        let scale = if self.fit {
            self.last_scale
        } else {
            self.zoom
        };
        let percent = (scale * ppp * 100.0).round() as i32;
        let p = percent.to_string();
        if self.fit {
            i18n::tf(self.lang, "hud.zoom_fit", &[("p", &p)])
        } else {
            i18n::tf(self.lang, "hud.zoom_actual", &[("p", &p)])
        }
    }

    fn draw_overlays(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let lang = self.lang;
        let now = snap.time;
        if !self.hint_scheduled {
            self.hint_scheduled = true;
            self.hint_until = now + HINT_SECS;
        }
        let screen = ctx.screen_rect();
        let bottom = screen.bottom();

        if snap.hovered_files && !self.photos.is_empty() {
            let w = 230.0;
            egui::Area::new(Id::new("drop"))
                .order(egui::Order::Foreground)
                .interactable(false)
                .fixed_pos(Pos2::new(
                    screen.center().x - w * 0.5,
                    screen.center().y - 34.0,
                ))
                .show(ctx, |ui| {
                    ui.set_width(w);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(20, 22, 28, 220))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::same(22))
                        .show(ui, |ui| {
                            ui.set_min_width(w - 44.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(i18n::tr(lang, "overlay.drop"))
                                        .size(18.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                            });
                        });
                });
        }

        if !self.photos.is_empty() && now < self.hint_until {
            let fade = ((self.hint_until - now) / 1.2).clamp(0.0, 1.0) as f32;
            let w = 380.0;
            egui::Area::new(Id::new("hint"))
                .order(egui::Order::Foreground)
                .interactable(false)
                .fixed_pos(Pos2::new(screen.center().x - w * 0.5, bottom - 52.0))
                .show(ctx, |ui| {
                    ui.set_width(w);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(12, 14, 18, (170.0 * fade) as u8))
                        .corner_radius(CornerRadius::same(9))
                        .inner_margin(Margin::symmetric(14, 8))
                        .show(ui, |ui| {
                            ui.set_min_width(w - 28.0);
                            ui.vertical_centered(|ui| {
                                let hint_key = if self.fullscreen {
                                    "overlay.hint_fullscreen"
                                } else {
                                    "overlay.hint"
                                };
                                ui.label(
                                    RichText::new(i18n::tr(lang, hint_key))
                                    .size(12.0)
                                    .color(Color32::from_rgba_unmultiplied(
                                        210,
                                        214,
                                        220,
                                        (225.0 * fade) as u8,
                                    )),
                                );
                            });
                        });
                });
        }

        if !self.photos.is_empty()
            && !self.textures.contains_key(&self.index)
            && !self.failed.contains(&self.index)
            && self.error.is_none()
        {
            let w = 132.0;
            egui::Area::new(Id::new("loading"))
                .order(egui::Order::Foreground)
                .interactable(false)
                .fixed_pos(Pos2::new(screen.center().x - w * 0.5, bottom - 96.0))
                .show(ctx, |ui| {
                    ui.set_width(w);
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(12, 14, 18, 175))
                        .corner_radius(CornerRadius::same(9))
                        .inner_margin(Margin::symmetric(12, 7))
                        .show(ui, |ui| {
                            ui.set_min_width(w - 24.0);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Spinner::new()
                                        .size(13.0)
                                        .color(Color32::from_gray(190)),
                                );
                                ui.label(
                                    RichText::new(i18n::tr(lang, "overlay.loading"))
                                        .size(12.0)
                                        .color(Color32::from_gray(205)),
                                );
                            });
                        });
                });
        }

        self.draw_scan_overlay(ctx, snap);

        if let Some((message, at)) = &self.error {
            if now < *at {
                let w = 660.0;
                egui::Area::new(Id::new("error"))
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .fixed_pos(Pos2::new(screen.center().x - w * 0.5, bottom - 160.0))
                    .show(ctx, |ui| {
                        ui.set_width(w);
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(70, 22, 24, 235))
                            .corner_radius(CornerRadius::same(9))
                            .inner_margin(Margin::symmetric(14, 9))
                            .show(ui, |ui| {
                                ui.set_min_width(w - 28.0);
                                ui.label(
                                    RichText::new(shorten(message, 220))
                                        .size(12.0)
                                        .color(Color32::from_rgb(255, 220, 220)),
                                );
                            });
                    });
            } else {
                self.error = None;
            }
        }

        self.draw_about(ctx);
        self.draw_options(ctx);
        self.draw_update(ctx);
        self.draw_path_prompt(ctx);
    }

    fn draw_about(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let lang = self.lang;

        let w = 470.0;
        let screen = ctx.screen_rect();
        let mut close = false;

        egui::Area::new(Id::new("about"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 150.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(22, 24, 30, 252))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(26))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 52.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Bladr")
                                    .size(34.0)
                                    .strong()
                                    .color(Color32::from_rgb(240, 243, 247)),
                            );
                            ui.label(
                                RichText::new(i18n::tf(
                                    lang,
                                    "about.version",
                                    &[
                                        ("version", env!("CARGO_PKG_VERSION")),
                                        ("subtitle", i18n::tr(lang, "app.subtitle")),
                                    ],
                                ))
                                .size(13.0)
                                .color(Color32::from_rgb(120, 190, 255)),
                            );
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new(i18n::tr(lang, "about.formats"))
                                .size(12.0)
                                .line_height(Some(19.0))
                                .color(Color32::from_gray(160)),
                            );
                            ui.add_space(14.0);
                            ui.label(
                                RichText::new(i18n::tr(lang, "about.license"))
                                    .size(11.0)
                                    .color(Color32::from_gray(120)),
                            );
                            ui.add_space(16.0);
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(i18n::tr(lang, "app.close")).size(14.0))
                                        .min_size(Vec2::new(120.0, 34.0))
                                        .corner_radius(CornerRadius::same(8)),
                                )
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
            });

        if close {
            self.about_open = false;
        }
    }

    fn draw_options(&mut self, ctx: &egui::Context) {
        if !self.options_open {
            return;
        }

        let lang = self.lang;
        let w = 480.0;
        let screen = ctx.screen_rect();
        let mut close = false;
        let mut check_now = false;
        let mut next_lang = None;

        egui::Area::new(Id::new("options"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 190.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(22, 24, 30, 252))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(26))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 52.0);
                        ui.label(
                            RichText::new(i18n::tr(lang, "options.title"))
                                .size(24.0)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(i18n::tf(
                                lang,
                                "options.version",
                                &[("version", env!("CARGO_PKG_VERSION"))],
                            ))
                            .size(12.0)
                            .color(Color32::from_gray(150)),
                        );
                        ui.add_space(16.0);

                        let changed = ui
                            .checkbox(
                                &mut self.settings.auto_update,
                                i18n::tr(lang, "options.auto_update"),
                            )
                            .changed();
                        if changed {
                            self.persist_settings();
                            if self.settings.auto_update && self.update_check.is_none() {
                                self.start_update_check();
                            }
                        }

                        if ui
                            .checkbox(
                                &mut self.settings.start_fullscreen,
                                i18n::tr(lang, "options.start_fullscreen"),
                            )
                            .changed()
                        {
                            self.persist_settings();
                        }
                        ui.label(
                            RichText::new(i18n::tr(lang, "options.start_fullscreen_hint"))
                                .size(10.5)
                                .color(Color32::from_gray(125)),
                        );
                        if let Some(error) = &self.settings_error {
                            ui.label(
                                RichText::new(i18n::tf(
                                    lang,
                                    "options.save_failed",
                                    &[("e", error)],
                                ))
                                .size(10.5)
                                .color(Color32::from_rgb(235, 120, 100)),
                            );
                        }

                        ui.add_space(8.0);
                        let status = match self.update_status {
                            UpdateStatus::Checking => {
                                Some(i18n::tr(lang, "options.checking").to_owned())
                            }
                            UpdateStatus::UpToDate => {
                                Some(i18n::tr(lang, "options.up_to_date").to_owned())
                            }
                            UpdateStatus::Failed => {
                                Some(i18n::tr(lang, "options.failed").to_owned())
                            }
                            UpdateStatus::Idle => None,
                        };
                        if let Some(status) = status {
                            ui.label(
                                RichText::new(status)
                                    .size(11.0)
                                    .color(Color32::from_gray(155)),
                            );
                        } else if let Some(info) = &self.update_available {
                            ui.label(
                                RichText::new(i18n::tf(
                                    lang,
                                    "update.body",
                                    &[
                                        ("new", &info.version),
                                        ("current", env!("CARGO_PKG_VERSION")),
                                    ],
                                ))
                                .size(11.0)
                                .color(Color32::from_rgb(120, 190, 255)),
                            );
                        }

                        ui.add_space(10.0);
                        if ui
                            .add_enabled(
                                self.update_status != UpdateStatus::Checking,
                                egui::Button::new(i18n::tr(lang, "options.check_now")),
                            )
                            .clicked()
                        {
                            check_now = true;
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(i18n::tr(lang, "options.language"))
                                .size(12.0)
                                .color(Color32::from_gray(160)),
                        );
                        for choice in Lang::ALL {
                            if ui
                                .radio(self.lang == choice, choice.label())
                                .clicked()
                            {
                                next_lang = Some(choice);
                            }
                        }

                        ui.add_space(18.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(i18n::tr(lang, "options.close")).size(14.0),
                                )
                                .min_size(Vec2::new(120.0, 34.0))
                                .corner_radius(CornerRadius::same(8)),
                            )
                            .clicked()
                        {
                            self.persist_settings();
                            close = true;
                        }
                    });
            });

        if let Some(choice) = next_lang {
            self.set_language(choice);
        }
        if check_now {
            self.start_update_check();
        }
        if close {
            self.options_open = false;
        }
    }

    fn draw_update(&mut self, ctx: &egui::Context) {
        let Some(info) = self.update_available.clone() else {
            return;
        };

        let lang = self.lang;
        let w = 460.0;
        let screen = ctx.screen_rect();
        let mut install = false;
        let mut later = false;

        egui::Area::new(Id::new("update"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 120.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(22, 24, 30, 252))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(24))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 48.0);
                        ui.label(
                            RichText::new(i18n::tr(lang, "update.title"))
                                .size(22.0)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(i18n::tf(
                                lang,
                                "update.body",
                                &[
                                    ("new", &info.version),
                                    ("current", env!("CARGO_PKG_VERSION")),
                                ],
                            ))
                            .size(13.0)
                            .color(Color32::from_gray(180)),
                        );
                        ui.add_space(14.0);

                        if self.update_downloading {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(14.0));
                                ui.label(
                                    RichText::new(i18n::tr(lang, "update.downloading"))
                                        .size(12.0)
                                        .color(Color32::from_gray(170)),
                                );
                            });
                        } else {
                            if let Some(error) = &self.update_error {
                                ui.label(
                                    RichText::new(shorten(error, 160))
                                        .size(11.0)
                                        .color(Color32::from_rgb(255, 160, 160)),
                                );
                                ui.add_space(8.0);
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(i18n::tr(lang, "update.download"))
                                                .size(13.0),
                                        )
                                        .corner_radius(CornerRadius::same(8)),
                                    )
                                    .clicked()
                                {
                                    install = true;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(i18n::tr(lang, "update.later"))
                                                .size(13.0),
                                        )
                                        .corner_radius(CornerRadius::same(8)),
                                    )
                                    .clicked()
                                {
                                    later = true;
                                }
                            });
                        }
                    });
            });

        if install {
            self.start_update_download();
        }
        if later {
            self.update_available = None;
        }
    }

    fn draw_scan_overlay(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let Some(job) = self.scan.as_ref() else {
            return;
        };
        let lang = self.lang;
        let path = short_folder(&job.dir);
        let network = job.dir.to_string_lossy().starts_with("\\\\");
        let w = 460.0;
        let screen = ctx.screen_rect();
        let mut cancel = false;

        egui::Area::new(Id::new("scan-overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 58.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 26, 248))
                    .corner_radius(CornerRadius::same(13))
                    .inner_margin(Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 40.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Spinner::new()
                                    .size(16.0)
                                    .color(Color32::from_gray(200)),
                            );
                            ui.label(
                                RichText::new(if network {
                                    i18n::tr(lang, "scan.network")
                                } else {
                                    i18n::tr(lang, "scan.local")
                                })
                                .size(15.0)
                                .strong(),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(path)
                                .size(12.0)
                                .color(Color32::from_gray(165)),
                        );
                        if network {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(i18n::tr(lang, "scan.network_hint"))
                                .size(11.0)
                                .color(Color32::from_gray(135)),
                            );
                        }
                        ui.add_space(12.0);
                        if ui.button(i18n::tr(lang, "scan.cancel")).clicked() {
                            cancel = true;
                        }
                    });
            });

        if cancel {
            self.scan = None;
        }
        ctx.request_repaint_after(Duration::from_millis(120));
        let _ = snap;
    }

    fn draw_path_prompt(&mut self, ctx: &egui::Context) {
        if self.path_prompt.is_none() {
            return;
        }
        let lang = self.lang;

        let w = 580.0;
        let screen = ctx.screen_rect();
        let mut text = self.path_prompt.take().unwrap();
        let error = self.path_prompt_error.clone();
        let focus_now = self.path_prompt_focus;
        self.path_prompt_focus = false;

        let mut submit = false;
        let mut cancel = false;

        egui::Area::new(Id::new("path-prompt"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 120.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(22, 24, 30, 252))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(22))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 44.0);
                        ui.label(RichText::new(i18n::tr(lang, "path.title")).size(18.0).strong());
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                i18n::tr(lang, "path.help"),
                            )
                            .size(12.0)
                            .color(Color32::from_gray(150)),
                        );
                        ui.add_space(10.0);

                        let response = ui.add(
                            egui::TextEdit::singleline(&mut text)
                                .id(Id::new("path-edit"))
                                .desired_width(w - 44.0)
                                .hint_text("\\\\192.168.1.221\\foto"),
                        );
                        if focus_now {
                            response.request_focus();
                        }

                        if !error.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(error.clone())
                                    .size(12.0)
                                    .color(Color32::from_rgb(255, 160, 160)),
                            );
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button(i18n::tr(lang, "path.open")).clicked() {
                                submit = true;
                            }
                            if ui.button(i18n::tr(lang, "path.cancel")).clicked() {
                                cancel = true;
                            }
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(i18n::tr(lang, "path.footer"))
                                    .size(11.0)
                                    .color(Color32::from_gray(125)),
                            );
                        });

                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                            submit = true;
                        }
                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            cancel = true;
                        }
                    });
            });

        if submit {
            self.path_prompt = Some(text);
            self.submit_path_prompt();
        } else if cancel {
            self.path_prompt = None;
        } else {
            self.path_prompt = Some(text);
        }
    }

    fn draw_empty_state(&mut self, ctx: &egui::Context) {
        let has_folder = self.folder.is_some();
        let lang = self.lang;
        let screen = ctx.screen_rect();
        let w = 460.0;
        egui::Area::new(Id::new("empty"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(
                screen.center().x - w * 0.5,
                screen.center().y - 215.0,
            ))
            .show(ctx, |ui| {
                ui.set_width(w);
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(14, 16, 20, 235))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(Margin::same(30))
                    .show(ui, |ui| {
                        ui.set_min_width(w - 60.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Bladr")
                                    .size(42.0)
                                    .strong()
                                    .color(Color32::from_rgb(240, 243, 247)),
                            );
                            let subtitle = if has_folder {
                                i18n::tr(lang, "empty.no_images")
                            } else {
                                i18n::tr(lang, "empty.subtitle")
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{subtitle}   ·   v{}",
                                    env!("CARGO_PKG_VERSION")
                                ))
                                .size(14.0)
                                .color(Color32::from_gray(160)),
                            );
                            ui.add_space(22.0);

                            let button = |text: &str| {
                                egui::Button::new(RichText::new(text).size(15.0))
                                    .min_size(Vec2::new(240.0, 42.0))
                                    .corner_radius(CornerRadius::same(9))
                            };

                            if ui.add(button(i18n::tr(lang, "empty.open_folder"))).clicked() {
                                self.pick_folder();
                            }
                            ui.add_space(8.0);
                            if ui.add(button(i18n::tr(lang, "empty.open_image"))).clicked() {
                                self.pick_file();
                            }
                            ui.add_space(8.0);
                            if ui.add(button(i18n::tr(lang, "empty.open_path"))).clicked() {
                                self.open_path_prompt();
                            }

                            ui.add_space(22.0);
                            ui.label(
                                RichText::new(i18n::tr(lang, "empty.hint"))
                                .size(12.0)
                                .line_height(Some(20.0))
                                .color(Color32::from_gray(130)),
                            );
                        });
                    });
            });
    }

    fn end_frame(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let title = self.desired_title();
        if title != self.title {
            self.title = title;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title.clone()));
        }

        // Ask for maximized once after the window exists. Without an explicit
        // inner size, the initial maximize request then covers the work area.
        if self.maximize_stage == 0 {
            self.maximize_stage = 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            ctx.request_repaint();
        }

        if self.start_fullscreen {
            self.start_fullscreen = false;
            self.set_fullscreen(ctx, true);
        }

        // Cursor: only auto-hide in fullscreen, and only once the HUD is gone.
        let idle = snap.time - self.last_activity;
        if self.fullscreen
            && snap.focused
            && idle > 1.2
            && self.hud_alpha < 0.05
            && !self.cursor_hidden
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(false));
            self.cursor_hidden = true;
        }

        let loading = !self.photos.is_empty()
            && !self.textures.contains_key(&self.index)
            && !self.failed.contains(&self.index)
            && self.error.is_none();
        let animating = self.hud_alpha > 0.0 && self.hud_alpha < 1.0;
        if loading || self.slideshow_next.is_some() || animating {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        if snap.time < self.hint_until {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

impl eframe::App for BladrApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = settings::save(&self.settings);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snap = self.snapshot(ctx);
        let rect = ctx.screen_rect();

        if self.last_activity == 0.0 {
            self.last_activity = snap.time;
        }

        self.poll_scan(snap.time);
        self.poll_decoder(ctx, snap.time);
        self.poll_update();
        self.poll_exif();
        self.poll_focus();
        self.poll_incoming(ctx);
        self.handle_input(ctx, rect, &snap);
        self.prefetch();
        self.draw(ctx, &snap);
        self.end_frame(ctx, &snap);
    }
}

fn fit_scale(rect: Rect, tex: &Tex) -> f32 {
    (rect.width() / tex.size.x)
        .min(rect.height() / tex.size.y)
        .min(1.0)
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn short_folder(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    match path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
    {
        Some(parent) => format!("{parent}\\{name}"),
        None => name,
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("Bladr")
            .join("last_folder.txt")
    })
}

fn load_last_folder() -> Option<PathBuf> {
    let path = config_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let folder = PathBuf::from(text.trim());
    folder.is_dir().then_some(folder)
}

fn save_last_folder(folder: &Path) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, folder.to_string_lossy().as_bytes());
}

