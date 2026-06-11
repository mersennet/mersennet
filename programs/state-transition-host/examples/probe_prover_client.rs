use anyhow::Result;

#[cfg(all(feature = "real-sp1", not(windows)))]
use sp1_sdk::blocking::ProverClient;

#[cfg(all(feature = "real-sp1", not(windows)))]
fn main() -> Result<()> {
    eprintln!("[mersennet-sp1-probe] client:build:start");
    let _client = ProverClient::builder().cpu().build();
    eprintln!("[mersennet-sp1-probe] client:build:done");
    Ok(())
}

#[cfg(all(feature = "real-sp1", windows))]
fn main() -> Result<()> {
    anyhow::bail!(
        "real-sp1 client probing is not supported on Windows because the current sp1-sdk toolchain pulls Unix-only components"
    )
}

#[cfg(not(feature = "real-sp1"))]
fn main() -> Result<()> {
    anyhow::bail!("run this example with --features real-sp1")
}