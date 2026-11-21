//! The phone's files: browse its shared storage and copy files both ways,
//! with `adb` (works over USB and Wi-Fi).
//!
//! Only the shared storage (`/sdcard`: DCIM, Download…) is reachable; apps'
//! private data is not. Listing uses toybox `find` + `stat`, checked on a
//! Xiaomi (Android 16). Copies run one file at a time on their own thread, so
//! progress is exact and a cancel stops right away (the partial file is
//! removed).

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::adb::Adb;
use crate::api::{
    Event, Notifier, PhoneFilesProblem, RemoteEntry, TransferDirection, TransferProblem,
    TransferState,
};

/// Where the phone's shared storage starts.
pub const STORAGE_ROOT: &str = "/sdcard";

const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const QUICK_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);
/// Progress events at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// How often the size of a file being sent is read on the phone.
const PUSH_POLL: Duration = Duration::from_secs(1);

/// `'it'\''s'`: a path for the phone's shell.
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `/sdcard/DCIM` + `Camera` → `/sdcard/DCIM/Camera`.
pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// Parent folder, never above the storage root.
pub fn parent(dir: &str) -> Option<String> {
    let dir = dir.trim_end_matches('/');
    if dir == STORAGE_ROOT || !dir.starts_with(STORAGE_ROOT) {
        return None;
    }
    dir.rsplit_once('/').map(|(parent, _)| parent.to_owned())
}

/// One `stat -c '%F|%s|%Y|%n'` line.
fn parse_stat_line(line: &str) -> Option<RemoteEntry> {
    let mut parts = line.splitn(4, '|');
    let kind = parts.next()?;
    let size = parts.next()?.parse().ok()?;
    let modified = parts.next()?.parse().ok();
    let path = parts.next()?.to_owned();
    let name = path.rsplit('/').next()?.to_owned();
    if name.is_empty() {
        return None;
    }
    Some(RemoteEntry {
        is_dir: kind == "directory",
        size,
        modified,
        name,
        path,
    })
}

/// Visible entries of a listing: hidden files left out, folders first,
/// then by name.
fn parse_listing(output: &str) -> Vec<RemoteEntry> {
    let mut entries: Vec<RemoteEntry> = output
        .lines()
        .filter_map(parse_stat_line)
        .filter(|e| !e.name.starts_with('.'))
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// The content of a folder of the phone (blocking: up to a few seconds).
pub fn list(adb: &Adb, serial: &str, dir: &str) -> Result<Vec<RemoteEntry>, PhoneFilesProblem> {
    let command = format!(
        "find {} -mindepth 1 -maxdepth 1 -exec stat -c '%F|%s|%Y|%n' {{}} + 2>&1",
        sh_quote(dir)
    );
    let out = adb
        .run(&["-s", serial, "shell", &command], LIST_TIMEOUT)
        .map_err(|err| {
            tracing::warn!(%err, "cannot list the phone's files");
            PhoneFilesProblem::Failed
        })?;
    let text = out.stdout;
    if text.contains("No such file") {
        return Err(PhoneFilesProblem::NotFound);
    }
    if text.contains("Permission denied") && !text.lines().any(|l| parse_stat_line(l).is_some()) {
        return Err(PhoneFilesProblem::Denied);
    }
    if text.trim().is_empty() && out.stderr.contains("not found") {
        return Err(PhoneFilesProblem::Failed);
    }
    Ok(parse_listing(&text))
}

/// A file to copy: where it comes from, where it goes, its size.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    from: String,
    to: String,
    size: u64,
}

/// Runs a copy on its own thread, reporting [`TransferState`].
pub struct TransferJob {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TransferJob {
    /// Copies files or folders of the phone into `folder` on the PC.
    pub fn to_pc(
        adb: Adb,
        serial: String,
        paths: Vec<String>,
        folder: PathBuf,
        events: Sender<Event>,
        notify: Notifier,
    ) -> std::io::Result<Self> {
        Self::start(
            events,
            notify,
            TransferDirection::ToPc,
            move |cancel, report| pull(&adb, &serial, &paths, &folder, cancel, report),
        )
    }

    /// Copies files or folders of the PC into the phone's folder `dir`.
    pub fn to_phone(
        adb: Adb,
        serial: String,
        files: Vec<PathBuf>,
        dir: String,
        events: Sender<Event>,
        notify: Notifier,
    ) -> std::io::Result<Self> {
        Self::start(
            events,
            notify,
            TransferDirection::ToPhone,
            move |cancel, report| push(&adb, &serial, &files, &dir, cancel, report),
        )
    }

