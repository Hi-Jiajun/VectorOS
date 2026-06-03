mod api;
mod config;
mod vpp;
mod services;

use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "vectoros")]
#[command(about = "VectorOS Control Plane - VPP-based router management")]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "/etc/vectoros/config.toml")]
    config: String,

    /// Listen address for API server
    #[arg(long, default_value = "0.0.0.0:8080")]
    api_listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    info!("Starting VectorOS Control Plane");

    let config = config::load(&cli.config).unwrap_or_else(|e| {
        info!("Using default config: {}", e);
        config::Config::default()
    });

    info!("Config loaded: {:?}", config);

    // Start API server
    api::start_server(&cli.api_listen, config).await?;

    Ok(())
}
