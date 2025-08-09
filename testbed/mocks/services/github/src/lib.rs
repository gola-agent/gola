//! Mock GitHub API server for testing configuration loading from repositories
//!
//! This crate provides a lightweight GitHub API simulator for testing scenarios
//! where agents load configurations from version-controlled sources. The design
//! philosophy emphasizes test isolation and determinism by eliminating dependencies
//! on external services during development and CI/CD. By mocking GitHub interactions,
//! tests run faster, more reliably, and without rate limiting concerns, enabling
//! comprehensive testing of git-based configuration management.

pub mod fixtures;
pub mod server;
pub mod handlers;

pub use server::MockServer;
pub use fixtures::{Repository, RepositoryFixture, FileContent};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn test_fixture_creation() {
        let fixture = RepositoryFixture::create_test_fixture();
        let repo = fixture.get_repository("testuser", "test-repo");
        
        assert!(repo.is_some());
        let repo = repo.unwrap();
        assert_eq!(repo.name, "test-repo");
        assert_eq!(repo.owner, "testuser");
        assert!(repo.files.contains_key("gola.yaml"));
        assert!(repo.files.contains_key("README.md"));
    }

    #[test]
    fn test_fixture_file_content() {
        let fixture = RepositoryFixture::create_test_fixture();
        let repo = fixture.get_repository("testuser", "test-repo").unwrap();
        
        let gola_yaml = repo.files.get("gola.yaml").unwrap();
        assert!(gola_yaml.content.contains("agent:"));
        assert!(gola_yaml.content.contains("name: \"Test Agent\""));
        assert_eq!(gola_yaml.encoding, "utf-8");
    }

    #[test]
    fn test_nonexistent_repository() {
        let fixture = RepositoryFixture::new();
        let repo = fixture.get_repository("nonexistent", "repo");
        assert!(repo.is_none());
    }

    #[tokio::test]
    async fn test_https_connectivity() {
        // Initialize crypto provider for rustls
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        
        // Create a mock server with test fixture
        let server = MockServer::new().await.expect("Failed to create mock server");
        
        // Start server on fixed ports for testing
        let http_addr = "127.0.0.1:8080";
        let https_addr = "127.0.0.1:8443";
        
        // Start server in background
        let server_handle = tokio::spawn(async move {
            server.serve(http_addr, https_addr).await
        });
        
        // Give server time to start
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Create HTTPS client that accepts self-signed certificates
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");
        
        // Test HTTPS connectivity
        let response = timeout(
            Duration::from_secs(10),
            client.get("https://127.0.0.1:8443/health").send()
        ).await;
        
        // Cleanup
        server_handle.abort();
        
        match response {
            Ok(Ok(resp)) => {
                assert!(resp.status().is_success(), "HTTPS health check failed: {}", resp.status());
                println!("✅ HTTPS connectivity test passed");
            }
            Ok(Err(e)) => {
                panic!("HTTPS request failed: {}", e);
            }
            Err(_) => {
                panic!("HTTPS request timed out");
            }
        }
    }
    
    #[tokio::test] 
    async fn test_repository_tarball_https() {
        // Initialize crypto provider for rustls
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        
        // Create a mock server with test fixture
        let server = MockServer::new().await.expect("Failed to create mock server");
        
        // Start server in background on fixed ports
        let http_addr = "127.0.0.1:8081";
        let https_addr = "127.0.0.1:8444";
        
        let server_handle = tokio::spawn(async move {
            server.serve(http_addr, https_addr).await
        });
        
        // Give server time to start
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Create HTTPS client that accepts self-signed certificates
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");
        
        // Test tarball download via HTTPS (using the correct testuser/gola-config repo)
        let response = timeout(
            Duration::from_secs(10),
            client.get("https://127.0.0.1:8444/testuser/gola-config/archive/main.tar.gz").send()
        ).await;
        
        // Cleanup
        server_handle.abort();
        
        match response {
            Ok(Ok(resp)) => {
                assert!(resp.status().is_success(), "HTTPS tarball request failed: {}", resp.status());
                
                // Verify it's actually a tar.gz file
                let content_type = resp.headers().get("content-type");
                if let Some(ct) = content_type {
                    assert!(ct.to_str().unwrap().contains("application/"), "Expected tar content type");
                }
                
                println!("✅ HTTPS tarball download test passed");
            }
            Ok(Err(e)) => {
                panic!("HTTPS tarball request failed: {}", e);
            }
            Err(_) => {
                panic!("HTTPS tarball request timed out");
            }
        }
    }
}