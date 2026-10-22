//! The relay's `--push-command`: run as `<device id hex> <kind> <token hex>`,
//! it hands the push to muster-apns over its socket and exits 0 when APNs
//! accepted it. MUSTER_APNS_SOCKET overrides the socket path.
use std::path::PathBuf;
use std::process::ExitCode;

use muster_apns::{DEFAULT_SOCKET, Request, request};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let socket = std::env::var_os("MUSTER_APNS_SOCKET")
        .map_or_else(|| PathBuf::from(DEFAULT_SOCKET), PathBuf::from);
    let answer = Request::parse(&args.join(" ")).and_then(|r| request(&socket, &r));
    match answer {
        Ok(line) if line.starts_with("ok") => ExitCode::SUCCESS,
        Ok(line) => {
            eprintln!("muster-apns-push: {line}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("muster-apns-push: {error:#}");
            ExitCode::FAILURE
        }
    }
}