    fn start(
        events: Sender<Event>,
        notify: Notifier,
        direction: TransferDirection,
        work: impl FnOnce(&AtomicBool, &mut dyn FnMut(TransferState)) -> TransferState + Send + 'static,
    ) -> std::io::Result<Self> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = thread::Builder::new()
            .name("phone-files".into())
            .spawn(move || {
                let mut send = |state: TransferState| {
                    if events.send(Event::Transfer(state)).is_ok() {
                        notify();
                    }
                };
                send(TransferState::Running {
                    direction,
                    done: 0,
                    total: 0,
                    file: String::new(),
                });
                let state = work(&flag, &mut send);
                send(state);
            })?;
        Ok(Self {
            cancel,
            thread: Some(thread),
        })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for TransferJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Why adb failed, from its message.
fn problem_from(message: &str) -> TransferProblem {
    let message = message.to_lowercase();
    if message.contains("no space") || message.contains("disk full") {
        TransferProblem::NoSpace
    } else if message.contains("not found") && message.contains("device")
        || message.contains("offline")
        || message.contains("no devices")
    {
        TransferProblem::PhoneGone
    } else if message.contains("permission denied") || message.contains("read-only") {
        TransferProblem::Denied
    } else {
        TransferProblem::Failed
    }
}

/// What a running copy does, and how it ended.
enum Run {
    Done,
    Cancelled,
    Failed(TransferProblem),
}

/// Runs `adb <args>` until it ends, calling `tick` while it runs (it returns
/// the bytes of this file done so far). A cancel kills adb.
fn run_copy(
    adb: &Adb,
    args: &[&str],
    cancel: &AtomicBool,
    tick: &mut dyn FnMut() -> Option<u64>,
    progress: &mut dyn FnMut(u64),
) -> Run {
    let mut child: Child = match adb.spawn(args) {
        Ok(child) => child,
        Err(err) => {
            tracing::warn!(%err, "cannot start adb");
            return Run::Failed(TransferProblem::Failed);
        }
    };
    let stderr = crate::adb::read_pipe(child.stderr.take());
    let stdout = crate::adb::read_pipe(child.stdout.take());
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Run::Done;
                }
                let message = format!(
                    "{} {}",
                    stdout
                        .recv_timeout(Duration::from_secs(1))
                        .unwrap_or_default(),
                    stderr
                        .recv_timeout(Duration::from_secs(1))
                        .unwrap_or_default()
                );
                tracing::warn!(message = %message.trim(), "copy failed");
                return Run::Failed(problem_from(&message));
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(%err, "cannot wait for adb");
                return Run::Failed(TransferProblem::Failed);
            }
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Run::Cancelled;
        }
        if let Some(done) = tick() {
            progress(done);
        }
        thread::sleep(POLL);
    }
}

/// Lists the files of a phone path (a file or a whole folder) with sizes.
fn expand_remote(
    adb: &Adb,
    serial: &str,
    path: &str,
) -> Result<Vec<(String, u64)>, TransferProblem> {
    let command = format!(
        "find {} -type f -exec stat -c '%s|%n' {{}} + 2>/dev/null",
        sh_quote(path)
    );
    let out = adb
        .run(&["-s", serial, "shell", &command], LIST_TIMEOUT)
        .map_err(|_| TransferProblem::PhoneGone)?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|line| {
            let (size, file) = line.split_once('|')?;
            Some((file.to_owned(), size.parse().ok()?))
        })
        .collect())
}

/// The files to pull: each selected path keeps its name (made unique in the
/// PC folder) and, for a folder, its inner layout.
fn plan_pull(selected: &[(String, Vec<(String, u64)>)], folder: &Path) -> Vec<Item> {
    let mut items = Vec::new();
    let mut taken: Vec<PathBuf> = Vec::new();
    for (root, files) in selected {
        let name = root
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("phone");
        // Unique among existing files and the other selected items.
        let mut top = crate::record::unique_path(folder, name);
        let mut n = 2;
        while taken.contains(&top) {
            top = crate::record::unique_path(folder, &format!("{name} ({n})"));
            n += 1;
        }
        taken.push(top.clone());
        for (file, size) in files {
            let relative = file.strip_prefix(root.trim_end_matches('/')).unwrap_or("");
            let mut to = top.clone();
            for part in relative.split('/').filter(|p| !p.is_empty()) {
                to.push(part);
            }
            items.push(Item {
                from: file.clone(),
                to: to.to_string_lossy().into_owned(),
                size: *size,
            });
        }
    }
    items
}

