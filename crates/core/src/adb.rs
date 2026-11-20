//! Access to the bundled `adb` executable: locating it, running commands with a
//! timeout, and parsing its output.
//!
//! We never reimplement the adb protocol; every call spawns `adb` as a child
//! process.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver};

use crate::api::{DeviceInfo, DeviceState};
use crate::{paths, platform};

/// Environment variable that overrides the adb location (for development).
pub const ADB_PATH_ENV: &str = "PHONYCUT_ADB";

const START_SERVER_TIMEOUT: Duration = Duration::from_secs(20);
const QUICK_TIMEOUT: Duration = Duration::from_secs(5);
const PUSH_TIMEOUT: Duration = Duration::from_secs(30);
/// `adb pair` runs a TLS handshake over the network.
const PAIR_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_STEP: Duration = Duration::from_millis(20);
/// How long to wait for a child's output pipes to close after it exited.
const PIPE_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, thiserror::Error)]
pub enum AdbError {
    #[error("adb executable not found (searched: {searched:?})")]
    NotFound { searched: Vec<PathBuf> },
    #[error("failed to run adb: {0}")]
    Io(#[from] std::io::Error),
    #[error("`adb {args}` timed out after {timeout:?}")]
    Timeout { args: String, timeout: Duration },
    #[error("`adb {args}` failed ({status}): {output}")]
    Failed {
        args: String,
        status: String,
        output: String,
    },
}

/// Captured output of a successful adb command.
#[derive(Debug, Clone, Default)]
pub struct AdbOutput {
    pub stdout: String,
    pub stderr: String,
}

impl AdbOutput {
    fn contains(&self, needle: &str) -> bool {
        self.stdout.contains(needle) || self.stderr.contains(needle)
    }

    /// True when adb reports that a server of another version was running
    /// (typically another tool's adb on this PC) and had to be replaced.
    pub fn reports_version_conflict(&self) -> bool {
        self.contains("doesn't match this client")
    }
}

/// Result of `adb start-server`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerStart {
    /// The server was not running and this call started it.
    pub started_by_us: bool,
    /// A server of another adb version had to be killed first.
    pub version_conflict: bool,
}

/// Handle on an adb executable.
#[derive(Debug, Clone)]
pub struct Adb {
    path: PathBuf,
}

