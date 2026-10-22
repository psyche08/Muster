//! Read-only live DoH check, including on a host with poisoned system DNS.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "muster.linsheng.dev".into());
    let started = std::time::Instant::now();
    let addresses = muster_net::doh::resolve(&name, 9443).await?;
    println!(
        "DoH {name}: {addresses:?} ({} ms)",
        started.elapsed().as_millis()
    );
    Ok(())
}
