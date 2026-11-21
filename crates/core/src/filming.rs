//! "Filming mode": makes the phone look clean on video, and always puts it
//! back as it was.
//!
//! - clean status bar: Android's SystemUI demo mode (12:00, full battery,
//!   full Wi-Fi, no notification icons);
//! - touches shown (`show_touches`);
//! - Do not disturb (`cmd notification set_dnd priority`).
//!
//! Before changing anything, the phone's own values are saved to a file in
//! the settings folder, keyed by the phone's hardware serial. They are put
//! back when the mode is turned off, when PhonyCut closes, and — if the phone
//! was unplugged meanwhile — the next time the phone is seen (USB or Wi-Fi).
//! Commands checked on a Xiaomi (Android 16): each value reads back changed,
//! then restored.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{unbounded, Sender};
use serde::{Deserialize, Serialize};

use crate::adb::{Adb, AdbError};
use crate::api::FilmingOptions;

const FILE_NAME: &str = "filming-restore.json";
const TIMEOUT: Duration = Duration::from_secs(10);
const DEMO: &str = "am broadcast -a com.android.systemui.demo -e command";

/// The phone's own values, to put back. `None`: the setting did not exist.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedState {
    /// Changed by the mode (only those are put back).
    pub demo: bool,
    pub demo_allowed: Option<String>,
    pub touches: bool,
    pub show_touches: Option<String>,
    pub dnd: bool,
    pub zen_mode: Option<String>,
}

/// Pending restorations, by hardware serial, kept on disk.
#[derive(Debug, Default)]
pub struct RestoreFile {
    path: Option<PathBuf>,
}

impl RestoreFile {
    pub fn new() -> Self {
        Self {
            path: crate::paths::app_data_dir().ok().map(|d| d.join(FILE_NAME)),
        }
    }

    fn load(&self) -> HashMap<String, SavedState> {
        self.path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn store(&self, all: &HashMap<String, SavedState>) {
        let Some(path) = &self.path else { return };
        let written = if all.is_empty() {
            match fs::remove_file(path) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            }
        } else {
            serde_json::to_string_pretty(all)
                .map_err(std::io::Error::other)
                .and_then(|text| fs::write(path, text))
        };
        if let Err(err) = written {
            tracing::warn!(%err, "cannot save the filming mode state");
        }
    }

    pub fn pending(&self, hardware_serial: &str) -> bool {
        self.load().contains_key(hardware_serial)
    }

    fn save(&self, hardware_serial: &str, state: SavedState) {
        let mut all = self.load();
        // Never overwrite the original values with modified ones.
        all.entry(hardware_serial.to_owned()).or_insert(state);
        self.store(&all);
    }

    fn take(&self, hardware_serial: &str) -> Option<SavedState> {
        let all = self.load();
        all.get(hardware_serial).cloned()
    }

    fn forget(&self, hardware_serial: &str) {
        let mut all = self.load();
        if all.remove(hardware_serial).is_some() {
            self.store(&all);
        }
    }
}

fn shell(adb: &Adb, serial: &str, command: &str) -> Result<String, AdbError> {
    adb.run(&["-s", serial, "shell", command], TIMEOUT)
        .map(|out| out.stdout.trim().to_owned())
}

/// A `settings get` value; `None` when unset ("null").
fn setting(
    adb: &Adb,
    serial: &str,
    namespace: &str,
    key: &str,
) -> Result<Option<String>, AdbError> {
    let value = shell(adb, serial, &format!("settings get {namespace} {key}"))?;
    Ok((!value.is_empty() && value != "null").then_some(value))
}

fn put_setting(
    adb: &Adb,
    serial: &str,
    namespace: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), AdbError> {
    let command = match value {
        Some(value) => format!("settings put {namespace} {key} {value}"),
        None => format!("settings delete {namespace} {key}"),
    };
    shell(adb, serial, &command).map(|_| ())
}

/// `cmd notification set_dnd` value for a `zen_mode` (0 off, 1 priority,
/// 2 total silence, 3 alarms).
fn dnd_command(zen_mode: Option<&str>) -> &'static str {
    match zen_mode {
        Some("1") => "priority",
        Some("2") => "none",
        Some("3") => "alarms",
        _ => "off",
    }
}