impl Adb {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Finds the bundled adb (`$PHONYCUT_ADB` overrides), see
    /// [`paths::find_vendor_file`].
    pub fn locate() -> Result<Self, AdbError> {
        let relative = Path::new("adb").join(platform::ADB_EXE);
        paths::find_vendor_file(&relative, ADB_PATH_ENV)
            .map(Self::new)
            .map_err(|e| AdbError::NotFound {
                searched: e.searched,
            })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `adb` command with piped output and no console window.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.path);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        platform::hide_console_window(&mut cmd);
        cmd
    }

    /// Runs `adb <args>`, killing it if it takes longer than `timeout`.
    pub fn run(&self, args: &[&str], timeout: Duration) -> Result<AdbOutput, AdbError> {
        let joined = args.join(" ");
        let mut child = self.command(args).spawn()?;
        let stdout = read_in_background(child.stdout.take());
        let stderr = read_in_background(child.stderr.take());

        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                // Best effort: the process may have exited in the meantime.
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdbError::Timeout {
                    args: joined,
                    timeout,
                });
            }
            thread::sleep(POLL_STEP);
        };

        // `start-server` spawns a daemon that may inherit the pipes; never
        // wait on them indefinitely.
        let output = AdbOutput {
            stdout: stdout.recv_timeout(PIPE_GRACE).unwrap_or_default(),
            stderr: stderr.recv_timeout(PIPE_GRACE).unwrap_or_default(),
        };
        if status.success() {
            Ok(output)
        } else {
            Err(AdbError::Failed {
                args: joined,
                status: status.to_string(),
                output: format!("{}{}", output.stdout, output.stderr)
                    .trim()
                    .to_owned(),
            })
        }
    }

    /// Starts the adb server if needed.
    pub fn start_server(&self) -> Result<ServerStart, AdbError> {
        let out = self.run(&["start-server"], START_SERVER_TIMEOUT)?;
        Ok(ServerStart {
            started_by_us: out.contains("daemon started successfully"),
            version_conflict: out.reports_version_conflict(),
        })
    }

    /// Stops the adb server.
    pub fn kill_server(&self) -> Result<(), AdbError> {
        self.run(&["kill-server"], QUICK_TIMEOUT).map(|_| ())
    }

    /// Lists connected devices (`adb devices -l`).
    pub fn devices(&self) -> Result<(Vec<DeviceInfo>, AdbOutput), AdbError> {
        let out = self.run(&["devices", "-l"], QUICK_TIMEOUT)?;
        Ok((parse_devices(&out.stdout), out))
    }

    /// Reads manufacturer and model from an authorized device.
    pub fn product_info(&self, serial: &str) -> Result<ProductInfo, AdbError> {
        let out = self.run(
            &[
                "-s",
                serial,
                "shell",
                "getprop ro.product.manufacturer; getprop ro.product.model; \
                 getprop ro.product.marketname",
            ],
            QUICK_TIMEOUT,
        )?;
        Ok(parse_product_info(&out.stdout))
    }

    /// Android API level (`ro.build.version.sdk`: 30 = Android 11,
    /// 33 = Android 13). `None` when it cannot be read.
    pub fn sdk_version(&self, serial: &str) -> Option<u32> {
        let out = self
            .run(
                &["-s", serial, "shell", "getprop ro.build.version.sdk"],
                QUICK_TIMEOUT,
            )
            .ok()?;
        out.stdout.trim().parse().ok()
    }

    /// Services adb sees on the local network (`adb mdns services`).
    pub fn mdns_services(&self) -> Result<Vec<MdnsService>, AdbError> {
        let out = self.run(&["mdns", "services"], QUICK_TIMEOUT)?;
        Ok(parse_mdns_services(&out.stdout))
    }

    /// Pairs with a phone offering Wi-Fi pairing at `address` (`ip:port`).
    pub fn pair(&self, address: &str, code: &str) -> Result<(), PairError> {
        match self.run(&["pair", address, code], PAIR_TIMEOUT) {
            Ok(out) if out.contains("Successfully paired") => Ok(()),
            Ok(out) => Err(pair_error(&format!("{}{}", out.stdout, out.stderr))),
            Err(AdbError::Failed { output, .. }) => Err(pair_error(&output)),
            Err(err) => Err(PairError::Adb(err)),
        }
    }

    /// True when the device is connected and authorized.
    pub fn is_ready(&self, serial: &str) -> bool {
        self.run(&["-s", serial, "get-state"], QUICK_TIMEOUT)
            .map(|out| out.stdout.trim() == "device")
            .unwrap_or(false)
    }

    /// Copies a local file to the device.
    pub fn push(&self, serial: &str, local: &Path, remote: &str) -> Result<(), AdbError> {
        let local = local.to_string_lossy();
        self.run(&["-s", serial, "push", &local, remote], PUSH_TIMEOUT)
            .map(|_| ())
    }

    /// Forwards a free local TCP port (chosen by adb) to `remote` on the
    /// device, and returns that port.
    pub fn forward_any_port(&self, serial: &str, remote: &str) -> Result<u16, AdbError> {
        let args = ["-s", serial, "forward", "tcp:0", remote];
        let out = self.run(&args, QUICK_TIMEOUT)?;
        out.stdout.trim().parse().map_err(|_| AdbError::Failed {
            args: args.join(" "),
            status: "ok".into(),
            output: format!("unexpected port: {:?}", out.stdout),
        })
    }

    /// Removes a forward created by [`Self::forward_any_port`].
    pub fn remove_forward(&self, serial: &str, port: u16) -> Result<(), AdbError> {
        let local = format!("tcp:{port}");
        self.run(
            &["-s", serial, "forward", "--remove", &local],
            QUICK_TIMEOUT,
        )
        .map(|_| ())
    }

    /// Starts a long-running `adb -s <serial> shell <args…>` with piped output.
    pub fn spawn_shell(&self, serial: &str, args: &[&str]) -> Result<Child, AdbError> {
        let mut full = vec!["-s", serial, "shell"];
        full.extend_from_slice(args);
        Ok(self.command(&full).spawn()?)
    }
}

/// Manufacturer and model reported by the device itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductInfo {
    pub manufacturer: Option<String>,
    /// Commercial name when the phone exposes one (`ro.product.marketname`,
    /// e.g. "POCO X7 Pro"), otherwise the model reference ("2412DPC0AG").
    pub model: Option<String>,
}

fn read_in_background<R: Read + Send + 'static>(pipe: Option<R>) -> Receiver<String> {
    let (tx, rx) = bounded(1);
    if let Some(mut pipe) = pipe {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
        });
    }
    rx
}

/// Why `adb pair` failed.
#[derive(Debug, thiserror::Error)]
pub enum PairError {
    #[error("the phone refused the pairing code")]
    WrongCode,
    #[error("pairing failed: {0}")]
    Other(String),
    #[error(transparent)]
    Adb(AdbError),
}

fn pair_error(output: &str) -> PairError {
    let output = output.trim();
    // "Failed: Wrong password or connection was dropped."
    if output.to_ascii_lowercase().contains("wrong password") {
        PairError::WrongCode
    } else {
        PairError::Other(output.to_owned())
    }
}

