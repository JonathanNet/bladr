//! Small, dependency-free UI localisation.
//!
//! Adding a language means adding one table below; unknown keys fall back to
//! English and then to the key itself, so a partial translation is safe.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Da,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Da, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Da => "da",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        let code = code.trim().to_ascii_lowercase();
        if code.starts_with("da") {
            Some(Lang::Da)
        } else if code.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Da => "Dansk",
            Lang::En => "English",
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Lang::Da => 0,
            Lang::En => 1,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        if value == 0 {
            Lang::Da
        } else {
            Lang::En
        }
    }

    /// `BLADR_LANG`, then the saved menu choice, then the Windows UI language.
    pub fn initial() -> Self {
        if let Ok(value) = std::env::var("BLADR_LANG") {
            if let Some(lang) = Lang::from_code(&value) {
                return lang;
            }
        }
        if let Some(saved) = load_saved() {
            return saved;
        }
        Lang::system()
    }

    pub fn system() -> Self {
        #[cfg(windows)]
        {
            if windows_ui_language() & 0x3ff == 0x06 {
                return Lang::Da;
            }
            return Lang::En;
        }

        #[cfg(not(windows))]
        {
            for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
                if let Ok(value) = std::env::var(name) {
                    if value.to_ascii_lowercase().starts_with("da") {
                        return Lang::Da;
                    }
                }
            }
            Lang::En
        }
    }
}

#[cfg(windows)]
fn windows_ui_language() -> u16 {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    unsafe { GetUserDefaultUILanguage() }
}

fn settings_path() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        std::path::PathBuf::from(appdata)
            .join("Bladr")
            .join("language.txt")
    })
}

fn load_saved() -> Option<Lang> {
    let path = settings_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    Lang::from_code(&text)
}

pub fn save(lang: Lang) {
    let Some(path) = settings_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, lang.code());
}

/// Global language for the decoder thread.
static DECODER_LANG: AtomicU8 = AtomicU8::new(1);

pub fn set_decoder_lang(lang: Lang) {
    DECODER_LANG.store(lang.as_u8(), Ordering::Relaxed);
}

pub fn decoder_lang() -> Lang {
    Lang::from_u8(DECODER_LANG.load(Ordering::Relaxed))
}

