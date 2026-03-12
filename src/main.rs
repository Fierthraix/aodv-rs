use aodv::{Config, run_daemon};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_cli()?;
    run_daemon(config).await?;
    Ok(())
}
