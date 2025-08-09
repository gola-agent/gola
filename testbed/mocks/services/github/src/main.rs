use github_mock::{MockServer, RepositoryFixture};
use tracing_subscriber;
use std::env;
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Check for fixtures directory
    let fixtures_path = env::var("FIXTURES_PATH").unwrap_or_else(|_| "/app/fixtures".to_string());
    
    let server = if let Ok(fixture_file) = fs::read_to_string(format!("{}/mock-repo-fixture.yaml", fixtures_path)) {
        tracing::info!("Loading fixtures from {}/mock-repo-fixture.yaml", fixtures_path);
        let fixture = RepositoryFixture::from_yaml(&fixture_file)?;
        MockServer::with_fixture(fixture).await?
    } else {
        tracing::info!("No fixture file found, using default test fixture");
        MockServer::new().await?
    };
    
    let https_addr = "0.0.0.0:443";
    
    tracing::info!("Starting GitHub Mock Server on HTTPS {}", https_addr);
    server.serve_https_only(https_addr).await?;
    
    Ok(())
}