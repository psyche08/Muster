#[cfg(windows)]
mod windows;

#[cfg(windows)]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    windows::run().await
}

#[cfg(not(windows))]
fn main() {
    eprintln!("muster-vnet requires Windows and Wintun");
    std::process::exit(1);
}
