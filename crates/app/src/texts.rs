//! All user-facing text, in French and English.
//!
//! The current language is global (set from the settings at startup and when
//! the user changes it); every text is a function returning the right
//! translation, so a language change applies on the next frame.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use phonycut_core::api::{AdbProblem, DeviceInfo, DeviceState, ExportProblem, RecordingProblem};
use serde::{Deserialize, Serialize};

/// Languages of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    French,
    English,
}

static LANGUAGE: AtomicU8 = AtomicU8::new(0);

pub fn set_language(language: Language) {
    LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn language() -> Language {
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => Language::English,
        _ => Language::French,
    }
}

/// Picks the translation for the current language.
fn tr(fr: &'static str, en: &'static str) -> &'static str {
    match language() {
        Language::French => fr,
        Language::English => en,
    }
}

/// Main message (title) and what the user should do (hint).
pub struct Message {
    pub title: String,
    pub hint: &'static str,
}

impl Message {
    fn new(title: impl Into<String>, hint: &'static str) -> Self {
        Self {
            title: title.into(),
            hint,
        }
    }
}

// --- Connection ------------------------------------------------------------

pub fn starting() -> &'static str {
    tr("Préparation de la connexion…", "Preparing the connection…")
}
pub fn starting_hint() -> &'static str {
    tr("Patientez quelques secondes.", "Please wait a few seconds.")
}
pub fn no_device() -> &'static str {
    tr("Aucun téléphone détecté", "No phone detected")
}
pub fn no_device_hint() -> &'static str {
    tr(
        "Branchez votre téléphone avec un câble USB et vérifiez que le débogage USB est \
         activé. Si rien ne se passe, essayez un autre câble ou un autre port USB.",
        "Plug your phone in with a USB cable and check that USB debugging is enabled. If \
         nothing happens, try another cable or another USB port.",
    )
}
pub fn choose_device() -> &'static str {
    tr("Plusieurs téléphones détectés", "Several phones detected")
}
pub fn choose_device_hint() -> &'static str {
    tr(
        "Choisissez le téléphone à utiliser dans la liste ci-dessous.",
        "Choose the phone to use in the list below.",
    )
}
pub fn adb_conflict() -> &'static str {
    tr(
        "Un autre logiciel Android est ouvert sur cet ordinateur (Android Studio, outil de \
         transfert…). Fermez-le pour éviter les coupures de connexion.",
        "Another Android program is open on this computer (Android Studio, a transfer \
         tool…). Close it to avoid connection drops.",
    )
}
pub fn device_list() -> &'static str {
    tr("Téléphones branchés :", "Connected phones:")
}
pub fn connection_help() -> &'static str {
    tr("Aide à la connexion", "Connection help")
}

pub fn adb_problem(problem: AdbProblem) -> Message {
    match problem {
        AdbProblem::AdbMissing => Message::new(
            tr(
                "Un fichier de PhonyCut est manquant",
                "A PhonyCut file is missing",
            ),
            tr(
                "Réinstallez PhonyCut pour réparer l'installation.",
                "Reinstall PhonyCut to repair it.",
            ),
        ),
        AdbProblem::ServerFailed => Message::new(
            tr(
                "Impossible de démarrer la connexion au téléphone",
                "Cannot start the connection to the phone",
            ),
            tr(
                "Fermez les autres logiciels Android ouverts (Android Studio, outils de \
                 transfert ou de flashage…). PhonyCut réessaie automatiquement.",
                "Close other open Android programs (Android Studio, transfer or flashing \
                 tools…). PhonyCut keeps retrying automatically.",
            ),
        ),
    }
}

pub fn device(device: &DeviceInfo) -> Message {
    match &device.state {
        DeviceState::Ready => Message::new(
            match language() {
                Language::French => format!("Téléphone détecté : {}", device.display_name()),
                Language::English => format!("Phone detected: {}", device.display_name()),
            },
            tr("Tout est prêt.", "Everything is ready."),
        ),
        DeviceState::Unauthorized => Message::new(
            tr(
                "Autorisation nécessaire sur le téléphone",
                "Permission needed on the phone",
            ),
            tr(
                "Déverrouillez le téléphone et appuyez sur « Autoriser » dans la fenêtre \
                 « Autoriser le débogage USB ? ». Cochez « Toujours autoriser sur cet \
                 ordinateur ». Si la fenêtre n'apparaît pas, débranchez puis rebranchez le câble.",
                "Unlock the phone and tap \"Allow\" in the \"Allow USB debugging?\" window. \
                 Tick \"Always allow from this computer\". If the window does not appear, \
                 unplug and plug the cable back in.",
            ),
        ),
        DeviceState::Authorizing => Message::new(
            tr("Autorisation en cours…", "Authorizing…"),
            starting_hint(),
        ),
        DeviceState::Offline => Message::new(
            tr("Le téléphone ne répond pas", "The phone is not responding"),
            tr(
                "Débranchez puis rebranchez le câble USB. Si le problème continue, \
                 redémarrez le téléphone.",
                "Unplug and plug the USB cable back in. If the problem continues, restart \
                 the phone.",
            ),
        ),
        DeviceState::Connecting => Message::new(
            tr("Connexion au téléphone…", "Connecting to the phone…"),
            starting_hint(),
        ),
        DeviceState::NoPermissions => Message::new(
            tr("Accès au téléphone refusé", "Access to the phone denied"),
            tr(
                "Débranchez puis rebranchez le câble, puis acceptez l'autorisation sur le \
                 téléphone.",
                "Unplug and plug the cable back in, then accept the permission on the phone.",
            ),
        ),
        DeviceState::Other(_) => Message::new(
            tr(
                "Le téléphone est dans un mode inhabituel",
                "The phone is in an unusual mode",
            ),
            tr(
                "Redémarrez le téléphone normalement, puis rebranchez-le.",
                "Restart the phone normally, then plug it back in.",
            ),
        ),
    }
}

