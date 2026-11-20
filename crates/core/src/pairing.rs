//! Pairing a phone over Wi-Fi (Android 11+ "Wireless debugging"), checked
//! with adb 37.0.1 and a real phone (see docs/decisions/0011):
//!
//! - **QR code**: we show `WIFI:T:ADB;S:<name>;P:<password>;;` (the format of
//!   Android Studio). Once the phone scanned it, it offers pairing on the
//!   network as an mDNS service `_adb-tls-pairing._tcp` named `<name>`; we
//!   then run `adb pair <ip:port> <password>`.
//! - **Code**: the phone shows a 6-digit code and offers pairing the same
//!   way under its own name; the user only types the code, the address is
//!   found on the network.
//!
//! After pairing, the adb server connects to the phone by itself
//! (`ADB_MDNS_AUTO_CONNECT`, default `adb-tls-connect`) whenever it is on the
//! same network: it then shows up in the device list like a USB phone.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::adb::{Adb, MdnsService, PairError, PAIRING_SERVICE};
use crate::api::{Event, Notifier, PairingProblem, PairingState};

/// Prefix of our QR code pairing names (to tell them from code pairings).
const QR_NAME_PREFIX: &str = "phonycut-";
/// How long the QR code stays valid (the user has to find the menu).
const QR_TIMEOUT: Duration = Duration::from_secs(180);
/// The phone offers code pairing as soon as its dialog is open.
const CODE_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(800);

/// Text of the QR code to scan from "Pair device with QR code".
pub fn qr_payload(name: &str, password: &str) -> String {
    format!("WIFI:T:ADB;S:{name};P:{password};;")
}

/// Random letters and digits, from the OS-seeded hasher keys.
fn random_text(len: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(out.len());
        let mut bits = hasher.finish();
        for _ in 0..8 {
            if out.len() == len {
                break;
            }
            out.push(char::from(CHARS[(bits % CHARS.len() as u64) as usize]));
            bits /= CHARS.len() as u64;
        }
    }
    out
}

/// A pairing running on its own thread; dropping it stops the search.
pub struct PairingJob {
    cancel: Arc<AtomicBool>,
}

impl PairingJob {
    /// Shows a QR code, waits for the phone, pairs.
    pub fn with_qr(adb: Adb, events: Sender<Event>, notify: Notifier) -> std::io::Result<Self> {
        let name = format!("{QR_NAME_PREFIX}{}", random_text(8));
        let password = random_text(12);
        Self::spawn(events, notify, move |report, cancel| {
            report(PairingState::WaitingForPhone {
                qr_payload: qr_payload(&name, &password),
            });
            let found = wait_for(&adb, cancel, QR_TIMEOUT, |s| s.instance == name);
            finish(&adb, found, &password, report);
        })
    }

    /// Pairs with the code shown by the phone, finding its address.
    pub fn with_code(
        adb: Adb,
        code: String,
        events: Sender<Event>,
        notify: Notifier,
    ) -> std::io::Result<Self> {
        let code: String = code.chars().filter(char::is_ascii_digit).collect();
        Self::spawn(events, notify, move |report, cancel| {
            let found = wait_for(&adb, cancel, CODE_TIMEOUT, |s| {
                !s.instance.starts_with(QR_NAME_PREFIX)
            });
            finish(&adb, found, &code, report);
        })
    }

    fn spawn(
        events: Sender<Event>,
        notify: Notifier,
        work: impl FnOnce(&dyn Fn(PairingState), &AtomicBool) + Send + 'static,
    ) -> std::io::Result<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        thread::Builder::new()
            .name("pairing".into())
            .spawn(move || {
                let report = |state: PairingState| {
                    // Nothing more to say once cancelled (a new pairing may run).
                    if !flag.load(Ordering::Relaxed) && events.send(Event::Pairing(state)).is_ok() {
                        notify();
                    }
                };
                work(&report, &flag);
            })?;
        Ok(Self { cancel })
    }
}

impl Drop for PairingJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Some(address) of the first pairing service matching `wanted`; None on
/// timeout or cancel.
fn wait_for(
    adb: &Adb,
    cancel: &AtomicBool,
    timeout: Duration,
    wanted: impl Fn(&MdnsService) -> bool,
) -> Option<Option<String>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        match adb.mdns_services() {
            Ok(services) => {
                if let Some(service) = services
                    .iter()
                    .find(|s| s.service == PAIRING_SERVICE && wanted(s))
                {
                    tracing::info!(instance = %service.instance, address = %service.address, "phone offers pairing");
                    return Some(Some(service.address.clone()));
                }
            }
            Err(err) => tracing::debug!(%err, "mdns services"),
        }
        thread::sleep(POLL);
    }
    Some(None)
}

fn finish(adb: &Adb, found: Option<Option<String>>, secret: &str, report: &dyn Fn(PairingState)) {
    let address = match found {
        None => return, // cancelled
        Some(None) => {
            tracing::info!("no phone offering pairing found");
            report(PairingState::Failed(PairingProblem::PhoneNotFound));
            return;
        }
        Some(Some(address)) => address,
    };
    report(PairingState::Pairing);
    match adb.pair(&address, secret) {
        Ok(()) => {
            tracing::info!(%address, "phone paired over Wi-Fi");
            report(PairingState::Paired);
        }
        Err(PairError::WrongCode) => {
            tracing::info!("pairing refused: wrong code");
            report(PairingState::Failed(PairingProblem::WrongCode));
        }
        Err(err) => {
            tracing::warn!(%err, "pairing failed");
            report(PairingState::Failed(PairingProblem::Failed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_payload_has_the_android_studio_format() {
        assert_eq!(
            qr_payload("phonycut-abc", "secret12"),
            "WIFI:T:ADB;S:phonycut-abc;P:secret12;;"
        );
    }

    #[test]
    fn random_text_is_random_and_safe_in_a_qr_payload() {
        let (a, b) = (random_text(12), random_text(12));
        assert_eq!(a.len(), 12);
        assert_ne!(a, b);
        assert!(
            a.chars().all(|c| c.is_ascii_alphanumeric()),
            "no ; or : that would break the payload"
        );
    }
}
