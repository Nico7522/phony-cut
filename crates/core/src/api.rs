//! Contract between `core` and the UI: commands (UI → core), events (core → UI)
//! and the shared types they carry.
//!
//! The UI owns user-facing wording: types here describe *what* happened, never
//! how to phrase it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

pub use crate::markers::{CutFile, Marker, Segment};

/// Provided by the UI: called after each event and each new video frame so
/// the UI wakes up and repaints.
pub type Notifier = Arc<dyn Fn() + Send + Sync>;

/// Requests sent by the UI to the core service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Re-read the device list now instead of waiting for the next poll.
    RefreshDevices,
    /// Start mirroring the screen of this device (stops any running mirror).
    StartMirror {
        serial: String,
        quality: VideoQuality,
    },
    /// Stop the running mirror, if any.
    StopMirror,
    /// Start recording the running mirror into a new file in `folder`
    /// (created if needed).
    StartRecording {
        folder: PathBuf,
    },
    /// Stop the recording; the file is finalized.
    StopRecording,
    /// Put a cut marker at the current position of the recording.
    MarkCut,

    // --- "Mes clips" ---
    /// List the recordings (`.mkv`) of a folder → [`Event::Recordings`].
    ListRecordings {
        folder: PathBuf,
    },
    /// Open a recording in the player (closes the previous one).
    OpenVideo {
        path: PathBuf,
    },
    Play,
    Pause,
    /// Show the frame at `time` (frame-accurate).
    Seek {
        time: Duration,
    },
    CloseVideo,
    /// Save edited segments/markers next to the video.
    SaveCuts {
        video: PathBuf,
        cuts: CutFile,
    },
    /// Export clips (one file per selected segment).
    Export(ExportRequest),
    CancelExport,
    /// Stop the service: background threads end and adb is cleaned up.
    Shutdown,
}

/// Video stream settings requested from the phone (also what gets recorded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoQuality {
    /// Longest side in pixels; 0 = the phone's native resolution.
    pub max_size: u32,
    /// Bits per second.
    pub bit_rate: u32,
    pub max_fps: u32,
}

impl Default for VideoQuality {
    fn default() -> Self {
        Self {
            max_size: 1920,
            bit_rate: 8_000_000,
            max_fps: 60,
        }
    }
}

/// Notifications sent by the core service to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The adb connection layer changed state.
    AdbStatus(AdbStatus),
    /// The list of connected devices changed (full list, not a diff).
    Devices(Vec<DeviceInfo>),
    /// Another program on this PC runs a different adb version and keeps
    /// restarting the shared adb server, which can drop the connection.
    AdbConflict,
    /// The screen mirror changed state. Frames themselves go through
    /// [`FrameSlot`], not through events.
    Mirror(MirrorState),
    /// Recording progress and result.
    Recording(RecordingState),
    /// A cut marker was saved, at `time` in the video being recorded.
    CutMarked {
        time: Duration,
        count: usize,
    },
    /// Recordings of the folder, newest first.
    Recordings(Vec<RecordingEntry>),
    Player(PlayerEvent),
    /// Edited segments could not be written next to the video.
    CutsSaveFailed {
        video: PathBuf,
    },
    Export(ExportState),
}