/// Short state label used in the device list.
pub fn device_state_short(state: &DeviceState) -> &'static str {
    match state {
        DeviceState::Ready => tr("prêt", "ready"),
        DeviceState::Unauthorized => tr("autorisation nécessaire", "permission needed"),
        DeviceState::Authorizing => tr("autorisation en cours", "authorizing"),
        DeviceState::Offline => tr("ne répond pas", "not responding"),
        DeviceState::Connecting => tr("connexion…", "connecting…"),
        DeviceState::NoPermissions => tr("accès refusé", "access denied"),
        DeviceState::Other(_) => tr("mode inhabituel", "unusual mode"),
    }
}

// --- Live preview ------------------------------------------------------------

pub fn mirror_starting() -> &'static str {
    tr("Lancement de l'aperçu…", "Starting the preview…")
}
pub fn mirror_interrupted() -> &'static str {
    tr("L'aperçu s'est interrompu", "The preview stopped")
}
pub fn mirror_interrupted_hint() -> &'static str {
    tr(
        "Appuyez sur « Relancer l'aperçu ». Si le problème continue, débranchez puis \
         rebranchez le câble USB.",
        "Click \"Restart preview\". If the problem continues, unplug and plug the USB cable \
         back in.",
    )
}
pub fn mirror_retry() -> &'static str {
    tr("Relancer l'aperçu", "Restart preview")
}
pub fn live_badge() -> &'static str {
    tr("Aperçu en direct", "Live preview")
}

/// Short status of the phone's sound, above the preview.
pub fn sound_status(state: Option<phonycut_core::api::SoundState>, capture: bool) -> &'static str {
    use phonycut_core::api::SoundState;
    match (capture, state) {
        (false, _) => tr(
            "Son : désactivé (Réglages)",
            "Sound: off (Settings)",
        ),
        (true, None) => "",
        (true, Some(SoundState::Captured { on_pc: true })) => tr("Son : sur le PC", "Sound: on the PC"),
        (true, Some(SoundState::Captured { on_pc: false })) => tr(
            "Son : enregistré, écoute coupée",
            "Sound: recorded, not played",
        ),
        (true, Some(SoundState::NoOutput)) => tr(
            "Son : enregistré, mais ce PC n'a pas de sortie audio (branchez un casque ou des \
             haut-parleurs)",
            "Sound: recorded, but this PC has no sound output (plug in headphones or speakers)",
        ),
        (true, Some(SoundState::NeedsAndroid11)) => tr(
            "Son : non disponible, il faut Android 11 ou plus",
            "Sound: not available, Android 11 or later is needed",
        ),
        (true, Some(SoundState::Unavailable)) => tr(
            "Son : le téléphone ne le partage pas. Déverrouillez-le, puis débranchez et \
             rebranchez le câble.",
            "Sound: the phone does not share it. Unlock it, then unplug and plug the cable back in.",
        ),
    }
}

// --- Phone control (mouse and keyboard) ----------------------------------------

pub fn phone_back() -> &'static str {
    tr("Retour", "Back")
}
pub fn phone_home() -> &'static str {
    tr("Accueil", "Home")
}
pub fn phone_recents() -> &'static str {
    tr("Applis récentes", "Recent apps")
}
pub fn phone_volume_down() -> &'static str {
    "Volume −"
}
pub fn phone_volume_up() -> &'static str {
    "Volume +"
}
pub fn control_hint() -> &'static str {
    tr(
        "Cliquez sur l'écran pour utiliser le téléphone. Clic droit : retour, molette : défiler.",
        "Click the screen to use the phone. Right click: back, wheel: scroll.",
    )
}
pub fn keyboard_active() -> &'static str {
    tr(
        "⌨ Le clavier écrit sur le téléphone (Ctrl+V colle le texte du PC). Cliquez à côté \
         de l'écran pour arrêter.",
        "⌨ The keyboard types on the phone (Ctrl+V pastes the PC text). Click outside the \
         screen to stop.",
    )
}

// --- Tabs and main buttons ------------------------------------------------------

pub fn tab_live() -> &'static str {
    tr("Direct", "Live")
}
pub fn tab_clips() -> &'static str {
    tr("Mes clips", "My clips")
}
pub fn tab_settings() -> &'static str {
    tr("Réglages", "Settings")
}
pub fn tab_recording() -> &'static str {
    "  ● REC"
}
pub fn open_clips() -> &'static str {
    tab_clips()
}
pub fn already_running() -> &'static str {
    tr(
        "PhonyCut est déjà ouvert. Retrouvez sa fenêtre dans la barre des tâches.",
        "PhonyCut is already open. Find its window in the taskbar.",
    )
}

// --- Recording -------------------------------------------------------------------

