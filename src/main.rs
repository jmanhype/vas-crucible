use std::net::SocketAddr;

use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};
use vas_crucible::sidecar::{SidecarConfig, SidecarRuntime};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let mut args = std::env::args().skip(1);
    let mut listen = String::from("0.0.0.0:50051");
    let mut hs256_secret = String::from("development-secret");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                if let Some(value) = args.next() {
                    listen = value;
                }
            }
            "--hs256-secret" => {
                if let Some(value) = args.next() {
                    hs256_secret = value;
                }
            }
            _ => {}
        }
    }

    let addr: SocketAddr = listen.parse()?;
    let runtime = SidecarRuntime::new(SidecarConfig::new_hs256(addr, hs256_secret))?;
    runtime.serve().await
}

