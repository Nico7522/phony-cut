//! All user-facing text, in French. Kept in one place to allow translation later.

use phonecut_core::api::{AdbProblem, DeviceInfo, DeviceState};

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