/// A service seen by adb on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsService {
    /// e.g. `adb-IR7XZT6DZPT44L99-0T2B7U`, or the name of a QR code pairing.
    pub instance: String,
    /// e.g. `_adb-tls-pairing._tcp`, `_adb-tls-connect._tcp`.
    pub service: String,
    /// `ip:port`.
    pub address: String,
}

/// mDNS service of a phone waiting to be paired.
pub const PAIRING_SERVICE: &str = "_adb-tls-pairing._tcp";

/// Parses `adb mdns services`: a title line, then `instance\tservice\tip:port`.
pub fn parse_mdns_services(output: &str) -> Vec<MdnsService> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('\t').map(str::trim);
            let (instance, service, address) = (parts.next()?, parts.next()?, parts.next()?);
            (!instance.is_empty() && service.starts_with('_') && address.contains(':')).then(|| {
                MdnsService {
                    instance: instance.to_owned(),
                    service: service.to_owned(),
                    address: address.to_owned(),
                }
            })
        })
        .collect()
}

/// Suffix of the serial adb gives a phone it connected to over Wi-Fi by
/// itself (mDNS auto-connect), e.g.
/// `adb-IR7XZT6DZPT44L99-0T2B7U._adb-tls-connect._tcp`.
const WIRELESS_SUFFIX: &str = "._adb-tls-connect._tcp";

/// True for a phone connected over Wi-Fi (mDNS name, or `ip:port`).
pub fn is_wireless(serial: &str) -> bool {
    serial.ends_with(WIRELESS_SUFFIX) || serial.parse::<std::net::SocketAddr>().is_ok()
}

/// Hardware serial inside a Wi-Fi serial (`IR7XZT6DZPT44L99` above), to
/// recognize the same phone plugged in over USB too.
pub fn wireless_hardware_serial(serial: &str) -> Option<&str> {
    let name = serial.strip_suffix(WIRELESS_SUFFIX)?.strip_prefix("adb-")?;
    let (hardware, _random) = name.rsplit_once('-')?;
    (!hardware.is_empty()).then_some(hardware)
}

/// Keeps one entry per phone: a phone both plugged in and connected over
/// Wi-Fi is listed once, by USB (faster, and it charges).
pub fn merge_transports(devices: Vec<DeviceInfo>) -> Vec<DeviceInfo> {
    let usb: Vec<String> = devices
        .iter()
        .filter(|d| !is_wireless(&d.serial))
        .map(|d| d.serial.clone())
        .collect();
    devices
        .into_iter()
        .filter(|d| {
            wireless_hardware_serial(&d.serial).is_none_or(|hw| !usb.iter().any(|u| u == hw))
        })
        .collect()
}

/// Parses the output of `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<DeviceInfo> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("List of devices")
                && !line.starts_with('*')
                && !line.starts_with("adb ")
        })
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<DeviceInfo> {
    let mut tokens = line.split_whitespace();
    let serial = tokens.next()?;
    let rest: Vec<&str> = tokens.collect();
    let first_property = rest
        .iter()
        .position(|t| is_property(t))
        .unwrap_or(rest.len());
    let state = rest[..first_property].join(" ");
    if state.is_empty() {
        return None;
    }
    let model = rest[first_property..]
        .iter()
        .find_map(|t| t.strip_prefix("model:"))
        .filter(|m| !m.is_empty())
        .map(|m| m.replace('_', " "));
    Some(DeviceInfo {
        serial: serial.to_owned(),
        state: DeviceState::from_adb(&state),
        manufacturer: None,
        model,
    })
}

fn is_property(token: &str) -> bool {
    const KEYS: [&str; 5] = ["usb:", "product:", "model:", "device:", "transport_id:"];
    KEYS.iter().any(|k| token.starts_with(k))
}