pub fn record_start() -> &'static str {
    tr("●  Enregistrer", "●  Record")
}
pub fn record_stop() -> &'static str {
    tr("Arrêter l'enregistrement", "Stop recording")
}
pub fn record_waiting() -> &'static str {
    tr("Démarrage de l'enregistrement…", "Starting the recording…")
}
/// The phone rotated during the recording: the video is re-encoded so every
/// player reads it.
pub fn record_finalizing(percent: u8) -> String {
    match language() {
        Language::French => {
            format!("Finalisation de l'enregistrement (le téléphone a tourné) : {percent} %…")
        }
        Language::English => {
            format!("Finishing the recording (the phone was rotated): {percent}%…")
        }
    }
}
pub fn record_waiting_asleep() -> &'static str {
    tr(
        "Le téléphone est peut-être en veille : rallumez son écran (bouton marche, ou clic \
         droit sur l'aperçu). L'enregistrement démarre dès qu'une image arrive.",
        "The phone may be asleep: turn its screen back on (power button, or right-click the \
         preview). The recording starts as soon as an image arrives.",
    )
}
pub fn record_live() -> &'static str {
    "● REC"
}
pub fn mark_cut() -> &'static str {
    tr("Marquer un cut  (M)", "Mark a cut  (M)")
}
pub fn mark_cut_hint() -> &'static str {
    tr(
        "Pose un repère à cet instant. Vous pourrez ensuite découper la vidéo à ces repères \
         dans « Mes clips ». Raccourci : touche M.",
        "Puts a marker at this moment. You can then cut the video at these markers in \"My \
         clips\". Shortcut: M key.",
    )
}
pub fn record_saved() -> &'static str {
    tr("Vidéo enregistrée :", "Video saved:")
}
pub fn record_saved_after_unplug() -> &'static str {
    tr(
        "Le téléphone s'est déconnecté : l'enregistrement s'est arrêté, mais la vidéo a bien \
         été sauvegardée.",
        "The phone was disconnected: the recording stopped, but the video was saved.",
    )
}
pub fn edit_video() -> &'static str {
    tr("Découper cette vidéo", "Cut this video")
}
pub fn open_folder() -> &'static str {
    tr("Ouvrir le dossier", "Open folder")
}
pub fn folder_label() -> &'static str {
    tr("Dossier des vidéos :", "Videos folder:")
}
pub fn folder_change() -> &'static str {
    tr("Modifier…", "Change…")
}
pub fn pick_folder_title() -> &'static str {
    tr("Choisir le dossier des vidéos", "Choose the videos folder")
}
pub fn folder_invalid() -> &'static str {
    tr(
        "Impossible d'utiliser ce dossier. Vérifiez le chemin ou choisissez un autre dossier \
         (par exemple dans « Vidéos »).",
        "This folder cannot be used. Check the path or choose another folder (for example \
         in \"Videos\").",
    )
}
pub fn open_folder_failed() -> &'static str {
    tr("Impossible d'ouvrir le dossier.", "Cannot open the folder.")
}

pub fn recording_problem(problem: RecordingProblem) -> &'static str {
    match problem {
        RecordingProblem::NotMirroring => tr(
            "L'enregistrement n'a pas pu démarrer : l'aperçu n'est pas actif. Attendez que \
             l'écran du téléphone s'affiche, puis réessayez.",
            "The recording could not start: the preview is not running. Wait until the phone \
             screen is shown, then try again.",
        ),
        RecordingProblem::AlreadyRecording => tr(
            "Un enregistrement est déjà en cours.",
            "A recording is already running.",
        ),
        RecordingProblem::CannotCreateFile => tr(
            "Impossible de créer la vidéo dans le dossier choisi. Choisissez un autre dossier \
             avec « Modifier… », puis réessayez.",
            "Cannot create the video in the chosen folder. Choose another folder with \
             \"Change…\", then try again.",
        ),
        RecordingProblem::WriteFailed => tr(
            "L'enregistrement s'est arrêté : impossible d'écrire sur le disque (disque plein ou \
             retiré ?). Libérez de la place, puis relancez l'enregistrement.",
            "The recording stopped: cannot write to the disk (full or removed?). Free some \
             space, then start the recording again.",
        ),
        RecordingProblem::NoVideo => tr(
            "L'enregistrement a été arrêté avant de commencer : aucune vidéo n'a été créée.",
            "The recording was stopped before it started: no video was created.",
        ),
    }
}

/// `3 cuts · dernier à 01:23` / `3 cuts · last at 01:23`
pub fn cuts_summary(count: usize, last: Duration) -> String {
    let noun = if count > 1 { "cuts" } else { "cut" };
    match language() {
        Language::French => format!("{count} {noun}  ·  dernier à {}", duration(last)),
        Language::English => format!("{count} {noun}  ·  last at {}", duration(last)),
    }
}

// --- My clips --------------------------------------------------------------------

