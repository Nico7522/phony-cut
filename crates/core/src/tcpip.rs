//! "Switch to Wi-Fi" for any Android version (like scrcpy's `--tcpip`): while
//! the phone is plugged in, `adb tcpip 5555` makes it listen on the network,
//! then `adb connect <its ip>:5555`. The cable can then be unplugged. The phone
//! stops listening when it restarts: the cable is needed once again.
//!
//! Addresses that worked are remembered (in the settings folder) and connected
//! to again in the background while PhonyCut runs, so the phone comes back by
//! itself after a Wi-Fi drop or a PhonyCut restart (not after a phone restart).
//!
//! Real `adb connect` answers (adb 37.0.1, exit code always 0): `connected
//! to …`, `already connected to …`, `failed to authenticate to …` (the phone
//! asks to allow the PC), `cannot connect to …: … (10060)`.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::adb::{Adb, ConnectResult};
use crate::api::{Event, Notifier, WifiSwitchProblem, WifiSwitchState};

/// Port the phone listens on after `adb tcpip` (adb's default).
pub const TCPIP_PORT: u16 = 5555;
const FILE_NAME: &str = "wifi-phones.json";
const MAX_REMEMBERED: usize = 5;
/// The phone's adb daemon restarts after `adb tcpip`.
const RESTART_DELAY: Duration = Duration::from_secs(2);
const CONNECT_ATTEMPTS: u32 = 5;

/// Addresses (`ip:5555`) of phones switched to Wi-Fi, most recent first.
#[derive(Debug, Default)]
pub struct KnownPhones {
    addresses: Mutex<Vec<String>>,
    path: Option<PathBuf>,
    /// A background `adb connect` round is running.
    reconnecting: AtomicBool,
}

fn lock(phones: &KnownPhones) -> MutexGuard<'_, Vec<String>> {
    phones
        .addresses
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl KnownPhones {
    /// Loads the remembered addresses (none if the file is missing or bad).
    pub fn load() -> Self {
        let path = crate::paths::app_data_dir().ok().map(|d| d.join(FILE_NAME));
        let addresses = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
            .unwrap_or_default();
        Self {
            addresses: Mutex::new(addresses),
            path,
            reconnecting: AtomicBool::new(false),
        }
    }

    pub fn is_empty(&self) -> bool {
        lock(self).is_empty()
    }

    pub fn addresses(&self) -> Vec<String> {
        lock(self).clone()
    }

    fn remember(&self, address: &str) {
        let list = {
            let mut list = lock(self);
            list.retain(|a| a != address);
            list.insert(0, address.to_owned());
            list.truncate(MAX_REMEMBERED);
            list.clone()
        };
        if let Some(path) = &self.path {
            let saved = serde_json::to_string_pretty(&list)
                .map_err(std::io::Error::other)
                .and_then(|text| fs::write(path, text));
            if let Err(err) = saved {
                tracing::warn!(%err, "cannot save the Wi-Fi phones");
            }
        }
    }

    /// Connects again, in the background, to remembered phones missing from
    /// `connected` (serials). One round at a time.
    pub fn reconnect(self: &Arc<Self>, adb: &Adb, connected: &[String]) {
        let missing: Vec<String> = self
            .addresses()
            .into_iter()
            .filter(|a| !connected.contains(a))
            .collect();
        if missing.is_empty() || self.reconnecting.swap(true, Ordering::AcqRel) {
            return;
        }
        let (adb, phones) = (adb.clone(), Arc::clone(self));
        let spawned = thread::Builder::new()
            .name("wifi-reconnect".into())
            .spawn(move || {
                for address in missing {
                    match adb.connect(&address) {
                        Ok(ConnectResult::Connected) => {
                            tracing::info!(%address, "phone reconnected over Wi-Fi");
                        }
                        Ok(other) => tracing::debug!(%address, ?other, "Wi-Fi phone not reachable"),
                        Err(err) => tracing::debug!(%address, %err, "adb connect"),
                    }
                }
                phones.reconnecting.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.reconnecting.store(false, Ordering::Release);
        }
    }
}

/// Switches the plugged-in phone `serial` to Wi-Fi, on its own thread.
pub fn switch_to_wifi(
    adb: Adb,
    serial: String,
    phones: Arc<KnownPhones>,
    events: Sender<Event>,
    notify: Notifier,
) {
    let report = move |state: WifiSwitchState| {
        if events.send(Event::WifiSwitch(state)).is_ok() {
            notify();
        }
    };
    let spawned = thread::Builder::new()
        .name("wifi-switch".into())
        .spawn(move || {
            report(WifiSwitchState::Switching);
            report(switch(&adb, &serial, &phones));
        });
    if let Err(err) = spawned {
        tracing::error!(%err, "cannot start the Wi-Fi switch");
    }
}

fn switch(adb: &Adb, serial: &str, phones: &KnownPhones) -> WifiSwitchState {
    // Read before `tcpip`: the USB connection drops while adbd restarts.
    let Some(ip) = adb.wifi_address(serial) else {
        tracing::info!("the phone is not on a Wi-Fi network");
        return WifiSwitchState::Failed(WifiSwitchProblem::NoWifi);
    };
    let address = format!("{ip}:{TCPIP_PORT}");
    if let Err(err) = adb.tcpip(serial, TCPIP_PORT) {
        tracing::warn!(%err, "adb tcpip failed");
        return WifiSwitchState::Failed(WifiSwitchProblem::Unreachable);
    }
    thread::sleep(RESTART_DELAY);
    for attempt in 1..=CONNECT_ATTEMPTS {
        match adb.connect(&address) {
            Ok(ConnectResult::Connected) => {
                tracing::info!(%address, "phone switched to Wi-Fi");
                phones.remember(&address);
                return WifiSwitchState::Done { address };
            }
            Ok(ConnectResult::Unauthorized) => {
                // Accepting on the phone is enough: the background
                // reconnection connects once it is allowed.
                tracing::info!(%address, "the phone asks to allow this PC over Wi-Fi");
                phones.remember(&address);
                return WifiSwitchState::Failed(WifiSwitchProblem::Unauthorized);
            }
            Ok(ConnectResult::Failed(answer)) => {
                tracing::debug!(%address, attempt, %answer, "adb connect");
            }
            Err(err) => tracing::debug!(%address, attempt, %err, "adb connect"),
        }
        thread::sleep(Duration::from_secs(1));
    }
    tracing::info!(%address, "phone unreachable over Wi-Fi");
    WifiSwitchState::Failed(WifiSwitchProblem::Unreachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_addresses_most_recent_first_without_duplicates() {
        let phones = KnownPhones::default(); // no file
        phones.remember("192.168.1.45:5555");
        phones.remember("192.168.1.60:5555");
        phones.remember("192.168.1.45:5555");
        assert_eq!(
            phones.addresses(),
            ["192.168.1.45:5555", "192.168.1.60:5555"]
        );
        for i in 0..10 {
            phones.remember(&format!("10.0.0.{i}:5555"));
        }
        assert_eq!(phones.addresses().len(), MAX_REMEMBERED);
    }
}