const DA: &[(&str, &str)] = &[
    ("app.subtitle", "hurtig billedfremviser"),
    ("app.formats", "JPEG · PNG · WebP · TIFF · RAW (CR3, NEF, ARW, DNG)\nFuldskærm, musens rullehjul og auto-afspilning."),
    ("app.license", "© 2026 · MIT-licens"),
    ("app.close", "Luk"),
    ("menu.fullscreen", "Fuldskærm        F"),
    ("menu.fit", "Tilpas i vinduet    0"),
    ("menu.actual", "Faktisk størrelse    1"),
    ("menu.focus_show", "Vis fokuspunkter    F3"),
    ("menu.focus_hide", "Skjul fokuspunkter    F3"),
    ("menu.histogram_show", "Vis histogram    H"),
    ("menu.histogram_hide", "Skjul histogram    H"),
    ("menu.wallpaper", "Sæt som baggrund"),
    ("menu.delete", "Slet billede…    Del"),
    ("menu.file_location", "Vis fil lokation"),
    ("menu.screenshot", "Tag skærmbillede…    S"),
    ("capture.not_ready", "Billedet er ikke klar endnu"),
    ("capture.instruction", "Træk for at vælge område · Esc annullerer"),
    ("capture.saved", "Skærmbillede kopieret og gemt: {path}"),
    ("capture.failed", "Kunne ikke gemme skærmbillede: {e}"),
    ("file_location.failed", "Kunne ikke åbne Stifinder: {e}"),
    ("delete.title", "Slet billede?"),
    ("delete.warning", "Er du sikker på, at du vil slette “{name}”? Billedet flyttes til papirkurven. Hvis papirkurven ikke kan bruges, slettes det permanent."),
    ("delete.warning_network", "Er du sikker på, at du vil slette “{name}”? På netværksdrev slettes billedet permanent, fordi der ikke er nogen papirkurv."),
    ("delete.cancel", "Annuller"),
    ("delete.confirm", "Slet"),
    ("delete.busy", "Sletter…"),
    ("delete.failed", "Kunne ikke slette billedet: {e}"),
    ("wallpaper.centered", "Centreret"),
    ("wallpaper.tiled", "Flise"),
    ("wallpaper.stretched", "Strakt"),
    ("wallpaper.restore", "Gendan"),
    ("slideshow.delay", "Forsinkelse"),
    ("slideshow.delay_hint", "Sekunder pr. billede"),
    ("slideshow.prev", "Forrige"),
    ("slideshow.pause", "Pause"),
    ("slideshow.play", "Afspil"),
    ("slideshow.next", "Næste"),
    ("slideshow.loop", "Gentag"),
    ("slideshow.shuffle", "Tilfældig"),
    ("slideshow.exit", "Afslut"),
    ("slideshow.error", "Slideshow-fejl"),
    ("menu.open_folder", "Åbn mappe…        O"),
    ("menu.open_file", "Åbn billede…    Ctrl+O"),
    ("menu.open_path", "Åbn sti…        Ctrl+L"),
    ("menu.language", "Sprog (Alt+L)"),
    ("menu.options", "Indstillinger…    F2"),
    ("menu.about", "Om Bladr…    F1"),
    ("menu.close", "Luk    Esc"),
    ("hud.raw_dims", "RAW · {w} × {h}"),
    ("hud.dims", "{w} × {h}"),
    ("hud.auto", "AUTO"),
    ("hud.zoom_fit", "Tilpas · {p}%"),
    ("hud.zoom_actual", "{p}%"),
    ("overlay.drop", "Slip for at åbne"),
    ("overlay.hint", "Rul for at bladre   ·   F fuld skærm   ·   Esc luk"),
    ("overlay.hint_fullscreen", "Rul for at bladre   ·   F vindue   ·   Esc forlad fuldskærm"),
    ("overlay.loading", "Indlæser…"),
    ("exif.loading", "Læser EXIF…"),
    ("histogram.title", "Histogram"),
    ("exif.none", "Ingen EXIF-data"),
    ("focus.loading", "Læser fokuspunkter…"),
    ("focus.none", "Ingen fokuspunkter i filen"),
    ("scan.network", "Åbner netværksmappe…"),
    ("scan.local", "Åbner mappe…"),
    ("scan.network_hint", "Netværksdrev kan tage flere sekunder, hvis delingen sover."),
    ("scan.cancel", "Annuller"),
    ("scan.interrupted", "Scanningen blev afbrudt."),
    ("path.title", "Åbn sti eller netværksmappe"),
    ("path.help", "Indsæt fx  \\\\server\\share\\mappe  eller en mapped drive som  Y:\\foto"),
    ("path.open", "Åbn"),
    ("path.cancel", "Annuller"),
    ("path.footer", "Enter = åbn   ·   Esc = annuller"),
    ("path.empty_error", "Skriv en sti først."),
    ("dialog.open_folder", "Åbn mappe"),
    ("dialog.open_image", "Åbn billede"),
    ("dialog.images", "Billeder"),
    ("empty.no_images", "Ingen billeder i denne mappe"),
    ("empty.subtitle", "En hurtig billedfremviser"),
    ("empty.open_folder", "Åbn mappe  (O)"),
    ("empty.open_image", "Åbn billede  (Ctrl+O)"),
    ("empty.open_path", "Åbn sti / netværk  (Ctrl+L)"),
    ("empty.hint", "Rul med musen for at bladre\nPiletaster skifter billede   ·   F fuld skærm\n+ - zoom   ·   0 tilpas   ·   1 faktisk størrelse\nMellemrum starter auto-afspilning"),
    ("about.version", "v{version}   ·   {subtitle}"),
    ("about.formats", "JPEG · PNG · WebP · TIFF · RAW (CR3, NEF, ARW, DNG)\nFuldskærm, musens rullehjul og auto-afspilning."),
    ("about.license", "© 2026 · MIT-licens"),
    ("options.title", "Indstillinger"),
    ("options.auto_update", "Søg efter opdateringer automatisk"),
    ("options.start_fullscreen", "Start i fuldskærm"),
    ("options.start_fullscreen_hint", "Slår først til ved næste opstart. Luk Bladr helt med X — Esc forlader kun fuldskærm."),
    ("options.default_viewer", "Standard billedfremviser"),
    ("options.default_yes", "Bladr er standard"),
    ("options.default_no", "Bladr er ikke standard"),
    ("options.default_checking", "Kontrollerer…"),
    ("options.default_choose", "Vælg Bladr som standard…"),
    ("options.default_ask", "Spørg ved opstart, hvis Bladr ikke er standard"),
    ("options.default_hint", "Windows kræver, at du bekræfter valget i Standardapps."),
    ("default.title", "Gør Bladr til standard billedfremviser?"),
    ("default.body", "Bladr er ikke standard til billeder endnu. Vil du åbne Windows' standardapps og vælge Bladr? Windows kræver din bekræftelse."),
    ("default.yes", "Ja, åbn Windows"),
    ("default.no", "Nej, spørg ikke igen"),
    ("default.later", "Senere (7 dage)"),
    ("options.save_failed", "Kunne ikke gemme indstillinger: {e}"),
    ("options.check_now", "Tjek nu"),
    ("options.checking", "Tjekker…"),
    ("options.up_to_date", "Du bruger den nyeste version."),
    ("options.failed", "Kunne ikke tjekke for opdateringer."),
    ("options.version", "Version {version}"),
    ("options.language", "Sprog"),
    ("options.close", "Luk"),
    ("update.title", "Opdatering findes"),
    ("update.body", "Version {new} er tilgængelig. Du har {current}."),
    ("update.download", "Download og installér"),
    ("update.later", "Senere"),
    ("update.downloading", "Downloader…"),
    ("update.failed", "Opdateringen kunne ikke hentes."),
    ("update.installing", "Starter installation…"),
    ("decode.open", "kunne ikke åbne filen: {e}"),
    ("decode.format", "ukendt billedformat: {e}"),
    ("decode.read", "kunne ikke læse billedet: {e}"),
    ("decode.decode", "kunne ikke afkode billedet: {e}"),
    ("decode.read_raw", "kunne ikke læse RAW-filen: {e}"),
    ("decode.no_preview", "Filen indeholder hverken et læsbart RGB-lag eller en JPEG-forhåndsvisning, som Bladr kan vise. DNG'er med CFA-data skal have en indlejret preview."),
    ("decode.preview_fail", "JPEG-forhåndsvisningen i filen kunne ikke afkodes ({n} kandidater). Filen kan bruge en RAW-komprimering, som Bladr ikke understøtter."),
    ("decode.jpeg_xl", "Denne DNG er JPEG XL-komprimeret (DNG 1.7, typisk DxO PureRAW). Bladr kan ikke afkode JPEG XL endnu. Eksportér DNG'en uden komprimering, eller gem den som TIFF/JPEG."),
    ("scan.open_failed", "Kunne ikke åbne \"{path}\":\n{error}"),
    ("scan.denied", "Adgang nægtet. Kontrollér at delingen findes, at du er logget på, og at stien kan åbnes i Stifinder. Prøv eventuelt et mapped drev (fx Y:\\foto) eller åbn stien med Ctrl+L."),
    ("scan.not_found", "Stien findes ikke. Kontrollér delingens navn – fx \\\\server\\share\\mappe."),
    ("scan.aborted", "Netværksforbindelsen blev afbrudt."),
    ("scan.unavailable", "Netværket er ikke tilgængeligt."),
    ("scan.tip", "Tip: en netværkssti kan indtastes direkte med Ctrl+L."),
    ("cli.usage", "Brug: bladr [mappe eller billede] [--fullscreen]"),
    ("cli.not_found", "Findes ikke: {path}"),
    ("cli.title", "Bladr - billedfremviser"),
];