fn pull(
    adb: &Adb,
    serial: &str,
    paths: &[String],
    folder: &Path,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(TransferState),
) -> TransferState {
    let direction = TransferDirection::ToPc;
    let mut selected = Vec::new();
    for path in paths {
        match expand_remote(adb, serial, path) {
            Ok(files) => selected.push((path.clone(), files)),
            Err(problem) => return TransferState::Failed { direction, problem },
        }
    }
    let items = plan_pull(&selected, folder);
    let total: u64 = items.iter().map(|i| i.size).sum();
    let mut done = 0;
    let mut last = Instant::now();
    for item in &items {
        let to = PathBuf::from(&item.to);
        if let Some(parent) = to.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                tracing::warn!(%err, "cannot create the folder");
                return TransferState::Failed {
                    direction,
                    problem: TransferProblem::PcFolder,
                };
            }
        }
        let file = display_name(&item.from);
        let run = run_copy(
            adb,
            &["-s", serial, "pull", &item.from, &item.to],
            cancel,
            &mut || std::fs::metadata(&to).ok().map(|m| m.len()),
            &mut |file_done| {
                if last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    report(TransferState::Running {
                        direction,
                        done: done + file_done.min(item.size),
                        total,
                        file: file.clone(),
                    });
                }
            },
        );
        match run {
            Run::Done => done += item.size,
            Run::Cancelled => {
                let _ = std::fs::remove_file(&to);
                return TransferState::Cancelled { direction };
            }
            Run::Failed(problem) => {
                let _ = std::fs::remove_file(&to);
                return TransferState::Failed { direction, problem };
            }
        }
    }
    tracing::info!(files = items.len(), bytes = total, "copied from the phone");
    TransferState::Done {
        direction,
        count: items.len(),
        folder: Some(folder.to_path_buf()),
    }
}

/// Every file under the PC paths, with the phone path it goes to.
fn plan_push(files: &[PathBuf], dir: &str, existing: &[String]) -> Vec<Item> {
    fn walk(path: &Path, to: &str, items: &mut Vec<Item>) {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        if meta.is_dir() {
            let Ok(read) = std::fs::read_dir(path) else {
                return;
            };
            let mut children: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
            children.sort();
            for child in children {
                if let Some(name) = child.file_name() {
                    walk(&child, &join(to, &name.to_string_lossy()), items);
                }
            }
        } else if meta.is_file() {
            items.push(Item {
                from: path.to_string_lossy().into_owned(),
                to: to.to_owned(),
                size: meta.len(),
            });
        }
    }
    let mut taken: Vec<String> = existing.to_vec();
    let mut items = Vec::new();
    for file in files {
        let Some(name) = file.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let name = unique_name(&name, &taken);
        taken.push(name.clone());
        walk(file, &join(dir, &name), &mut items);
    }
    items
}

/// `photo.jpg` → `photo (2).jpg` while the name is taken.
fn unique_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .unwrap_or_else(|| name.to_owned())
}

fn push(
    adb: &Adb,
    serial: &str,
    files: &[PathBuf],
    dir: &str,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(TransferState),
) -> TransferState {
    let direction = TransferDirection::ToPhone;
    let existing = match list(adb, serial, dir) {
        Ok(entries) => entries.into_iter().map(|e| e.name).collect::<Vec<_>>(),
        Err(_) => {
            return TransferState::Failed {
                direction,
                problem: TransferProblem::PhoneGone,
            }
        }
    };
    let items = plan_push(files, dir, &existing);
    let total: u64 = items.iter().map(|i| i.size).sum();
    let mut done = 0;
    let mut last = Instant::now();
    for item in &items {
        let file = display_name(&item.from);
        let mut next_size_read = Instant::now() + PUSH_POLL;
        let stat = format!("stat -c %s {} 2>/dev/null", sh_quote(&item.to));
        let run = run_copy(
            adb,
            &["-s", serial, "push", &item.from, &item.to],
            cancel,
            &mut || {
                // Big files only: reading the size costs an adb call.
                if item.size < 8 * 1024 * 1024 || Instant::now() < next_size_read {
                    return None;
                }
                next_size_read = Instant::now() + PUSH_POLL;
                adb.run(&["-s", serial, "shell", &stat], QUICK_TIMEOUT)
                    .ok()
                    .and_then(|out| out.stdout.trim().parse().ok())
            },
            &mut |file_done| {
                if last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    report(TransferState::Running {
                        direction,
                        done: done + file_done.min(item.size),
                        total,
                        file: file.clone(),
                    });
                }
            },
        );
        match run {
            Run::Done => {
                done += item.size;
                report(TransferState::Running {
                    direction,
                    done,
                    total,
                    file,
                });
            }
            Run::Cancelled => {
                let rm = format!("rm -f {}", sh_quote(&item.to));
                let _ = adb.run(&["-s", serial, "shell", &rm], QUICK_TIMEOUT);
                return TransferState::Cancelled { direction };
            }
            Run::Failed(problem) => return TransferState::Failed { direction, problem },
        }
    }
    scan_media(adb, serial, &items);
    tracing::info!(files = items.len(), bytes = total, "copied to the phone");
    TransferState::Done {
        direction,
        count: items.len(),
        folder: None,
    }
}

