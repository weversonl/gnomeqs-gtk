#[macro_use]
extern crate log;

use gnomeqs_core::RQS;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if std::env::var("RUST_LOG").is_err() {
        unsafe {
            std::env::set_var(
                "RUST_LOG",
                "TRACE,mdns_sd=ERROR,polling=ERROR,neli=ERROR,bluez_async=ERROR",
            );
        }
    }

    tracing_subscriber::fmt::init();

    let mut rqs = RQS::default();
    rqs.run().await?;

    let discovery_channel = broadcast::channel(10);
    rqs.discovery(discovery_channel.0)?;

    let _ = tokio::signal::ctrl_c().await;
    info!("Stopping service.");
    rqs.stop().await;

    Ok(())
}
