//! scrcpy server launch, protocol and mirror session.

pub mod control;
pub mod protocol;
pub mod server;
pub mod session;

/// Version of the bundled `vendor/scrcpy/scrcpy-server`. Must match exactly:
/// the server refuses to run with another client version.
pub const SCRCPY_VERSION: &str = "5.0.1";
