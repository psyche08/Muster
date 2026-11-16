#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use muster_net::vnet::windows_helper;
    windows_helper::prepare_process()?;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [address, prefix, token] = args.as_slice() else {
        anyhow::bail!("muster-tun is launched by the Muster daemon");
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(windows_helper::run(
        address.parse()?,
        prefix.parse()?,
        u128::from_str_radix(token, 16)?,
    ))?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("muster-tun requires Windows");
    std::process::exit(1);
}
