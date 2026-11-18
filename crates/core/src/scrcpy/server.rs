//! Pushing and starting the scrcpy server on the device, and connecting to it.

use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adb::{Adb, AdbError};
use crate::api::VideoQuality;
use crate::paths;
use crate::scrcpy::protocol::{self, ProtocolError};
use crate::scrcpy::SCRCPY_VERSION;

/// Environment variable that overrides the server file location (development).
pub const SERVER_PATH_ENV: &str = "PHONECUT_SCRCPY_SERVER";
/// Where the server is pushed on the device (same path as scrcpy itself).
pub const DEVICE_SERVER_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Same values as the official client: 100 attempts, 100 ms apart.
const CONNECT_ATTEMPTS: u32 = 100;
const CONNECT_DELAY: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Stream settings sent to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    /// Longest side of the video in pixels (0 = native resolution).
    pub max_size: u32,
    /// Video bit rate in bits per second.
    pub video_bit_rate: u32,
    pub max_fps: u32,
    /// Requested key frame interval in seconds (cuts are aligned on key frames).
    pub key_frame_interval_s: u32,
    /// Open the control socket (mouse and keyboard).
    pub control: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            max_size: 1920,
            video_bit_rate: 8_000_000,
            max_fps: 60,
            key_frame_interval_s: 1,
            control: true,
        }
    }
}

/// Sockets of a connected server.
pub struct Connection {
    /// Device meta, codec, session and media packets.
    pub video: TcpStream,
    /// Control messages (PC → phone) and device messages (phone → PC).
    pub control: Option<TcpStream>,
}

impl From<VideoQuality> for ServerOptions {
    fn from(quality: VideoQuality) -> Self {
        Self {
            max_size: quality.max_size,
            video_bit_rate: quality.bit_rate,
            max_fps: quality.max_fps,
            ..Self::default()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("scrcpy server file not found: {0}")]
    FileNotFound(#[from] paths::VendorFileNotFound),
    #[error("adb: {0}")]
    Adb(#[from] AdbError),
    #[error("the scrcpy server exited before accepting the connection")]
    ServerExited,
    #[error("could not connect to the scrcpy server")]
    ConnectFailed,
    #[error("stopped")]
    Stopped,
    #[error("protocol: {0}")]
    Protocol(#[from] ProtocolError),
}

/// Finds the bundled `vendor/scrcpy/scrcpy-server`.
pub fn locate_server_file() -> Result<PathBuf, ServerError> {
    let relative = Path::new("scrcpy").join("scrcpy-server");
    Ok(paths::find_vendor_file(&relative, SERVER_PATH_ENV)?)
}

/// Random 31-bit id, so that several clients do not collide on the device.
pub fn random_scid() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32).rotate_left(16))
        .unwrap_or(0);
    (nanos ^ std::process::id().rotate_left(8)) & 0x7fff_ffff
}

/// Name of the abstract socket the server listens on (`scrcpy_%08x`).
pub fn socket_name(scid: u32) -> String {
    format!("scrcpy_{scid:08x}")
}

/// Arguments after `adb shell` that start the server.
pub fn server_args(scid: u32, options: &ServerOptions) -> Vec<String> {
    let mut args = vec![
        format!("CLASSPATH={DEVICE_SERVER_PATH}"),
        "app_process".into(),
        "/".into(),
        "com.genymobile.scrcpy.Server".into(),
        SCRCPY_VERSION.into(),
        format!("scid={scid:08x}"),
        "log_level=info".into(),
        "tunnel_forward=true".into(),
        "audio=false".into(),
        "video_codec=h264".into(),
        format!("video_bit_rate={}", options.video_bit_rate),
        format!("max_fps={}", options.max_fps),
        format!(
            "video_codec_options=i-frame-interval:int={}",
            options.key_frame_interval_s
        ),
    ];
    if options.max_size > 0 {
        args.push(format!("max_size={}", options.max_size));
    }
    if options.control {
        // Control is on by default on the server. The phone clipboard is
        // not synchronized to the PC (not handled yet): no device messages.
        args.push("clipboard_autosync=false".into());
    } else {
        args.push("control=false".into());
    }
    args
}

/// Server process running through `adb shell`, plus its port forward.
/// Dropping it kills the process and removes the forward.
pub struct RunningServer {
    adb: Adb,
    serial: String,
    pub port: u16,
    child: Child,
}

impl RunningServer {
    /// Pushes the server, sets up the forward tunnel and starts the server.
    pub fn start(adb: &Adb, serial: &str, options: &ServerOptions) -> Result<Self, ServerError> {
        let server_file = locate_server_file()?;
        adb.push(serial, &server_file, DEVICE_SERVER_PATH)?;

        let scid = random_scid();
        let port = adb.forward_any_port(serial, &format!("localabstract:{}", socket_name(scid)))?;

        let args = server_args(scid, options);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut child = match adb.spawn_shell(serial, &arg_refs) {
            Ok(child) => child,
            Err(err) => {
                let _ = adb.remove_forward(serial, port);
                return Err(err.into());
            }
        };
        log_output(child.stdout.take());
        log_output(child.stderr.take());
        tracing::info!(port, scid = format!("{scid:08x}"), "scrcpy server started");

        Ok(Self {
            adb: adb.clone(),
            serial: serial.to_owned(),
            port,
            child,
        })
    }