const EN: &[(&str, &str)] = &[
    ("app.subtitle", "fast photo viewer"),
    ("app.formats", "JPEG · PNG · WebP · TIFF · RAW (CR3, NEF, ARW, DNG)\nFull screen, mouse wheel and slideshow."),
    ("app.license", "© 2026 · MIT license"),
    ("app.close", "Close"),
    ("menu.fullscreen", "Full screen        F"),
    ("menu.fit", "Fit to window    0"),
    ("menu.actual", "Actual size    1"),
    ("menu.focus_show", "Show Focus Points    F3"),
    ("menu.focus_hide", "Hide Focus Points    F3"),
    ("menu.histogram_show", "Show Histogram    H"),
    ("menu.histogram_hide", "Hide Histogram    H"),
    ("menu.wallpaper", "Set Wallpaper"),
    ("menu.delete", "Delete image…    Del"),
    ("menu.file_location", "Show file location"),
    ("menu.screenshot", "Take screenshot…    S"),
    ("capture.not_ready", "The image is not ready yet"),
    ("capture.instruction", "Drag to select an area · Esc cancels"),
    ("capture.saved", "Screenshot copied and saved: {path}"),
    ("capture.failed", "Could not save screenshot: {e}"),
    ("file_location.failed", "Could not open Explorer: {e}"),
    ("delete.title", "Delete image?"),
    ("delete.warning", "Are you sure you want to delete “{name}”? The image will be moved to the Recycle Bin. If the Recycle Bin is unavailable, it is deleted permanently."),
    ("delete.warning_network", "Are you sure you want to delete “{name}”? On a network drive the image is deleted permanently because there is no Recycle Bin."),
    ("delete.cancel", "Cancel"),
    ("delete.confirm", "Delete"),
    ("delete.busy", "Deleting…"),
    ("delete.failed", "Could not delete the image: {e}"),
    ("wallpaper.centered", "Centered"),
    ("wallpaper.tiled", "Tiled"),
    ("wallpaper.stretched", "Stretched"),
    ("wallpaper.restore", "Restore"),
    ("slideshow.delay", "Delay"),
    ("slideshow.delay_hint", "Seconds per image"),
    ("slideshow.prev", "Previous"),
    ("slideshow.pause", "Pause"),
    ("slideshow.play", "Play"),
    ("slideshow.next", "Next"),
    ("slideshow.loop", "Loop"),
    ("slideshow.shuffle", "Shuffle"),
    ("slideshow.exit", "Exit"),
    ("slideshow.error", "Slideshow error"),
    ("menu.open_folder", "Open folder…        O"),
    ("menu.open_file", "Open image…    Ctrl+O"),
    ("menu.open_path", "Open path…        Ctrl+L"),
    ("menu.language", "Language (Alt+L)"),
    ("menu.options", "Options…    F2"),
    ("menu.about", "About Bladr…    F1"),
    ("menu.close", "Close    Esc"),
    ("hud.raw_dims", "RAW · {w} × {h}"),
    ("hud.dims", "{w} × {h}"),
    ("hud.auto", "AUTO"),
    ("hud.zoom_fit", "Fit · {p}%"),
    ("hud.zoom_actual", "{p}%"),
    ("overlay.drop", "Drop to open"),
    ("overlay.hint", "Scroll to browse   ·   F full screen   ·   Esc close"),
    ("overlay.hint_fullscreen", "Scroll to browse   ·   F windowed   ·   Esc exit full screen"),
    ("overlay.loading", "Loading…"),
    ("exif.loading", "Reading EXIF…"),
    ("histogram.title", "Histogram"),
    ("exif.none", "No EXIF data"),
    ("focus.loading", "Reading focus points…"),
    ("focus.none", "No focus point data in this file"),
    ("scan.network", "Opening network folder…"),
    ("scan.local", "Opening folder…"),
    ("scan.network_hint", "A network drive can take a few seconds if the share is asleep."),
    ("scan.cancel", "Cancel"),
    ("scan.interrupted", "The scan was interrupted."),
    ("path.title", "Open path or network folder"),
    ("path.help", "Paste e.g.  \\\\server\\share\\folder  or a mapped drive like  Y:\\photos"),
    ("path.open", "Open"),
    ("path.cancel", "Cancel"),
    ("path.footer", "Enter = open   ·   Esc = cancel"),
    ("path.empty_error", "Enter a path first."),
    ("dialog.open_folder", "Open folder"),
    ("dialog.open_image", "Open image"),
    ("dialog.images", "Images"),
    ("empty.no_images", "No images in this folder"),
    ("empty.subtitle", "A fast photo viewer"),
    ("empty.open_folder", "Open folder  (O)"),
    ("empty.open_image", "Open image  (Ctrl+O)"),
    ("empty.open_path", "Open path / network  (Ctrl+L)"),
    ("empty.hint", "Scroll with the mouse to browse\nArrow keys change image   ·   F full screen\n+ - zoom   ·   0 fit   ·   1 actual size\nSpace starts the slideshow"),
    ("about.version", "v{version}   ·   {subtitle}"),
    ("about.formats", "JPEG · PNG · WebP · TIFF · RAW (CR3, NEF, ARW, DNG)\nFull screen, mouse wheel and slideshow."),
    ("about.license", "© 2026 · MIT license"),
    ("options.title", "Options"),
    ("options.auto_update", "Check for updates automatically"),
    ("options.start_fullscreen", "Start in full screen"),
    ("options.start_fullscreen_hint", "Takes effect the next time Bladr starts. Close Bladr with X — Esc only leaves full screen."),
    ("options.default_viewer", "Default photo viewer"),
    ("options.default_yes", "Bladr is the default"),
    ("options.default_no", "Bladr is not the default"),
    ("options.default_checking", "Checking…"),
    ("options.default_choose", "Choose Bladr as default…"),
    ("options.default_ask", "Ask at startup when Bladr is not the default"),
    ("options.default_hint", "Windows requires you to confirm the choice in Default apps."),
    ("default.title", "Make Bladr the default photo viewer?"),
    ("default.body", "Bladr is not the default image app yet. Open Windows Default apps and choose Bladr? Windows requires your confirmation."),
    ("default.yes", "Yes, open Windows"),
    ("default.no", "No, don't ask again"),
    ("default.later", "Later (7 days)"),
    ("options.save_failed", "Could not save settings: {e}"),
    ("options.check_now", "Check now"),
    ("options.checking", "Checking…"),
    ("options.up_to_date", "You are using the latest version."),
    ("options.failed", "Could not check for updates."),
    ("options.version", "Version {version}"),
    ("options.language", "Language"),
    ("options.close", "Close"),
    ("update.title", "Update available"),
    ("update.body", "Version {new} is available. You have {current}."),
    ("update.download", "Download and install"),
    ("update.later", "Later"),
    ("update.downloading", "Downloading…"),
    ("update.failed", "The update could not be downloaded."),
    ("update.installing", "Starting installer…"),
    ("decode.open", "could not open the file: {e}"),
    ("decode.format", "unknown image format: {e}"),
    ("decode.read", "could not read the image: {e}"),
    ("decode.decode", "could not decode the image: {e}"),
    ("decode.read_raw", "could not read the RAW file: {e}"),
    ("decode.no_preview", "The file has neither a readable RGB layer nor a JPEG preview that Bladr can show. DNG files with CFA data need an embedded preview."),
    ("decode.preview_fail", "The JPEG preview in the file could not be decoded ({n} candidates). The file may use RAW compression that Bladr does not support."),
    ("decode.jpeg_xl", "This DNG uses JPEG XL compression (DNG 1.7, typically DxO PureRAW). Bladr cannot decode JPEG XL yet. Export the DNG without compression, or save it as TIFF/JPEG."),
    ("scan.open_failed", "Could not open \"{path}\":\n{error}"),
    ("scan.denied", "Access denied. Check that the share exists, that you are signed in, and that the path opens in Explorer. Try a mapped drive (e.g. Y:\\photos) or open the path with Ctrl+L."),
    ("scan.not_found", "The path does not exist. Check the share name – e.g. \\\\server\\share\\folder."),
    ("scan.aborted", "The network connection was interrupted."),
    ("scan.unavailable", "The network is not available."),
    ("scan.tip", "Tip: a network path can be entered directly with Ctrl+L."),
    ("cli.usage", "Usage: bladr [folder or image] [--fullscreen]"),
    ("cli.not_found", "Not found: {path}"),
    ("cli.title", "Bladr - photo viewer"),
];

pub fn tr(lang: Lang, key: &'static str) -> &'static str {
    let table = match lang {
        Lang::Da => DA,
        Lang::En => EN,
    };
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
        .or_else(|| {
            EN.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| *value)
        })
        .unwrap_or(key)
}

/// Format a translated template with `{name}` placeholders.
pub fn tf(lang: Lang, key: &'static str, args: &[(&str, &str)]) -> String {
    let mut text = tr(lang, key).to_owned();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}