pub fn clips_list_title() -> &'static str {
    tr("Vos vidéos", "Your videos")
}
pub fn clips_refresh() -> &'static str {
    tr("Actualiser", "Refresh")
}
pub fn clips_empty() -> &'static str {
    tr(
        "Aucune vidéo pour l'instant. Enregistrez l'écran de votre téléphone depuis l'onglet \
         « Direct ».",
        "No videos yet. Record your phone screen from the \"Live\" tab.",
    )
}
pub fn clips_pick() -> &'static str {
    tr(
        "Choisissez une vidéo dans la liste pour la découper en clips.",
        "Choose a video in the list to cut it into clips.",
    )
}
pub fn clips_loading() -> &'static str {
    tr("Ouverture de la vidéo…", "Opening the video…")
}
pub fn player_failed() -> &'static str {
    tr(
        "Impossible d'ouvrir cette vidéo. Elle a peut-être été déplacée, supprimée ou \
         endommagée.",
        "Cannot open this video. It may have been moved, deleted or damaged.",
    )
}
pub fn cuts_save_failed() -> &'static str {
    tr(
        "Impossible d'enregistrer vos modifications à côté de la vidéo. Vérifiez que le \
         dossier n'est pas en lecture seule.",
        "Cannot save your changes next to the video. Check that the folder is not read-only.",
    )
}
pub fn play() -> &'static str {
    tr("Lecture", "Play")
}
pub fn pause() -> &'static str {
    "Pause"
}
pub fn play_hint() -> &'static str {
    tr("Raccourci : barre d'espace.", "Shortcut: space bar.")
}
pub fn split_here() -> &'static str {
    tr("Couper ici", "Split here")
}
pub fn split_hint() -> &'static str {
    tr(
        "Coupe le clip sélectionné en deux à la position de lecture.",
        "Splits the selected clip in two at the playback position.",
    )
}
pub fn reset_segments() -> &'static str {
    tr("Revenir aux cuts d'origine", "Back to the original cuts")
}
pub fn segments_title() -> &'static str {
    "Clips"
}
pub fn segments_hint() -> &'static str {
    tr(
        "Cochez les clips à exporter. Faites glisser les poignées jaunes sur la barre pour \
         ajuster le début et la fin du clip sélectionné.",
        "Tick the clips to export. Drag the yellow handles on the bar to adjust the start and \
         end of the selected clip.",
    )
}
/// Name of the second half of a split clip: `Intro (suite)` / `Intro (cont.)`.
pub fn continued(name: &str) -> String {
    match language() {
        Language::French => format!("{name} (suite)"),
        Language::English => format!("{name} (cont.)"),
    }
}
pub fn segment_go() -> &'static str {
    tr("Voir", "View")
}
pub fn segment_set_start() -> &'static str {
    tr("Début ici", "Start here")
}
pub fn segment_set_end() -> &'static str {
    tr("Fin ici", "End here")
}
pub fn export_mode() -> &'static str {
    tr("Export :", "Export:")
}
pub fn export_fast() -> &'static str {
    tr("Rapide", "Fast")
}
pub fn export_fast_hint() -> &'static str {
    tr(
        "Instantané et sans perte de qualité. Le clip peut commencer jusqu'à 1 seconde avant \
         le début choisi.",
        "Instant and lossless. The clip may start up to 1 second before the chosen start.",
    )
}
pub fn export_precise() -> &'static str {
    tr("Précis", "Precise")
}
pub fn export_precise_hint() -> &'static str {
    tr(
        "Commence exactement au début choisi. Plus lent : la vidéo est recompressée.",
        "Starts exactly at the chosen start. Slower: the video is re-encoded.",
    )
}
pub fn export_running() -> &'static str {
    tr("Export en cours…", "Exporting…")
}
pub fn export_cancel() -> &'static str {
    tr("Annuler", "Cancel")
}
pub fn export_cancelled() -> &'static str {
    tr("Export annulé.", "Export cancelled.")
}

pub fn export_button(count: usize) -> String {
    match (language(), count) {
        (Language::French, 0) => "Aucun clip coché".to_owned(),
        (Language::French, 1) => "Exporter 1 clip".to_owned(),
        (Language::French, n) => format!("Exporter {n} clips"),
        (Language::English, 0) => "No clip ticked".to_owned(),
        (Language::English, 1) => "Export 1 clip".to_owned(),
        (Language::English, n) => format!("Export {n} clips"),
    }
}

/// Website page with the ways to support the project, in the UI language.
pub fn support_url() -> &'static str {
    tr(
        "https://nico7522.github.io/phony-cut/fr/support/",
        "https://nico7522.github.io/phony-cut/support/",
    )
}
pub fn support_project() -> &'static str {
    tr("Soutenir le projet", "Support the project")
}
pub fn support_after_export() -> &'static str {
    tr(
        "PhonyCut est gratuit. S'il vous est utile, vous pouvez soutenir le projet :",
        "PhonyCut is free. If it is useful to you, you can support the project:",
    )
}

pub fn export_done(count: usize) -> String {
    match (language(), count) {
        (Language::French, 1) => "1 clip exporté en MP4.".to_owned(),
        (Language::French, n) => format!("{n} clips exportés en MP4."),
        (Language::English, 1) => "1 clip exported as MP4.".to_owned(),
        (Language::English, n) => format!("{n} clips exported as MP4."),
    }
}

pub fn export_problem(problem: ExportProblem) -> &'static str {
    match problem {
        ExportProblem::NothingToExport => tr(
            "Cochez au moins un clip à exporter.",
            "Tick at least one clip to export.",
        ),
        ExportProblem::CannotRead => tr(
            "Impossible de lire la vidéo. Elle a peut-être été déplacée ou supprimée.",
            "Cannot read the video. It may have been moved or deleted.",
        ),
        ExportProblem::CannotWrite => tr(
            "Impossible d'écrire les clips (disque plein ou dossier protégé ?). Libérez de la \
             place ou changez le dossier des vidéos, puis réessayez.",
            "Cannot write the clips (disk full or protected folder?). Free some space or \
             change the videos folder, then try again.",
        ),
        ExportProblem::NoEncoder => tr(
            "L'export précis n'est pas disponible sur cet ordinateur. Utilisez l'export rapide.",
            "Precise export is not available on this computer. Use fast export.",
        ),
        ExportProblem::Busy => tr(
            "Un export est déjà en cours. Attendez qu'il se termine.",
            "An export is already running. Wait until it finishes.",
        ),
    }
}