/// Turns the mode on for `serial`; the original values are saved first.
pub fn apply(
    adb: &Adb,
    serial: &str,
    hardware_serial: &str,
    options: FilmingOptions,
    file: &RestoreFile,
) -> Result<(), AdbError> {
    let saved = SavedState {
        demo: options.clean_status_bar,
        demo_allowed: setting(adb, serial, "global", "sysui_demo_allowed")?,
        touches: options.show_touches,
        show_touches: setting(adb, serial, "system", "show_touches")?,
        dnd: options.do_not_disturb,
        zen_mode: setting(adb, serial, "global", "zen_mode")?,
    };
    file.save(hardware_serial, saved);
    if options.clean_status_bar {
        shell(
            adb,
            serial,
            &format!(
                "settings put global sysui_demo_allowed 1; {DEMO} enter; \
                 {DEMO} clock -e hhmm 1200; \
                 {DEMO} battery -e level 100 -e plugged false; \
                 {DEMO} network -e wifi show -e level 4 -e fully true; \
                 {DEMO} network -e mobile show -e datatype none -e level 4; \
                 {DEMO} notifications -e visible false"
            ),
        )?;
    }
    if options.show_touches {
        put_setting(adb, serial, "system", "show_touches", Some("1"))?;
    }
    if options.do_not_disturb {
        shell(adb, serial, "cmd notification set_dnd priority")?;
    }
    tracing::info!(?options, "filming mode on");
    Ok(())
}

/// Puts the phone back as it was, if the mode changed it. Returns `true`
/// when something was restored.
pub fn restore(
    adb: &Adb,
    serial: &str,
    hardware_serial: &str,
    file: &RestoreFile,
) -> Result<bool, AdbError> {
    let Some(saved) = file.take(hardware_serial) else {
        return Ok(false);
    };
    if saved.demo {
        shell(adb, serial, &format!("{DEMO} exit"))?;
        put_setting(
            adb,
            serial,
            "global",
            "sysui_demo_allowed",
            saved.demo_allowed.as_deref(),
        )?;
    }
    if saved.touches {
        put_setting(
            adb,
            serial,
            "system",
            "show_touches",
            saved.show_touches.as_deref(),
        )?;
    }
    if saved.dnd {
        shell(
            adb,
            serial,
            &format!(
                "cmd notification set_dnd {}",
                dnd_command(saved.zen_mode.as_deref())
            ),
        )?;
    }
    // Only once every value is back: a failure keeps it for the next time.
    file.forget(hardware_serial);
    tracing::info!("phone settings restored after the filming mode");
    Ok(true)
}

/// Work for the filming thread.
type Job = Box<dyn FnOnce(&RestoreFile) + Send>;

/// The filming mode of the service: applied to the phone being mirrored,
/// restored when the preview stops. Phone commands (a few hundred ms over
/// Wi-Fi) run on one thread, in the order they were asked: a restore is
/// always done before the next apply.
pub struct FilmingControl {
    /// What the user wants (`None`: off).
    wanted: Option<FilmingOptions>,
    /// (adb serial, hardware serial) of the phone currently changed.
    applied: Option<(String, String)>,
    file: Arc<RestoreFile>,
    /// Held while a job runs (the service thread never waits on it).
    busy: Arc<Mutex<()>>,
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl FilmingControl {
    pub fn new() -> Self {
        let file = Arc::new(RestoreFile::new());
        let busy = Arc::new(Mutex::new(()));
        let (jobs, queue) = unbounded::<Job>();
        let (thread_file, thread_busy) = (Arc::clone(&file), Arc::clone(&busy));
        let thread = thread::Builder::new()
            .name("filming".into())
            .spawn(move || {
                for job in queue {
                    let _one_at_a_time = thread_busy.lock().unwrap_or_else(PoisonError::into_inner);
                    job(&thread_file);
                }
            })
            .map_err(|err| tracing::warn!(%err, "cannot start the filming mode thread"))
            .ok();
        Self {
            wanted: None,
            applied: None,
            file,
            busy,
            jobs: thread.is_some().then_some(jobs),
            thread,
        }
    }

    fn queue(&self, job: impl FnOnce(&RestoreFile) + Send + 'static) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Box::new(job));
        }
    }

