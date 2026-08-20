//! AUGECOIN ops — operational platform backend entry point.

use std::error::Error;
use std::time::Duration;

use augecoin_ops::{
    config::Config, db, http, monitor::Monitor, node::NodeClient, reward::RewardSync,
    state::AppState, validator::ValidatorRepo,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "augecoin_ops=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = db::connect(&config).await?;
    let state = AppState::new(config.clone(), pool.clone());

    // Rewards sync: walk the chain and attribute block rewards to validators.
    // Only runs when the node bridge is configured (the blockchain is the
    // authority; no node means nothing to mirror).
    if config.has_node_bridge() {
        let node = NodeClient::new(
            config.node_rpc_url.clone().expect("checked"),
            config.node_admin_key.clone().expect("checked"),
        );
        let sync = RewardSync::new(
            node,
            state.rewards.clone(),
            ValidatorRepo::new(pool.clone()),
            pool.clone(),
        );
        tracing::info!("reward sync enabled (30s tick)");
        sync.spawn(Duration::from_secs(30));
    } else {
        tracing::info!("node bridge not configured; reward sync disabled");
    }

    // Presence monitor: staleness tiers + automatic PoA removal. Runs always;
    // removal only takes effect when the node bridge is available (the monitor
    // logs the failure otherwise).
    {
        let monitor = Monitor::new(pool.clone(), state.validators.clone());
        tracing::info!("presence monitor enabled (30s tick)");
        monitor.spawn(Duration::from_secs(30));
    }

    // Heartbeat retention: prune samples older than 7 days, daily.
    {
        let repo = ValidatorRepo::new(pool.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(86_400));
            loop {
                tick.tick().await;
                let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
                match repo.prune_heartbeats(cutoff).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(removed = n, "heartbeat retention"),
                    Err(e) => tracing::error!(error = %e, "heartbeat retention failed"),
                }
            }
        });
    }

    let app = http::router(state);
    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(addr = %config.listen_addr, "augecoin-ops listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