/// A recording file in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingEntry {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
    /// Number of cut markers saved for it.
    pub cuts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// The video is open; the first frame follows (paused).
    Loaded(LoadedVideo),
    /// Current position, sent while playing and after each seek.
    Position { time: Duration, playing: bool },
    /// Playback reached the end (paused on the last frame).
    Ended,
    /// The file could not be opened or read.
    Failed { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedVideo {
    pub path: PathBuf,
    pub duration: Duration,
    pub width: u32,
    pub height: u32,
    /// Markers and saved segments (empty if none).
    pub cuts: CutFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    pub video: PathBuf,
    /// Segments to export, in order (each becomes one `.mp4`).
    pub segments: Vec<Segment>,
    pub mode: ExportMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportMode {
    /// Stream copy: instant, the clip starts on the key frame at or before
    /// the in point (at most ~1 s earlier).
    #[default]
    Fast,
    /// Re-encoded: starts exactly on the in point, slower.
    Precise,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportState {
    /// `clip` (0-based) of `clips` is being written; `fraction` of the whole job.
    Progress {
        clip: usize,
        clips: usize,
        fraction: f32,
    },
    Finished {
        files: Vec<PathBuf>,
        folder: PathBuf,
    },
    Cancelled,
    Failed {
        problem: ExportProblem,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportProblem {
    /// No segment selected.
    NothingToExport,
    /// The recording cannot be read (moved, deleted, damaged).
    CannotRead,
    /// The clips cannot be written (folder, disk full…).
    CannotWrite,
    /// No H.264 encoder available for precise export.
    NoEncoder,
    /// An export is already running.
    Busy,
}

/// Lifecycle of a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingState {
    /// File created; waiting for the next key frame (at most ~1 s) to start.
    WaitingKeyFrame { path: PathBuf },
    /// Writing. Sent when writing starts, then about twice per second.
    Recording(RecordingInfo),
    /// The file is complete and playable.
    Finished {
        info: RecordingInfo,
        end: RecordingEnd,
    },
    /// The recording could not start or could not be completed.
    Failed {
        path: Option<PathBuf>,
        problem: RecordingProblem,
    },
}

/// Facts about a recording file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingInfo {
    pub path: PathBuf,
    /// Current size on disk.
    pub bytes: u64,
    /// Video duration (what players will show).
    pub duration: Duration,
}

/// Why a recording ended normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingEnd {
    /// The user stopped it.
    Requested,
    /// The mirror stopped (phone unplugged…); the file was still finalized.
    MirrorStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingProblem {
    /// No mirror is running, there is nothing to record.
    NotMirroring,
    /// A recording is already running.
    AlreadyRecording,
    /// The output folder or file cannot be created.
    CannotCreateFile,
    /// Writing failed (disk full, drive removed…).
    WriteFailed,
    /// Stopped before the first key frame: nothing was recorded (file removed).
    NoVideo,
}

/// Lifecycle of the screen mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorState {
    /// Pushing and starting the scrcpy server.
    Starting { serial: String },
    /// Video is flowing. Sent again when the size changes (rotation).
    Running {
        serial: String,
        device_name: String,
        width: u32,
        height: u32,
    },
    /// The mirror ended.
    Stopped { serial: String, end: MirrorEnd },
}

/// Why a mirror ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorEnd {
    /// Requested by the UI (or app shutdown).
    Requested,
    /// The phone was unplugged or stopped answering.
    Disconnected,
    /// Something went wrong while the phone is still connected.
    Failed,
}

/// A decoded video frame, tightly packed RGBA8 (`width * height * 4` bytes).
#[derive(Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Device presentation time in microseconds.
    pub pts_us: Option<u64>,
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pts_us", &self.pts_us)
            .finish_non_exhaustive()
    }
}

/// Holds only the most recent frame: the UI always shows the latest image and
/// never accumulates a backlog (which would add latency).
#[derive(Debug, Default)]
pub struct FrameSlot {
    latest: Mutex<Option<VideoFrame>>,
}

impl FrameSlot {
    /// Replaces the pending frame. Returns `true` when an older frame was
    /// dropped before the UI displayed it (the UI is not keeping up).
    pub fn put(&self, frame: VideoFrame) -> bool {
        // A poisoned lock only means another thread panicked mid-swap of an
        // Option; the data is still valid.
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(frame)
            .is_some()
    }

