//! Integration tests for the binary installation system

use crate::installation::binary::{InstallationOrchestrator, DockerManagerImpl};
use crate::installation::traits::{BinaryManager, DockerManager, DockerRegistry, SourceBuildConfig};
use std::collections::HashMap;
use tempfile::tempdir;
use tokio;

#[tokio::test]
async fn test_orchestrator_creation_and_basic_functionality() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Test basic orchestrator creation
    let orchestrator = InstallationOrchestrator::new(installation_dir.clone()).await.unwrap();
    
    // Test installation directory
    assert_eq!(orchestrator.get_installation_dir(), installation_dir);
    
    // Test that no binaries are initially installed
    let binaries = orchestrator.find_binary("nonexistent-binary").await;
    assert!(binaries.is_none());
}

#[tokio::test]
async fn test_orchestrator_with_github_strategy() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create orchestrator with GitHub strategy
    let orchestrator = InstallationOrchestrator::new(installation_dir)
        .await
        .unwrap()
        .add_github_strategy("ripgrep-all".to_string(), "rga".to_string());
    
    // Test that strategy was added
    let stats = orchestrator.get_installation_stats().await.unwrap();
    assert_eq!(stats.get("available_strategies"), Some(&"1".to_string()));
    assert_eq!(stats.get("strategy_names"), Some(&"github-releases".to_string()));
}

#[tokio::test]
async fn test_orchestrator_with_multiple_strategies() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create orchestrator with multiple strategies
    let result = InstallationOrchestrator::new(installation_dir)
        .await
        .unwrap()
        .add_github_strategy("owner".to_string(), "repo".to_string())
        .add_docker_strategy(
            "owner".to_string(),
            "repo".to_string(),
            vec![DockerRegistry::DockerHub, DockerRegistry::GitHubContainerRegistry]
        )
        .await;
    
    // This test depends on Docker availability
    match result {
        Ok(orchestrator) => {
            let stats = orchestrator.get_installation_stats().await.unwrap();
            // Should have at least GitHub strategy, possibly Docker strategy too
            let strategy_count: usize = stats.get("available_strategies").unwrap().parse().unwrap();
            assert!(strategy_count >= 1);
        }
        Err(e) => {
            // Docker not available, which is expected in some CI environments
            println!("Docker not available, skipping Docker strategy test: {}", e);
        }
    }
}

#[tokio::test]
async fn test_source_building_strategy_creation() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create source build configuration
    let source_config = SourceBuildConfig {
        repository: "https://github.com/BurntSushi/ripgrep".to_string(),
        git_ref: Some("main".to_string()),
        build_image: "rust:latest".to_string(),
        build_command: "cargo build --release".to_string(),
        binary_path: "target/release/rg".to_string(),
        build_env: HashMap::new(),
    };
    
    // Test creating orchestrator with source building
    let result = InstallationOrchestrator::new(installation_dir)
        .await
        .unwrap()
        .add_source_strategy(source_config)
        .await;
    
    match result {
        Ok(orchestrator) => {
            let stats = orchestrator.get_installation_stats().await.unwrap();
            assert_eq!(stats.get("available_strategies"), Some(&"1".to_string()));
            assert_eq!(stats.get("strategy_names"), Some(&"source-build".to_string()));
        }
        Err(e) => {
            // Docker not available, which is expected in some CI environments
            println!("Docker not available, skipping source build strategy test: {}", e);
        }
    }
}

#[tokio::test]
async fn test_default_orchestrator_creation() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    let result = InstallationOrchestrator::create_default(
        installation_dir,
        "BurntSushi".to_string(),
        "ripgrep".to_string()
    ).await;
    
    match result {
        Ok(orchestrator) => {
            let stats = orchestrator.get_installation_stats().await.unwrap();
            let strategy_count: usize = stats.get("available_strategies").unwrap().parse().unwrap();
            // Should have at least GitHub strategy
            assert!(strategy_count >= 1);
            
            // Should include GitHub strategy
            let strategy_names = stats.get("strategy_names").unwrap();
            assert!(strategy_names.contains("github-releases"));
        }
        Err(e) => {
            // Docker not available, but GitHub strategy should still work
            println!("Some strategies not available (likely Docker): {}", e);
        }
    }
}