/// `01:02 → 01:30  (28,0 s)` / `(28.0 s)`
pub fn segment_range(start_ms: u64, end_ms: u64) -> String {
    let len = end_ms.saturating_sub(start_ms);
    let length = if len < 60_000 {
        decimal(format!("{:.1} s", len as f64 / 1000.0))
    } else {
        duration(Duration::from_millis(len))
    };
    format!(
        "{} → {}  ({length})",
        duration(Duration::from_millis(start_ms)),
        duration(Duration::from_millis(end_ms))
    )
}

/// `10,1 Mo · 3 cuts` / `10.1 MB · 3 cuts`
pub fn recording_details(bytes: u64, cuts: usize) -> String {
    match cuts {
        0 => file_size(bytes),
        1 => format!("{}  ·  1 cut", file_size(bytes)),
        n => format!("{}  ·  {n} cuts", file_size(bytes)),
    }
}

// --- Formatting --------------------------------------------------------------------

/// `12:34` or `1:02:03`.
pub fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// `850 Ko`, `12,4 Mo`, `1,25 Go` / `850 KB`, `12.4 MB`, `1.25 GB`.
pub fn file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    let (k, m, g) = match language() {
        Language::French => ("Ko", "Mo", "Go"),
        Language::English => ("KB", "MB", "GB"),
    };
    let text = if b < KB * KB {
        format!("{:.0} {k}", (b / KB).max(if bytes > 0 { 1.0 } else { 0.0 }))
    } else if b < KB * KB * KB {
        format!("{:.1} {m}", b / (KB * KB))
    } else {
        format!("{:.2} {g}", b / (KB * KB * KB))
    };
    decimal(text)
}

/// Decimal comma in French.
fn decimal(text: String) -> String {
    match language() {
        Language::French => text.replace('.', ","),
        Language::English => text,
    }
}

// --- Connection assistant ------------------------------------------------------------

/// Texts of the first-connection assistant.
pub mod onboarding {
    use super::{language, tr, Language};
    use crate::screens::onboarding::{Brand, Step};

