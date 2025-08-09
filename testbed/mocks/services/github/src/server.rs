use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use axum_server::tls_rustls::RustlsConfig;
use rcgen::{Certificate, CertificateParams, KeyPair};
use std::net::SocketAddr;

use crate::fixtures::RepositoryFixture;
use crate::handlers::{health_check, get_repository, get_repository_contents, get_repository_tarball, get_repository_archive_tarball, get_raw_file};

pub struct MockServer {
    fixture: Arc<RepositoryFixture>,
}

impl MockServer {
    pub async fn new() -> anyhow::Result<Self> {
        let fixture = Arc::new(RepositoryFixture::create_test_fixture());
        Ok(Self { fixture })
    }

    pub async fn with_fixture(fixture: RepositoryFixture) -> anyhow::Result<Self> {
        let fixture = Arc::new(fixture);
        Ok(Self { fixture })
    }

    pub async fn serve(self, http_addr: &str, https_addr: &str) -> anyhow::Result<()> {
        // Generate self-signed certificate before consuming self
        let (cert_der, key_der) = self.generate_self_signed_cert()?;
        
        let app = Router::new()
            .route("/health", get(health_check))
            .route("/repos/:owner/:repo", get(get_repository))
            .route("/repos/:owner/:repo/contents/*path", get(get_repository_contents))
            .route("/repos/:owner/:repo/tarball/:ref", get(get_repository_tarball))
            .route("/:owner/:repo/raw/:ref/*path", get(get_raw_file))
            .route("/raw/:owner/:repo/:ref/*path", get(get_raw_file))
            // Add GitHub archive endpoint that gola expects
            .route("/:owner/:repo/archive/*ref", get(get_repository_archive_tarball))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(self.fixture);
            
        let config = RustlsConfig::from_der(
            vec![cert_der],
            key_der,
        ).await?;

        // Clone app for both servers
        let https_app = app.clone();
        let http_app = app;

        // Parse addresses
        let https_addr: SocketAddr = https_addr.parse()?;
        let http_addr: SocketAddr = http_addr.parse()?;
        
        tracing::info!("Starting GitHub Mock Server on HTTP {} and HTTPS {}", http_addr, https_addr);
        
        // Start HTTPS server
        let https_server = axum_server::bind_rustls(https_addr, config)
            .serve(https_app.into_make_service());

        // Start HTTP server
        let http_server = axum_server::bind(http_addr)
            .serve(http_app.into_make_service());
        
        // Run both servers concurrently
        tokio::try_join!(
            async { 
                tracing::info!("HTTPS server starting on {}", https_addr);
                https_server.await.map_err(|e| anyhow::anyhow!("HTTPS server error: {}", e)) 
            },
            async { 
                tracing::info!("HTTP server starting on {}", http_addr);
                http_server.await.map_err(|e| anyhow::anyhow!("HTTP server error: {}", e)) 
            }
        )?;
        
        Ok(())
    }

    pub async fn serve_https_only(self, https_addr: &str) -> anyhow::Result<()> {
        // Generate self-signed certificate
        let (cert_der, key_der) = self.generate_self_signed_cert()?;

        let app = Router::new()
            .route("/health", get(crate::handlers::health_check))
            .route("/repos/:owner/:repo", get(crate::handlers::get_repository))
            .route("/repos/:owner/:repo/contents/*path", get(crate::handlers::get_repository_contents))
            .route("/repos/:owner/:repo/tarball/:ref", get(crate::handlers::get_repository_tarball))
            .route("/raw/:owner/:repo/:ref/*path", get(crate::handlers::get_raw_file))
            // Add GitHub archive endpoint that gola expects
            .route("/:owner/:repo/archive/*ref", get(crate::handlers::get_repository_archive_tarball))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(self.fixture);
            
        let config = RustlsConfig::from_der(
            vec![cert_der],
            key_der,
        ).await?;

        // Parse address
        let https_addr: SocketAddr = https_addr.parse()?;
        
        tracing::info!("HTTPS server starting on {}", https_addr);
        
        // Start HTTPS server only
        axum_server::bind_rustls(https_addr, config)
            .serve(app.into_make_service())
            .await
            .map_err(|e| anyhow::anyhow!("HTTPS server error: {}", e))?;
        
        Ok(())
    }

    fn generate_self_signed_cert(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let mut params = CertificateParams::new(vec!["github.com".to_string()]);
        params.distinguished_name.push(rcgen::DnType::CommonName, "github.com");
        
        let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        params.key_pair = Some(key_pair);
        
        let cert = Certificate::from_params(params)?;
        
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        
        Ok((cert_der, key_der))
    }

    pub fn get_fixture(&self) -> Arc<RepositoryFixture> {
        self.fixture.clone()
    }
}