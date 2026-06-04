mod api;
mod auth;
mod config;
mod db;
mod vpp;
mod services;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;

use services::manager::ServiceManager;
use services::impls::{PppoeService, DhcpService, DnsService, NatService, FirewallService, VpnService};

#[derive(Parser)]
#[command(name = "vectoros")]
#[command(about = "VectorOS Control Plane - VPP-based router management")]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "/etc/vectoros/config.toml")]
    config: String,

    /// Database path
    #[arg(long, default_value = "/var/lib/vectoros/vectoros.db")]
    db: String,

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

    // Initialize database
    std::fs::create_dir_all(std::path::Path::new(&cli.db).parent().unwrap())?;
    db::init(&cli.db)?;
    info!("Database initialized: {}", cli.db);

    let config = config::load(&cli.config).unwrap_or_else(|e| {
        info!("Using default config: {}", e);
        config::Config::default()
    });

    info!("Config loaded: {:?}", config);

    // Initialize Service Manager and register all services
    let service_manager = Arc::new(ServiceManager::new());

    service_manager.register(Box::new(PppoeService)).await;
    service_manager.register(Box::new(DhcpService)).await;
    service_manager.register(Box::new(DnsService)).await;
    service_manager.register(Box::new(NatService)).await;
    service_manager.register(Box::new(FirewallService)).await;
    service_manager.register(Box::new(VpnService)).await;

    // Synchronize service states with actual runtime
    service_manager.sync_all().await;

    info!("Service Manager initialized with {} services",
          service_manager.list_services().await.len());

    // Start API server
    api::start_server(&cli.api_listen, config, service_manager).await?;

    Ok(())
}