    pub fn title() -> &'static str {
        tr("Connecter votre téléphone", "Connect your phone")
    }
    pub fn step_short() -> [&'static str; 4] {
        match language() {
            Language::French => ["Mode développeur", "Débogage USB", "Câble", "Autorisation"],
            Language::English => ["Developer mode", "USB debugging", "Cable", "Permission"],
        }
    }
    pub fn varies_notice() -> &'static str {
        tr(
            "Les menus varient selon la marque du téléphone, mais restent proches. Les images \
             ci-dessous montrent un téléphone Xiaomi. Si vous ne trouvez pas une option, \
             cherchez sur internet « activer le débogage USB » suivi du modèle de votre \
             téléphone.",
            "Menus vary by phone brand but stay similar. The pictures below show a Xiaomi \
             phone set to French. If you cannot find an option, search the web for \"enable \
             USB debugging\" followed by your phone model.",
        )
    }
    pub fn brand_label() -> &'static str {
        tr("Marque de votre téléphone :", "Your phone brand:")
    }
    pub fn waiting_phone() -> &'static str {
        tr("En attente du téléphone…", "Waiting for the phone…")
    }
    pub fn waiting_authorization() -> &'static str {
        tr(
            "En attente de votre autorisation sur le téléphone…",
            "Waiting for your permission on the phone…",
        )
    }
    pub fn help_title() -> &'static str {
        tr("Ça ne marche pas ?", "Not working?")
    }
    pub fn previous() -> &'static str {
        tr("Précédent", "Back")
    }
    pub fn next() -> &'static str {
        tr("Suivant", "Next")
    }
    pub fn skip() -> &'static str {
        tr("Passer l'assistant", "Skip the assistant")
    }
    pub fn skip_hint() -> &'static str {
        tr(
            "Vous pourrez le rouvrir à tout moment depuis les Réglages.",
            "You can reopen it at any time from the Settings.",
        )
    }
    pub fn done_title() -> &'static str {
        tr("C'est prêt !", "All set!")
    }
    pub fn done_body() -> &'static str {
        tr(
            "Votre téléphone est connecté. Vous pouvez maintenant voir son écran, l'enregistrer \
             et créer des clips. Cet assistant reste disponible dans les Réglages.",
            "Your phone is connected. You can now see its screen, record it and make clips. \
             This assistant stays available in the Settings.",
        )
    }
    pub fn start() -> &'static str {
        tr("Commencer", "Get started")
    }

    pub fn step(step: Step) -> (&'static str, &'static str) {
        match step {
            Step::DeveloperOptions => (
                tr(
                    "1. Activer les options pour les développeurs",
                    "1. Turn on developer options",
                ),
                tr(
                    "Sur le téléphone, ouvrez les Paramètres, puis « À propos du téléphone ». \
                     Touchez plusieurs fois de suite la ligne « Numéro de build » (son nom \
                     change selon la marque, voir ci-dessous) pour débloquer les options pour \
                     les développeurs. Appuyez ensuite sur « Suivant ».",
                    "On the phone, open Settings, then \"About phone\". Tap the \"Build number\" \
                     line several times in a row (its name depends on the brand, see below) to \
                     unlock the developer options. Then click \"Next\".",
                ),
            ),
            Step::UsbDebugging => (
                tr("2. Activer le débogage USB", "2. Turn on USB debugging"),
                tr(
                    "Ouvrez « Options pour les développeurs » et activez « Débogage USB ». \
                     Confirmez dans la fenêtre d'avertissement qui s'affiche, puis appuyez sur \
                     « Suivant ». Seul « Débogage USB » est nécessaire : les autres options \
                     peuvent rester désactivées.",
                    "Open \"Developer options\" and turn on \"USB debugging\". Confirm in the \
                     warning window that appears, then click \"Next\". Only \"USB debugging\" \
                     is needed: the other options can stay off.",
                ),
            ),
            Step::Plug => (
                tr("3. Brancher le téléphone", "3. Plug in the phone"),
                tr(
                    "Branchez le téléphone à l'ordinateur avec un câble USB, de préférence sur \
                     un port à l'arrière du PC. Attention : certains câbles ne servent qu'à \
                     charger et ne transmettent pas de données. Si une notification USB \
                     apparaît sur le téléphone, choisissez « Transfert de fichiers ». PhonyCut \
                     détecte le téléphone tout seul.",
                    "Connect the phone to the computer with a USB cable, preferably on a port at \
                     the back of the PC. Careful: some cables only charge and do not carry \
                     data. If a USB notification appears on the phone, choose \"File \
                     transfer\". PhonyCut detects the phone by itself.",
                ),
            ),
            Step::Authorize => (
                tr("4. Autoriser cet ordinateur", "4. Allow this computer"),
                tr(
                    "Déverrouillez le téléphone : une fenêtre « Autoriser le débogage USB ? » \
                     s'affiche. Cochez « Toujours autoriser sur cet ordinateur », puis appuyez \
                     sur « Autoriser ».",
                    "Unlock the phone: an \"Allow USB debugging?\" window appears. Tick \"Always \
                     allow from this computer\", then tap \"Allow\".",
                ),
            ),
            Step::Done => ("", ""),
        }
    }

    pub fn brand_name(brand: Brand) -> &'static str {
        match brand {
            Brand::Xiaomi => "Xiaomi, Redmi, POCO",
            Brand::Samsung => "Samsung",
            Brand::Pixel => "Google Pixel",
            Brand::OppoOnePlus => "Oppo, OnePlus, realme",
            Brand::Other => tr("Autre marque", "Other brand"),
        }
    }

    /// Menu path for enabling developer options (`developer`) or USB debugging.
    pub fn brand_path(brand: Brand, developer: bool) -> &'static str {
        match (brand, developer) {
            (Brand::Xiaomi, true) => tr(
                "Paramètres → À propos du téléphone → touchez plusieurs fois « Version de \
                 l'OS » (ou « Version de MIUI » sur les modèles plus anciens).",
                "Settings → About phone → tap \"OS version\" several times (or \"MIUI \
                 version\" on older models).",
            ),
            (Brand::Xiaomi, false) => tr(
                "Paramètres → Paramètres supplémentaires → Options pour les développeurs → \
                 Débogage USB.",
                "Settings → Additional settings → Developer options → USB debugging.",
            ),
            (Brand::Samsung, true) => tr(
                "Paramètres → À propos du téléphone → Informations sur le logiciel → touchez \
                 plusieurs fois « Numéro de version ».",
                "Settings → About phone → Software information → tap \"Build number\" several \
                 times.",
            ),
            (Brand::Samsung, false) => tr(
                "Paramètres → Options de développement (tout en bas de la liste) → Débogage USB.",
                "Settings → Developer options (at the bottom of the list) → USB debugging.",
            ),
            (Brand::Pixel, true) => tr(
                "Paramètres → À propos du téléphone → touchez plusieurs fois « Numéro de build ».",
                "Settings → About phone → tap \"Build number\" several times.",
            ),
            (Brand::Pixel, false) => tr(
                "Paramètres → Système → Options pour les développeurs → Débogage USB.",
                "Settings → System → Developer options → USB debugging.",
            ),
            (Brand::OppoOnePlus, true) => tr(
                "Paramètres → À propos de l'appareil → Version → touchez plusieurs fois « Numéro \
                 de build ».",
                "Settings → About device → Version → tap \"Build number\" several times.",
            ),
            (Brand::OppoOnePlus, false) => tr(
                "Paramètres → Paramètres supplémentaires (ou Système) → Options pour les \
                 développeurs → Débogage USB.",
                "Settings → Additional settings (or System) → Developer options → USB \
                 debugging.",
            ),
            (Brand::Other, true) => tr(
                "En général : Paramètres → À propos du téléphone → touchez plusieurs fois \
                 « Numéro de build » (parfois rangé dans « Informations sur le logiciel »).",
                "Usually: Settings → About phone → tap \"Build number\" several times \
                 (sometimes under \"Software information\").",
            ),
            (Brand::Other, false) => tr(
                "En général : Paramètres → Système → Options pour les développeurs → Débogage \
                 USB.",
                "Usually: Settings → System → Developer options → USB debugging.",
            ),
        }
    }

    /// Troubleshooting tips for a step.
    pub fn help(step: Step) -> &'static [&'static str] {
        match (language(), step) {
            (Language::French, Step::DeveloperOptions) => &[
                "Si le téléphone demande votre code de verrouillage, saisissez-le : c'est normal.",
                "« Options pour les développeurs » n'apparaît pas ? Tapez « développeur » dans \
                 la barre de recherche des Paramètres.",
                "Toujours bloqué ? Cherchez sur internet « activer options développeur » suivi \
                 du modèle de votre téléphone.",
            ],
            (Language::English, Step::DeveloperOptions) => &[
                "If the phone asks for your screen lock code, enter it: this is normal.",
                "\"Developer options\" does not appear? Type \"developer\" in the Settings search \
                 bar.",
                "Still stuck? Search the web for \"enable developer options\" followed by your \
                 phone model.",
            ],
            (Language::French, Step::UsbDebugging) => &[
                "Une fenêtre d'avertissement s'affiche quand vous activez le débogage USB : \
                 c'est normal, confirmez.",
                "Vous ne trouvez pas l'option ? Tapez « débogage USB » dans la barre de \
                 recherche des Paramètres, ou cherchez sur internet avec le modèle de votre \
                 téléphone.",
            ],
            (Language::English, Step::UsbDebugging) => &[
                "A warning window appears when you turn on USB debugging: this is normal, \
                 confirm.",
                "Cannot find the option? Type \"USB debugging\" in the Settings search bar, or \
                 search the web with your phone model.",
            ],
            (Language::French, Step::Plug | Step::Done) => &[
                "Le téléphone charge mais rien ne se passe ? Le câble ne transmet sans doute \
                 pas de données : essayez le câble d'origine du téléphone, ou un câble qui vous \
                 a déjà servi à transférer des photos.",
                "Branchez directement sur un port à l'arrière du PC (sans hub ni rallonge). Les \
                 ports à l'avant sont souvent moins fiables.",
                "Ouvrez la notification USB du téléphone et choisissez « Transfert de fichiers ».",
                "Vérifiez que « Débogage USB » est toujours activé : il se désactive parfois \
                 après une mise à jour du téléphone.",
                "Certains téléphones ont besoin du pilote USB de leur fabricant (par exemple \
                 « Samsung USB Driver »). Cherchez « pilote USB » et la marque de votre \
                 téléphone sur le site du fabricant.",
                "Redémarrez le téléphone, puis rebranchez-le.",
            ],
            (Language::English, Step::Plug | Step::Done) => &[
                "The phone charges but nothing happens? The cable probably does not carry data: \
                 try the phone's original cable, or one you have already used to transfer \
                 photos.",
                "Plug directly into a port at the back of the PC (no hub or extension). Front \
                 ports are often less reliable.",
                "Open the phone's USB notification and choose \"File transfer\".",
                "Check that \"USB debugging\" is still on: it is sometimes turned off after a \
                 phone update.",
                "Some phones need their maker's USB driver (for example \"Samsung USB Driver\"). \
                 Search for \"USB driver\" and your phone brand on the maker's website.",
                "Restart the phone, then plug it back in.",
            ],
            (Language::French, Step::Authorize) => &[
                "La fenêtre n'apparaît que si l'écran du téléphone est déverrouillé.",
                "Toujours rien ? Dans « Options pour les développeurs », touchez « Révoquer les \
                 autorisations de débogage USB », puis débranchez et rebranchez le câble.",
                "Fermez les autres logiciels Android ouverts sur l'ordinateur (Android Studio, \
                 outils de transfert…).",
            ],
            (Language::English, Step::Authorize) => &[
                "The window only appears when the phone screen is unlocked.",
                "Still nothing? In \"Developer options\", tap \"Revoke USB debugging \
                 authorizations\", then unplug and plug the cable back in.",
                "Close other Android programs open on the computer (Android Studio, transfer \
                 tools…).",
            ],
        }
    }
}