    /// The preview of `serial` started.
    pub fn mirror_started(&mut self, adb: &Adb, serial: &str, hardware_serial: &str) {
        self.mirror_stopped(adb);
        let Some(options) = self.wanted else { return };
        self.applied = Some((serial.to_owned(), hardware_serial.to_owned()));
        let (adb, serial, hardware) = (adb.clone(), serial.to_owned(), hardware_serial.to_owned());
        self.queue(move |file| {
            if let Err(err) = apply(&adb, &serial, &hardware, options, file) {
                tracing::warn!(%err, "cannot turn the filming mode on");
            }
        });
    }

    /// The preview stopped (or the mode changed): the phone is put back.
    pub fn mirror_stopped(&mut self, adb: &Adb) {
        let Some((serial, hardware)) = self.applied.take() else {
            return;
        };
        let adb = adb.clone();
        self.queue(move |file| restore_or_log(&adb, &serial, &hardware, file));
    }

    /// New choice from the settings, applied to the running preview.
    pub fn set(
        &mut self,
        adb: &Adb,
        wanted: Option<FilmingOptions>,
        mirrored: Option<(&str, &str)>,
    ) {
        if self.wanted == wanted {
            return;
        }
        self.wanted = wanted;
        self.mirror_stopped(adb);
        if let Some((serial, hardware)) = mirrored {
            self.mirror_started(adb, serial, hardware);
        }
    }

    /// A phone is connected: put it back if it was left changed (unplugged
    /// during the filming mode, PhonyCut closed abruptly).
    pub fn phone_seen(&self, adb: &Adb, serial: &str, hardware_serial: &str) {
        let in_use = self
            .applied
            .as_ref()
            .is_some_and(|(_, h)| h == hardware_serial);
        if in_use || !self.file.pending(hardware_serial) {
            return;
        }
        let Ok(_one_at_a_time) = self.busy.try_lock() else {
            return; // next poll
        };
        restore_or_log(adb, serial, hardware_serial, &self.file);
    }

    /// PhonyCut closes: the phone is put back, and this waits until every
    /// queued phone command is done (a few hundred ms).
    pub fn shutdown(&mut self, adb: &Adb) {
        self.mirror_stopped(adb);
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::error!("filming mode thread panicked");
            }
        }
    }
}

impl Default for FilmingControl {
    fn default() -> Self {
        Self::new()
    }
}

fn restore_or_log(adb: &Adb, serial: &str, hardware_serial: &str, file: &RestoreFile) {
    if let Err(err) = restore(adb, serial, hardware_serial, file) {
        // Kept in the file: done the next time the phone is seen.
        tracing::warn!(%err, "cannot put the phone back yet");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnd_values_map_to_commands() {
        assert_eq!(dnd_command(None), "off");
        assert_eq!(dnd_command(Some("0")), "off");
        assert_eq!(dnd_command(Some("1")), "priority");
        assert_eq!(dnd_command(Some("2")), "none");
        assert_eq!(dnd_command(Some("3")), "alarms");
    }

    #[test]
    fn original_values_are_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("phonycut-filming-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = RestoreFile {
            path: Some(dir.join(FILE_NAME)),
        };
        let original = SavedState {
            dnd: true,
            zen_mode: Some("0".into()),
            ..SavedState::default()
        };
        file.save("SERIAL", original.clone());
        // Turned on again before a restore: the saved values stay the originals.
        file.save(
            "SERIAL",
            SavedState {
                dnd: true,
                zen_mode: Some("1".into()),
                ..SavedState::default()
            },
        );
        assert!(file.pending("SERIAL"));
        assert_eq!(file.take("SERIAL"), Some(original));
        file.forget("SERIAL");
        assert!(!file.pending("SERIAL"));
        assert!(!dir.join(FILE_NAME).exists(), "file removed once empty");
        fs::remove_dir_all(&dir).unwrap();
    }
}