/// Asks the phone to index new photos and videos, so the gallery shows them.
fn scan_media(adb: &Adb, serial: &str, items: &[Item]) {
    for chunk in items.chunks(40) {
        let command = chunk
            .iter()
            .map(|item| {
                format!(
                    "am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d {} >/dev/null",
                    sh_quote(&format!("file://{}", item.to))
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        if let Err(err) = adb.run(&["-s", serial, "shell", &command], LIST_TIMEOUT) {
            tracing::debug!(%err, "media scan failed");
        }
    }
}

/// The last part of a path (phone or PC).
fn display_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_is_parsed_sorted_and_hidden_files_left_out() {
        let output = "directory|3452|1739057495|/sdcard/Android\n\
                      regular file|36|1750304540|/sdcard/b.dat\n\
                      regular file|10|1750304540|/sdcard/.hidden\n\
                      directory|3452|1762073015|/sdcard/DCIM\n\
                      regular file|5|1750304540|/sdcard/A file | with bar.txt\n\
                      garbage";
        let entries = parse_listing(output);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Android", "DCIM", "A file | with bar.txt", "b.dat"]);
        assert!(entries[0].is_dir);
        assert_eq!(entries[3].size, 36);
        assert_eq!(entries[3].modified, Some(1_750_304_540));
        assert_eq!(entries[2].path, "/sdcard/A file | with bar.txt");
    }

    #[test]
    fn paths_and_quotes() {
        assert_eq!(join("/sdcard/", "DCIM"), "/sdcard/DCIM");
        assert_eq!(
            parent("/sdcard/DCIM/Camera").as_deref(),
            Some("/sdcard/DCIM")
        );
        assert_eq!(parent("/sdcard"), None);
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn names_are_made_unique() {
        let taken = vec![
            "a.jpg".to_owned(),
            "a (2).jpg".to_owned(),
            "notes".to_owned(),
        ];
        assert_eq!(unique_name("a.jpg", &taken), "a (3).jpg");
        assert_eq!(unique_name("notes", &taken), "notes (2)");
        assert_eq!(unique_name("b.jpg", &taken), "b.jpg");
    }

    #[test]
    fn pull_keeps_the_folder_layout() {
        let folder = std::env::temp_dir().join(format!("phonycut-pull-{}", std::process::id()));
        let selected = vec![
            (
                "/sdcard/DCIM/Camera".to_owned(),
                vec![
                    ("/sdcard/DCIM/Camera/a.jpg".to_owned(), 3),
                    ("/sdcard/DCIM/Camera/sub/b.mp4".to_owned(), 5),
                ],
            ),
            (
                "/sdcard/Download/c.pdf".to_owned(),
                vec![("/sdcard/Download/c.pdf".to_owned(), 7)],
            ),
        ];
        let items = plan_pull(&selected, &folder);
        let to: Vec<PathBuf> = items.iter().map(|i| PathBuf::from(&i.to)).collect();
        assert_eq!(
            to,
            [
                folder.join("Camera").join("a.jpg"),
                folder.join("Camera").join("sub").join("b.mp4"),
                folder.join("c.pdf"),
            ]
        );
    }

    #[test]
    fn adb_messages_become_problems() {
        assert_eq!(
            problem_from("adb: error: failed to copy: No space left on device"),
            TransferProblem::NoSpace
        );
        assert_eq!(
            problem_from("error: device '123' not found"),
            TransferProblem::PhoneGone
        );
        assert_eq!(
            problem_from("remote couldn't create file: Permission denied"),
            TransferProblem::Denied
        );
    }
}