    /// Connects to the server, retrying while it starts up, consumes the
    /// dummy byte, then opens the control socket if enabled. `stop` aborts
    /// the wait.
    ///
    /// Order matters (scrcpy `DesktopConnection.open`): the server accepts
    /// the video socket, then the control socket, and only then sends the
    /// device name on the video socket.
    pub fn connect(&mut self, stop: &AtomicBool, control: bool) -> Result<Connection, ServerError> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        for _ in 0..CONNECT_ATTEMPTS {
            if stop.load(Ordering::Relaxed) {
                return Err(ServerError::Stopped);
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                tracing::warn!(%status, "scrcpy server exited early");
                return Err(ServerError::ServerExited);
            }
            // With a forward tunnel, connect() succeeds as soon as adb listens;
            // only the dummy byte proves the server is there.
            if let Ok(mut video) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                video.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
                if protocol::read_dummy_byte(&mut video).is_ok() {
                    let control = if control {
                        // No dummy byte on the following sockets.
                        let socket = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
                        socket.set_nodelay(true)?;
                        Some(socket)
                    } else {
                        None
                    };
                    return Ok(Connection { video, control });
                }
            }
            thread::sleep(CONNECT_DELAY);
        }
        Err(ServerError::ConnectFailed)
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Err(err) = self.adb.remove_forward(&self.serial, self.port) {
            tracing::debug!(%err, "could not remove adb forward");
        }
    }
}

impl From<std::io::Error> for ServerError {
    fn from(err: std::io::Error) -> Self {
        Self::Protocol(err.into())
    }
}

/// Forwards the server's console output to our log.
fn log_output<R: Read + Send + 'static>(pipe: Option<R>) {
    let Some(pipe) = pipe else { return };
    let spawned = thread::Builder::new()
        .name("scrcpy-server-log".into())
        .spawn(move || {
            for line in BufReader::new(pipe).lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        tracing::info!(target: "scrcpy_server", "{}", line.trim_end());
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start server log thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_name_is_zero_padded_hex() {
        assert_eq!(socket_name(0x1234), "scrcpy_00001234");
        assert_eq!(socket_name(0x7fff_ffff), "scrcpy_7fffffff");
    }

    #[test]
    fn scid_fits_in_31_bits() {
        assert!(random_scid() <= 0x7fff_ffff);
    }

    #[test]
    fn server_args_match_protocol() {
        let args = server_args(0xabc, &ServerOptions::default());
        assert_eq!(args[0], "CLASSPATH=/data/local/tmp/scrcpy-server.jar");
        assert_eq!(args[3], "com.genymobile.scrcpy.Server");
        assert_eq!(args[4], SCRCPY_VERSION);
        for expected in [
            "scid=00000abc",
            "tunnel_forward=true",
            "audio=false",
            "clipboard_autosync=false",
            "video_codec=h264",
            "max_size=1920",
            "video_codec_options=i-frame-interval:int=1",
        ] {
            assert!(args.iter().any(|a| a == expected), "missing {expected}");
        }
    }

    #[test]
    fn native_resolution_omits_max_size() {
        let options = ServerOptions {
            max_size: 0,
            ..ServerOptions::default()
        };
        assert!(!server_args(1, &options)
            .iter()
            .any(|a| a.starts_with("max_size")));
    }

    #[test]
    fn control_can_be_disabled() {
        let options = ServerOptions {
            control: false,
            ..ServerOptions::default()
        };
        let args = server_args(1, &options);
        assert!(args.iter().any(|a| a == "control=false"));
        assert!(!args.iter().any(|a| a.starts_with("clipboard_autosync")));
    }
}