fn parse_product_info(output: &str) -> ProductInfo {
    let mut lines = output
        .lines()
        .map(str::trim)
        .map(|l| (!l.is_empty()).then(|| l.to_owned()));
    let manufacturer = lines.next().flatten();
    let model = lines.next().flatten();
    let market_name = lines.next().flatten();
    ProductInfo {
        manufacturer,
        model: market_name.or(model),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real outputs of adb 37.0.1 with a Xiaomi paired over Wi-Fi.
    const MDNS: &str = "List of discovered mdns services\n\
        adb-IR7XZT6DZPT44L99-0T2B7U\t_adb-tls-connect._tcp\t192.168.1.45:39435\n\
        adb-IR7XZT6DZPT44L99\t_adb._tcp\t192.168.1.45:5555\n\
        \n";
    const WIFI: &str = "adb-IR7XZT6DZPT44L99-0T2B7U._adb-tls-connect._tcp";

    #[test]
    fn mdns_services_are_parsed() {
        let services = parse_mdns_services(MDNS);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].instance, "adb-IR7XZT6DZPT44L99-0T2B7U");
        assert_eq!(services[0].service, "_adb-tls-connect._tcp");
        assert_eq!(services[0].address, "192.168.1.45:39435");
    }

    #[test]
    fn wireless_serials_are_recognized() {
        assert!(is_wireless(WIFI));
        assert!(is_wireless("192.168.1.45:5555"));
        assert!(!is_wireless("IR7XZT6DZPT44L99"));
        assert_eq!(wireless_hardware_serial(WIFI), Some("IR7XZT6DZPT44L99"));
        assert_eq!(wireless_hardware_serial("IR7XZT6DZPT44L99"), None);
    }

    #[test]
    fn a_phone_on_usb_and_wifi_is_listed_once() {
        let out = format!(
            "List of devices attached\n\
             IR7XZT6DZPT44L99       device product:rodin_eea model:2412DPC0AG device:rodin transport_id:5\n\
             {WIFI} device product:rodin_eea model:2412DPC0AG device:rodin transport_id:6\n"
        );
        let both = parse_devices(&out);
        assert_eq!(both.len(), 2);
        let merged = merge_transports(both.clone());
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].serial, "IR7XZT6DZPT44L99", "USB first");
        // Cable unplugged: the Wi-Fi connection takes over.
        let wifi_only = merge_transports(vec![both[1].clone()]);
        assert_eq!(wifi_only[0].serial, WIFI);
    }

    #[test]
    fn pairing_errors() {
        assert!(matches!(
            pair_error("Failed: Wrong password or connection was dropped."),
            PairError::WrongCode
        ));
        assert!(matches!(
            pair_error("error: unable to connect"),
            PairError::Other(_)
        ));
    }

    #[test]
    fn parses_typical_output() {
        let out = "List of devices attached\r\n\
            R58M12345A     device usb:1-1 product:beyond1lteeea model:SM_G973F device:beyond1 transport_id:1\r\n\
            emulator-5554  offline transport_id:2\r\n\
            0123456789ABC  unauthorized usb:1-2 transport_id:3\r\n\
            \r\n";
        let devices = parse_devices(out);
        assert_eq!(devices.len(), 3);

        assert_eq!(devices[0].serial, "R58M12345A");
        assert_eq!(devices[0].state, DeviceState::Ready);
        assert_eq!(devices[0].model.as_deref(), Some("SM G973F"));

        assert_eq!(devices[1].serial, "emulator-5554");
        assert_eq!(devices[1].state, DeviceState::Offline);
        assert_eq!(devices[1].model, None);

        assert_eq!(devices[2].state, DeviceState::Unauthorized);
    }

    #[test]
    fn parses_empty_list_and_daemon_messages() {
        let out = "* daemon not running; starting now at tcp:5037\n\
            * daemon started successfully\n\
            List of devices attached\n\n";
        assert!(parse_devices(out).is_empty());
    }

    #[test]
    fn parses_multi_word_state() {
        let out = "List of devices attached\n\
            ABC123 no permissions (missing udev rules? user is in the plugdev group); \
            see [http://developer.android.com/tools/device.html] usb:3-1 transport_id:4\n";
        let devices = parse_devices(out);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].state, DeviceState::NoPermissions);
    }

    #[test]
    fn ignores_line_without_state() {
        assert!(parse_devices("List of devices attached\nlonely\n").is_empty());
    }

    #[test]
    fn parses_product_info() {
        assert_eq!(
            parse_product_info("samsung\r\nSM-G973F\r\n"),
            ProductInfo {
                manufacturer: Some("samsung".into()),
                model: Some("SM-G973F".into()),
            }
        );
        assert_eq!(parse_product_info(""), ProductInfo::default());
        // The commercial name wins when present.
        assert_eq!(
            parse_product_info("Xiaomi\n2412DPC0AG\nPOCO X7 Pro\n"),
            ProductInfo {
                manufacturer: Some("Xiaomi".into()),
                model: Some("POCO X7 Pro".into()),
            }
        );
        assert_eq!(
            parse_product_info("\nPixel 7\n"),
            ProductInfo {
                manufacturer: None,
                model: Some("Pixel 7".into()),
            }
        );
    }

    #[test]
    fn detects_version_conflict() {
        let out = AdbOutput {
            stdout: String::new(),
            stderr: "adb server version (40) doesn't match this client (41); killing...\n".into(),
        };
        assert!(out.reports_version_conflict());
        assert!(!AdbOutput::default().reports_version_conflict());
    }
}