// --- Settings ----------------------------------------------------------------------------

/// Texts of the settings screen.
pub mod settings {
    use super::{language, tr, Language};
    use crate::settings::{Fps, LanguageChoice, QualityPreset};

    pub fn title() -> &'static str {
        super::tab_settings()
    }
    pub fn quality() -> &'static str {
        tr("Qualité vidéo", "Video quality")
    }
    pub fn fps() -> &'static str {
        tr("Fluidité", "Smoothness")
    }
    pub fn folder() -> &'static str {
        tr("Dossier des vidéos", "Videos folder")
    }
    pub fn sound() -> &'static str {
        tr("Son", "Sound")
    }
    pub fn record_sound() -> &'static str {
        tr("Capturer le son du téléphone", "Capture the phone's sound")
    }
    pub fn record_sound_hint() -> &'static str {
        tr(
            "Le son est enregistré avec la vidéo et gardé dans les clips. Android 11 ou plus. \
             Pendant la capture, le téléphone devient muet : son son sort sur le PC.",
            "The sound is recorded with the video and kept in the clips. Android 11 or later. \
             While it is captured, the phone goes silent: its sound comes out of the PC.",
        )
    }
    pub fn listen_sound() -> &'static str {
        tr(
            "Écouter le son sur le PC pendant l'aperçu",
            "Listen on the PC during the preview",
        )
    }
    pub fn listen_sound_hint() -> &'static str {
        tr(
            "Décochez pour enregistrer le son sans l'entendre.",
            "Untick to record the sound without hearing it.",
        )
    }
    pub fn language_title() -> &'static str {
        "Langue / Language"
    }
    pub fn assistant() -> &'static str {
        tr("Connexion du téléphone", "Phone connection")
    }
    pub fn assistant_button() -> &'static str {
        tr(
            "Ouvrir l'assistant de connexion",
            "Open the connection assistant",
        )
    }
    pub fn applies_now() -> &'static str {
        tr(
            "Les changements s'appliquent tout de suite à l'aperçu. Pendant un enregistrement, \
             ils s'appliqueront dès qu'il sera terminé.",
            "Changes apply to the preview right away. During a recording, they apply as soon as \
             it ends.",
        )
    }
    pub fn about() -> &'static str {
        tr("À propos", "About")
    }
    pub fn about_uses() -> &'static str {
        tr(
            "PhonyCut s'appuie sur ces logiciels libres :",
            "PhonyCut is built on this free software:",
        )
    }
    pub fn about_credits() -> [&'static str; 5] {
        match language() {
            Language::French => [
                "scrcpy (serveur Android), par Genymobile — licence Apache 2.0",
                "FFmpeg 9.0 (lecture, enregistrement, export) — licence LGPL 2.1 ou ultérieure, \
                 utilisé sous forme de bibliothèques séparées. Code source : ffmpeg.org",
                "OpenH264 (inclus dans FFmpeg, pour l'export précis) — licence BSD",
                "Android SDK Platform-Tools (adb), par Google — voir la notice fournie",
                "Bibliothèques Rust (egui, eframe…) — licences MIT ou Apache 2.0, liste complète \
                 dans les licences tierces ci-dessous",
            ],
            Language::English => [
                "scrcpy (Android server), by Genymobile — Apache 2.0 license",
                "FFmpeg 9.0 (playback, recording, export) — LGPL 2.1 or later license, used as \
                 separate libraries. Source code: ffmpeg.org",
                "OpenH264 (included in FFmpeg, for precise export) — BSD license",
                "Android SDK Platform-Tools (adb), by Google — see the included notice",
                "Rust libraries (egui, eframe…) — MIT or Apache 2.0 licenses, full list in the \
                 third-party licenses below",
            ],
        }
    }
    pub fn third_party_licenses() -> &'static str {
        tr("Licences tierces", "Third-party licenses")
    }
    pub fn open_licenses() -> &'static str {
        tr("Ouvrir le dossier des licences", "Open the licenses folder")
    }

    pub fn version(version: &str) -> String {
        format!("PhonyCut version {version}")
    }

    pub fn language_choice(choice: LanguageChoice) -> &'static str {
        match choice {
            LanguageChoice::Auto => tr(
                "Automatique (langue de Windows)",
                "Automatic (Windows language)",
            ),
            LanguageChoice::French => "Français",
            LanguageChoice::English => "English",
        }
    }

    pub fn quality_preset(preset: QualityPreset) -> (&'static str, &'static str) {
        match preset {
            QualityPreset::Light => (
                tr("Légère", "Light"),
                tr(
                    "Image plus petite (1280 pixels) et fichiers légers. Pour les ordinateurs \
                     modestes ou les longs enregistrements.",
                    "Smaller picture (1280 pixels) and light files. For modest computers or \
                     long recordings.",
                ),
            ),
            QualityPreset::Standard => (
                tr("Standard (recommandée)", "Standard (recommended)"),
                tr(
                    "Bonne netteté (1920 pixels) avec des fichiers de taille raisonnable.",
                    "Sharp picture (1920 pixels) with reasonably sized files.",
                ),
            ),
            QualityPreset::High => (
                tr("Haute", "High"),
                tr(
                    "Résolution maximale du téléphone. Fichiers environ deux fois plus gros.",
                    "The phone's full resolution. Files about twice as large.",
                ),
            ),
        }
    }

    pub fn fps_choice(fps: Fps) -> (&'static str, &'static str) {
        match fps {
            Fps::Thirty => (
                tr("30 images par seconde", "30 frames per second"),
                tr(
                    "Fichiers plus légers. Suffisant pour la plupart des applications.",
                    "Lighter files. Enough for most apps.",
                ),
            ),
            Fps::Sixty => (
                tr(
                    "60 images par seconde (recommandé)",
                    "60 frames per second (recommended)",
                ),
                tr(
                    "Mouvements plus fluides, idéal pour les jeux.",
                    "Smoother motion, ideal for games.",
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests share the global language: each one sets what it needs, and the
    // assertions that depend on it run under a lock.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn durations() {
        assert_eq!(duration(Duration::from_secs(0)), "00:00");
        assert_eq!(duration(Duration::from_millis(83_900)), "01:23");
        assert_eq!(duration(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn sizes_follow_the_language() {
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_language(Language::French);
        assert_eq!(file_size(0), "0 Ko");
        assert_eq!(file_size(300), "1 Ko");
        assert_eq!(file_size(850 * 1024), "850 Ko");
        assert_eq!(file_size(13_002_342), "12,4 Mo");
        assert_eq!(file_size(1_342_177_280), "1,25 Go");
        set_language(Language::English);
        assert_eq!(file_size(13_002_342), "12.4 MB");
        assert_eq!(file_size(1_342_177_280), "1.25 GB");
        assert_eq!(segment_range(0, 28_000), "00:00 → 00:28  (28.0 s)");
        set_language(Language::French);
        assert_eq!(segment_range(0, 28_000), "00:00 → 00:28  (28,0 s)");
    }

    #[test]
    fn texts_switch_language() {
        let _guard = LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_language(Language::English);
        assert_eq!(no_device(), "No phone detected");
        assert_eq!(export_button(3), "Export 3 clips");
        assert_eq!(onboarding::step_short()[0], "Developer mode");
        set_language(Language::French);
        assert_eq!(no_device(), "Aucun téléphone détecté");
        assert_eq!(export_button(3), "Exporter 3 clips");
    }
}
