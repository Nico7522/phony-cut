//! All user-facing text, in French. Kept in one place to allow translation later.

use std::time::Duration;

use phonecut_core::api::{AdbProblem, DeviceInfo, DeviceState, RecordingProblem};

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

pub const STARTING: &str = "Préparation de la connexion…";
pub const STARTING_HINT: &str = "Patientez quelques secondes.";

pub const NO_DEVICE: &str = "Aucun téléphone détecté";
pub const NO_DEVICE_HINT: &str = "Branchez votre téléphone avec un câble USB et vérifiez que le \
    débogage USB est activé. Si rien ne se passe, essayez un autre câble ou un autre port USB.";

pub const CHOOSE_DEVICE: &str = "Plusieurs téléphones détectés";
pub const CHOOSE_DEVICE_HINT: &str = "Choisissez le téléphone à utiliser dans la liste ci-dessous.";

pub const ADB_CONFLICT: &str = "Un autre logiciel Android est ouvert sur cet ordinateur \
    (Android Studio, outil de transfert…). Fermez-le pour éviter les coupures de connexion.";

pub const DEVICE_LIST: &str = "Téléphones branchés :";

pub const MIRROR_STARTING: &str = "Lancement de l'aperçu…";
pub const MIRROR_INTERRUPTED: &str = "L'aperçu s'est interrompu";
pub const MIRROR_INTERRUPTED_HINT: &str = "Appuyez sur « Relancer l'aperçu ». Si le problème \
    continue, débranchez puis rebranchez le câble USB.";
pub const MIRROR_RETRY: &str = "Relancer l'aperçu";
pub const LIVE_BADGE: &str = "Aperçu en direct";

pub const RECORD_START: &str = "●  Enregistrer";
pub const RECORD_STOP: &str = "Arrêter l'enregistrement";
pub const RECORD_WAITING: &str = "Démarrage de l'enregistrement…";
pub const RECORD_LIVE: &str = "● REC";
pub const RECORD_SAVED: &str = "Vidéo enregistrée :";
pub const RECORD_SAVED_AFTER_UNPLUG: &str = "Le téléphone s'est déconnecté : l'enregistrement \
    s'est arrêté, mais la vidéo a bien été sauvegardée.";
pub const OPEN_FOLDER: &str = "Ouvrir le dossier";
pub const FOLDER_LABEL: &str = "Dossier des vidéos :";
pub const FOLDER_CHANGE: &str = "Modifier…";
pub const PICK_FOLDER_TITLE: &str = "Choisir le dossier des vidéos";
pub const FOLDER_INVALID: &str = "Impossible d'utiliser ce dossier. Vérifiez le chemin ou \
    choisissez un autre dossier (par exemple dans « Vidéos »).";
pub const OPEN_FOLDER_FAILED: &str = "Impossible d'ouvrir le dossier.";

pub fn recording_problem(problem: RecordingProblem) -> &'static str {
    match problem {
        RecordingProblem::NotMirroring => {
            "L'enregistrement n'a pas pu démarrer : l'aperçu n'est pas actif. \
             Attendez que l'écran du téléphone s'affiche, puis réessayez."
        }
        RecordingProblem::AlreadyRecording => "Un enregistrement est déjà en cours.",
        RecordingProblem::CannotCreateFile => {
            "Impossible de créer la vidéo dans le dossier choisi. Choisissez un autre \
             dossier avec « Modifier… », puis réessayez."
        }
        RecordingProblem::WriteFailed => {
            "L'enregistrement s'est arrêté : impossible d'écrire sur le disque (disque \
             plein ou retiré ?). Libérez de la place, puis relancez l'enregistrement."
        }
        RecordingProblem::NoVideo => {
            "L'enregistrement a été arrêté avant de commencer : aucune vidéo n'a été créée."
        }
    }
}

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

/// French-style size: `850 Ko`, `12,4 Mo`, `1,25 Go`.
pub fn file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    let text = if b < KB * KB {
        format!("{:.0} Ko", (b / KB).max(if bytes > 0 { 1.0 } else { 0.0 }))
    } else if b < KB * KB * KB {
        format!("{:.1} Mo", b / (KB * KB))
    } else {
        format!("{:.2} Go", b / (KB * KB * KB))
    };
    text.replace('.', ",")
}

pub fn adb_problem(problem: AdbProblem) -> Message {
    match problem {
        AdbProblem::AdbMissing => Message::new(
            "Un fichier de PhoneCut est manquant",
            "Réinstallez PhoneCut pour réparer l'installation.",
        ),
        AdbProblem::ServerFailed => Message::new(
            "Impossible de démarrer la connexion au téléphone",
            "Fermez les autres logiciels Android ouverts (Android Studio, outils de \
             transfert ou de flashage…). PhoneCut réessaie automatiquement.",
        ),
    }
}

pub fn device(device: &DeviceInfo) -> Message {
    match &device.state {
        DeviceState::Ready => Message::new(
            format!("Téléphone détecté : {}", device.display_name()),
            "Tout est prêt.",
        ),
        DeviceState::Unauthorized => Message::new(
            "Autorisation nécessaire sur le téléphone",
            "Déverrouillez le téléphone et appuyez sur « Autoriser » dans la fenêtre \
             « Autoriser le débogage USB ? ». Cochez « Toujours autoriser sur cet \
             ordinateur ». Si la fenêtre n'apparaît pas, débranchez puis rebranchez le câble.",
        ),
        DeviceState::Authorizing => {
            Message::new("Autorisation en cours…", "Patientez quelques secondes.")
        }
        DeviceState::Offline => Message::new(
            "Le téléphone ne répond pas",
            "Débranchez puis rebranchez le câble USB. Si le problème continue, \
             redémarrez le téléphone.",
        ),
        DeviceState::Connecting => {
            Message::new("Connexion au téléphone…", "Patientez quelques secondes.")
        }
        DeviceState::NoPermissions => Message::new(
            "Accès au téléphone refusé",
            "Débranchez puis rebranchez le câble, puis acceptez l'autorisation sur le téléphone.",
        ),
        DeviceState::Other(_) => Message::new(
            "Le téléphone est dans un mode inhabituel",
            "Redémarrez le téléphone normalement, puis rebranchez-le.",
        ),
    }
}

/// Short state label used in the device list.
pub fn device_state_short(state: &DeviceState) -> &'static str {
    match state {
        DeviceState::Ready => "prêt",
        DeviceState::Unauthorized => "autorisation nécessaire",
        DeviceState::Authorizing => "autorisation en cours",
        DeviceState::Offline => "ne répond pas",
        DeviceState::Connecting => "connexion…",
        DeviceState::NoPermissions => "accès refusé",
        DeviceState::Other(_) => "mode inhabituel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration(Duration::from_secs(0)), "00:00");
        assert_eq!(duration(Duration::from_millis(83_900)), "01:23");
        assert_eq!(duration(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn sizes() {
        assert_eq!(file_size(0), "0 Ko");
        assert_eq!(file_size(300), "1 Ko");
        assert_eq!(file_size(850 * 1024), "850 Ko");
        assert_eq!(file_size(13_002_342), "12,4 Mo");
        assert_eq!(file_size(1_342_177_280), "1,25 Go");
    }
}