    /// Takes the pending frame, if any.
    pub fn take(&self) -> Option<VideoFrame> {
        self.latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// State of the adb layer (our bundled `adb.exe` and its server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbStatus {
    /// Locating adb and starting its server.
    Starting,
    /// adb works; the device list is meaningful.
    Ready,
    /// adb cannot be used; see the problem.
    Problem(AdbProblem),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbProblem {
    /// The bundled adb executable is missing (broken installation).
    AdbMissing,
    /// adb is present but its server cannot be started or reached
    /// (e.g. port 5037 taken by another program). Retried automatically.
    ServerFailed,
}

/// Connection state of a device, as reported by `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    /// Connected and authorized (`device`).
    Ready,
    /// USB debugging not yet accepted on the phone.
    Unauthorized,
    /// The phone is showing the authorization prompt.
    Authorizing,
    /// Transport exists but the device does not answer.
    Offline,
    /// Connection in progress.
    Connecting,
    /// The OS refuses access to the USB device.
    NoPermissions,
    /// Any other state (`recovery`, `bootloader`, `sideload`, …), verbatim.
    Other(String),
}

impl DeviceState {
    /// Parses the state column of `adb devices`.
    pub fn from_adb(state: &str) -> Self {
        match state {
            "device" => Self::Ready,
            "unauthorized" => Self::Unauthorized,
            "authorizing" => Self::Authorizing,
            "offline" => Self::Offline,
            "connecting" => Self::Connecting,
            s if s.starts_with("no permissions") => Self::NoPermissions,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// A device seen by adb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// adb serial, used to address the device (`adb -s <serial>`).
    pub serial: String,
    pub state: DeviceState,
    /// Manufacturer (`ro.product.manufacturer`), when known.
    pub manufacturer: Option<String>,
    /// Model name, when known.
    pub model: Option<String>,
}

impl DeviceInfo {
    /// Human-friendly name, e.g. "Samsung SM-G973F" or "Pixel 7".
    /// Falls back to the serial when nothing better is known.
    pub fn display_name(&self) -> String {
        match (&self.manufacturer, &self.model) {
            (Some(maker), Some(model))
                if !model.to_lowercase().starts_with(&maker.to_lowercase()) =>
            {
                format!("{} {model}", capitalize(maker))
            }
            (_, Some(model)) => model.clone(),
            (Some(maker), None) => capitalize(maker),
            (None, None) => self.serial.clone(),
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(maker: Option<&str>, model: Option<&str>) -> DeviceInfo {
        DeviceInfo {
            serial: "SERIAL1".into(),
            state: DeviceState::Ready,
            manufacturer: maker.map(Into::into),
            model: model.map(Into::into),
        }
    }

    #[test]
    fn display_name_combines_manufacturer_and_model() {
        assert_eq!(
            device(Some("samsung"), Some("SM-G973F")).display_name(),
            "Samsung SM-G973F"
        );
    }

    #[test]
    fn display_name_avoids_repeating_manufacturer() {
        assert_eq!(
            device(Some("OnePlus"), Some("OnePlus 9")).display_name(),
            "OnePlus 9"
        );
    }

    #[test]
    fn display_name_falls_back() {
        assert_eq!(device(None, Some("Pixel 7")).display_name(), "Pixel 7");
        assert_eq!(device(Some("xiaomi"), None).display_name(), "Xiaomi");
        assert_eq!(device(None, None).display_name(), "SERIAL1");
    }

    #[test]
    fn frame_slot_keeps_only_latest() {
        let slot = FrameSlot::default();
        assert!(slot.take().is_none());
        let frame = |pts| VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
            pts_us: Some(pts),
        };
        assert!(!slot.put(frame(1)));
        assert!(slot.put(frame(2)), "frame 1 dropped");
        assert!(slot.put(frame(3)), "frame 2 dropped");
        assert_eq!(slot.take().and_then(|f| f.pts_us), Some(3));
        assert!(slot.take().is_none());
    }

    #[test]
    fn state_parsing() {
        assert_eq!(DeviceState::from_adb("device"), DeviceState::Ready);
        assert_eq!(
            DeviceState::from_adb("unauthorized"),
            DeviceState::Unauthorized
        );
        assert_eq!(DeviceState::from_adb("offline"), DeviceState::Offline);
        assert_eq!(
            DeviceState::from_adb("no permissions (user in plugdev group)"),
            DeviceState::NoPermissions
        );
        assert_eq!(
            DeviceState::from_adb("recovery"),
            DeviceState::Other("recovery".into())
        );
    }
}