#[tokio::test]
async fn test_binary_installation_workflow() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create a simple GitHub-only orchestrator
    let orchestrator = InstallationOrchestrator::new(installation_dir.clone())
        .await
        .unwrap()
        .add_github_strategy("BurntSushi".to_string(), "ripgrep".to_string());
    
    // Test that ripgrep is not initially installed
    let binary_path = orchestrator.find_binary("rg").await;
    assert!(binary_path.is_none());
    
    // Test installation stats
    let stats = orchestrator.get_installation_stats().await.unwrap();
    assert!(stats.contains_key("installation_directory"));
    assert!(stats.contains_key("available_strategies"));
    
    // Note: We don't actually try to install ripgrep in tests to avoid
    // depending on external network resources and taking too long
}

#[tokio::test]
async fn test_docker_manager_creation() {
    let result = DockerManagerImpl::new(300).await;
    
    match result {
        Ok(manager) => {
            // Test that Docker is available
            let available = manager.is_available().await;
            println!("Docker available: {}", available);
            
            // Test basic functionality (other methods are private/internal)
        }
        Err(e) => {
            println!("Docker not available: {}", e);
            // This is expected in environments without Docker
        }
    }
}

#[tokio::test]
async fn test_installation_error_handling() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create orchestrator with GitHub strategy for non-existent repo
    let orchestrator = InstallationOrchestrator::new(installation_dir)
        .await
        .unwrap()
        .add_github_strategy("nonexistent-owner".to_string(), "nonexistent-repo".to_string());
    
    // Try to install from non-existent repo
    let result = orchestrator.ensure_binary("nonexistent-binary").await;
    
    // Should fail gracefully
    assert!(result.is_err());
    
    // Error should be informative
    let error = result.unwrap_err();
    assert!(error.to_string().contains("nonexistent-binary"));
}

#[tokio::test]
async fn test_binary_cache_functionality() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    let orchestrator = InstallationOrchestrator::new(installation_dir.clone()).await.unwrap();
    
    // Create a fake binary in the installation directory
    let binary_path = installation_dir.join("test-binary");
    tokio::fs::write(&binary_path, b"fake binary content").await.unwrap();
    
    // Test that the binary is found
    let found_binary = orchestrator.find_binary("test-binary").await;
    assert!(found_binary.is_some());
    assert_eq!(found_binary.unwrap(), binary_path);
    
    // Test that it's found again (should be cached)
    let found_binary_again = orchestrator.find_binary("test-binary").await;
    assert!(found_binary_again.is_some());
}

#[tokio::test]
async fn test_installation_orchestrator_integration() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Test the main entry point function
    let result = crate::installation::create_default_orchestrator(
        installation_dir,
        "test-org".to_string(),
        "test-repo".to_string()
    ).await;
    
    match result {
        Ok(binary_manager) => {
            // Test that we can use it as a BinaryManager
            let install_dir = binary_manager.get_installation_dir();
            assert!(install_dir.exists() || install_dir.parent().map_or(false, |p| p.exists()));
            
            // Test finding a non-existent binary
            let binary_path = binary_manager.find_binary("nonexistent-binary").await;
            assert!(binary_path.is_none());
        }
        Err(e) => {
            println!("Default orchestrator creation failed (likely Docker unavailable): {}", e);
            // This is expected in environments without Docker
        }
    }
}

/// Test that demonstrates the complete workflow
#[tokio::test]
async fn test_complete_installation_workflow() {
    let temp_dir = tempdir().unwrap();
    let installation_dir = temp_dir.path().to_path_buf();
    
    // Create orchestrator with GitHub strategy
    let orchestrator = InstallationOrchestrator::new(installation_dir.clone())
        .await
        .unwrap()
        .add_github_strategy("sharkdp".to_string(), "bat".to_string());
    
    // Test the complete workflow without actually downloading
    // (to avoid network dependencies in tests)
    
    // 1. Check initial state
    let initial_binaries = orchestrator.find_binary("bat").await;
    assert!(initial_binaries.is_none());
    
    // 2. Get installation stats
    let stats = orchestrator.get_installation_stats().await.unwrap();
    assert_eq!(stats.get("available_strategies"), Some(&"1".to_string()));
    assert_eq!(stats.get("strategy_names"), Some(&"github-releases".to_string()));
    
    // 3. Test that installation directory is correct
    assert_eq!(orchestrator.get_installation_dir(), installation_dir);
    
    println!("Complete installation workflow test passed!");
}