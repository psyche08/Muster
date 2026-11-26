#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::args_os()
            .skip(1)
            .eq([std::ffi::OsString::from("--console-worker")]),
        "expected --console-worker"
    );
    musterd::run_console_worker().await
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The console worker requires Windows.");
    std::process::exit(1);
}
