//! Cut markers and clip segments, stored next to the video in a JSON sidecar
//! (`<video stem>.markers.json`).
//!
//! Markers are cut points set while recording. Segments are what will be
//! exported: by default the pieces between markers, then adjusted (in/out
//! points, name, selection) in the "Mes clips" screen.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
/// Shortest segment allowed (ms).
pub const MIN_SEGMENT_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Marker {
    /// Position in the video, milliseconds from the start.
    pub time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub name: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// Selected for export.
    pub export: bool,
}

impl Segment {
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }

    /// Moves the in point, keeping at least [`MIN_SEGMENT_MS`] before the out point.
    pub fn set_start(&mut self, ms: u64) {
        self.start_ms = ms.min(self.end_ms.saturating_sub(MIN_SEGMENT_MS));
    }

    /// Moves the out point, keeping at least [`MIN_SEGMENT_MS`] after the in point,
    /// and not past the end of the video.
    pub fn set_end(&mut self, ms: u64, video_ms: u64) {
        let min = self.start_ms + MIN_SEGMENT_MS;
        self.end_ms = ms.clamp(min.min(video_ms), video_ms.max(min));
    }
}

/// Content of the sidecar file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutFile {
    pub version: u32,
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Edited segments; `None` until the user edits them (then derived from
    /// the markers).
    #[serde(default)]
    pub segments: Option<Vec<Segment>>,
}

impl Default for CutFile {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            markers: Vec::new(),
            segments: None,
        }
    }
}

/// `PhoneCut 2026-11-17 17-22-45.mkv` → `PhoneCut 2026-11-17 17-22-45.markers.json`
pub fn sidecar_path(video: &Path) -> PathBuf {
    video.with_extension("markers.json")
}

impl CutFile {
    /// Loads the sidecar of `video`; a missing file means "no markers".
    pub fn load(video: &Path) -> io::Result<Self> {
        match fs::read_to_string(sidecar_path(video)) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes the sidecar atomically (temporary file + rename), so a crash
    /// never leaves a half-written file.
    pub fn save(&self, video: &Path) -> io::Result<()> {
        let path = sidecar_path(video);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &path)
    }

    /// Segments to show: the edited ones, or the default split.
    pub fn segments_or_default(&self, video_ms: u64) -> Vec<Segment> {
        self.segments
            .clone()
            .unwrap_or_else(|| default_segments(&self.markers, video_ms))
    }
}

/// Splits the video at each marker: `[0, m1]`, `[m1, m2]`, …, `[mn, end]`.
/// Markers too close to each other or to the ends are ignored.
pub fn default_segments(markers: &[Marker], video_ms: u64) -> Vec<Segment> {
    let mut cuts: Vec<u64> = markers.iter().map(|m| m.time_ms).collect();
    cuts.sort_unstable();
    let mut bounds = vec![0];
    for cut in cuts {
        let last = bounds.last().copied().unwrap_or(0);
        if cut >= last + MIN_SEGMENT_MS && cut + MIN_SEGMENT_MS <= video_ms {
            bounds.push(cut);
        }
    }
    bounds.push(video_ms);
    bounds
        .windows(2)
        .enumerate()
        .map(|(i, w)| Segment {
            name: format!("Clip {}", i + 1),
            start_ms: w[0],
            end_ms: w[1],
            export: true,
        })
        .collect()
}

/// Makes a segment name safe as a file name on Windows (and elsewhere).
pub fn file_safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    let upper = trimmed.to_uppercase();
    if trimmed.is_empty()
        || reserved.contains(&upper.as_str())
        || ((upper.starts_with("COM") || upper.starts_with("LPT")) && upper.len() == 4)
    {
        format!("_{trimmed}")
    } else {
        trimmed.chars().take(100).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(ms: u64) -> Marker {
        Marker { time_ms: ms }
    }

    #[test]
    fn no_marker_gives_one_segment() {
        let segs = default_segments(&[], 10_000);
        assert_eq!(segs.len(), 1);
        assert_eq!((segs[0].start_ms, segs[0].end_ms), (0, 10_000));
        assert_eq!(segs[0].name, "Clip 1");
    }

    #[test]
    fn markers_split_in_order() {
        let segs = default_segments(&[m(7_000), m(3_000)], 10_000);
        let ranges: Vec<_> = segs.iter().map(|s| (s.start_ms, s.end_ms)).collect();
        assert_eq!(ranges, vec![(0, 3_000), (3_000, 7_000), (7_000, 10_000)]);
        assert!(segs.iter().all(|s| s.export));
        assert_eq!(segs[2].name, "Clip 3");
    }

    #[test]
    fn markers_too_close_are_ignored() {
        // 100 ms from the start, 200 ms after another, 100 ms before the end.
        let segs = default_segments(&[m(100), m(3_000), m(3_200), m(9_900)], 10_000);
        let ranges: Vec<_> = segs.iter().map(|s| (s.start_ms, s.end_ms)).collect();
        assert_eq!(ranges, vec![(0, 3_000), (3_000, 10_000)]);
    }

    #[test]
    fn in_out_points_are_clamped() {
        let mut seg = Segment {
            name: "a".into(),
            start_ms: 1_000,
            end_ms: 5_000,
            export: true,
        };
        seg.set_start(4_900);
        assert_eq!(seg.start_ms, 4_500, "keeps the minimum length");
        seg.set_start(0);
        seg.set_end(20_000, 8_000);
        assert_eq!(seg.end_ms, 8_000, "not past the end of the video");
        seg.set_end(100, 8_000);
        assert_eq!(seg.end_ms, MIN_SEGMENT_MS);
        assert_eq!(seg.duration_ms(), MIN_SEGMENT_MS);
    }

    #[test]
    fn sidecar_round_trip() {
        let dir = std::env::temp_dir().join(format!("phonecut-markers-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let video = dir.join("PhoneCut 2026-11-17 17-22-45.mkv");
        assert_eq!(
            sidecar_path(&video),
            dir.join("PhoneCut 2026-11-17 17-22-45.markers.json")
        );
        assert_eq!(CutFile::load(&video).unwrap(), CutFile::default());

        let mut file = CutFile::default();
        file.markers.push(m(1_234));
        file.save(&video).unwrap();
        assert_eq!(CutFile::load(&video).unwrap(), file);

        file.segments = Some(default_segments(&file.markers, 5_000));
        file.save(&video).unwrap();
        let loaded = CutFile::load(&video).unwrap();
        assert_eq!(
            loaded.segments_or_default(5_000),
            file.segments.clone().unwrap()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn older_files_without_segments_load() {
        let file: CutFile =
            serde_json::from_str(r#"{"version":1,"markers":[{"time_ms":42}]}"#).unwrap();
        assert_eq!(file.markers, vec![m(42)]);
        assert_eq!(file.segments, None);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_safe_name("Mon clip: top/flop?"), "Mon clip_ top_flop_");
        assert_eq!(file_safe_name("  fin.  "), "fin");
        assert_eq!(file_safe_name("con"), "_con");
        assert_eq!(file_safe_name("COM1"), "_COM1");
        assert_eq!(file_safe_name(""), "_");
        assert_eq!(file_safe_name("Été à la plage"), "Été à la plage");
    }
}
