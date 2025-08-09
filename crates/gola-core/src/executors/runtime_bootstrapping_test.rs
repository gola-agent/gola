use super::runtime_manager::RuntimeManager;
use crate::config::types::McpExecutionType;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

// Helper to create a fake executable script
fn create_fake_executable(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, "#!/bin/sh").unwrap();
    writeln!(file, "{}", content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

#[tokio::test]
async fn test_local_only_installs_bun_if_missing() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    
    // Use the fake install for testing
    create_fake_executable(&gola_home.join("bin").join("bun"), "echo 'fake bun'");
    env::set_var("GOLA_TEST_FAKE_INSTALL", "true");

    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone());

    let exec_type = McpExecutionType::Runtime {
        runtime: "nodejs".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };

    let result = runtime_manager.resolve_command(&exec_type).await;

    env::remove_var("GOLA_TEST_FAKE_INSTALL");

    assert!(result.is_ok(), "Expected command resolution to succeed, but it failed: {:?}", result.err());
    let command = result.unwrap();
    let expected_path = gola_home.join("bin").join("bun");
    assert_eq!(command.as_std().get_program().to_string_lossy(), expected_path.to_string_lossy());
    let args: Vec<_> = command.as_std().get_args().map(|s| s.to_string_lossy()).collect();
    assert_eq!(args, &["x", "--yes", "some-package"]);
}

#[tokio::test]
#[ignore] // Requires UV to be installed on the system
async fn test_local_only_uses_existing_uv() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let gola_bin_path = gola_home.join("bin");
    create_fake_executable(&gola_bin_path.join("uv"), "echo 'fake uv'");

    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone());
    
    let exec_type = McpExecutionType::Runtime {
        runtime: "python".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };

    let result = runtime_manager.resolve_command(&exec_type).await;
    assert!(result.is_ok(), "UV command resolution failed: {:?}", result.err());
    let command = result.unwrap();
    assert_eq!(command.as_std().get_program().to_string_lossy(), gola_bin_path.join("uv").to_string_lossy());
}

#[tokio::test]
#[ignore]
async fn test_cargo_runtime_installs_and_resolves_command() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let cargo_home = temp_home.path().join("cargo");
    let cargo_bin = cargo_home.join("bin");
    fs::create_dir_all(&cargo_bin).unwrap();

    // Create a fake cargo that creates a fake binary
    let fake_cargo_path = gola_home.join("bin").join("cargo");
    create_fake_executable(&fake_cargo_path, &format!(
        "#!/bin/sh

         echo 'fake cargo install'

         /bin/mkdir -p {}

         echo '#!/bin/sh' > {}/mcp-server-openmeteo

         echo 'echo fake openmeteo server' >> {}/mcp-server-openmeteo

         /bin/chmod +x {}/mcp-server-openmeteo

        ",
        cargo_bin.display(),
        cargo_bin.display(),
        cargo_bin.display(),
        cargo_bin.display()
    ));

    // Temporarily override PATH and CARGO_HOME
    let original_path = env::var("PATH").unwrap_or_default();
    let original_cargo_home = env::var("CARGO_HOME").unwrap_or_default();
    env::set_var("PATH", gola_home.join("bin"));
    env::set_var("CARGO_HOME", &cargo_home);

    let runtime_manager = RuntimeManager::new(false, true) // default mode, non-interactive
        .with_cargo_home(cargo_home.clone());

    let exec_type = McpExecutionType::Runtime {
        runtime: "rust".to_string(),
        entry_point: "https://github.com/gbrigandi/mcp-server-openmeteo.git".to_string(),
        args: vec!["--test-arg".to_string()],
        env: Default::default(),
        working_dir: None,
    };

    let result = runtime_manager.resolve_command(&exec_type).await;

    // Restore environment
    env::set_var("PATH", original_path);
    env::set_var("CARGO_HOME", original_cargo_home);

    assert!(result.is_ok(), "Failed to resolve cargo command: {:?}", result.err());
    let command = result.unwrap();
    let expected_bin_path = cargo_bin.join("mcp-server-openmeteo");
    assert_eq!(command.as_std().get_program().to_string_lossy(), expected_bin_path.to_string_lossy());
    let args: Vec<_> = command.as_std().get_args().map(|s| s.to_string_lossy()).collect();
    assert_eq!(args, &["--test-arg"]);
}

#[tokio::test]
async fn test_install_bun_real() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone());

    // Test bun installation
    let exec_type_bun = McpExecutionType::Runtime {
        runtime: "nodejs".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    
    let result_bun = runtime_manager.resolve_command(&exec_type_bun).await;
    assert!(result_bun.is_ok(), "bun installation failed: {:?}", result_bun.err());
    assert!(gola_home.join("bin").join("bun").exists(), "bun binary not found after installation");
}

#[tokio::test]
async fn test_install_uv_real() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone());

    // Test uv installation
    let exec_type_uv = McpExecutionType::Runtime {
        runtime: "python".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    
    let result_uv = runtime_manager.resolve_command(&exec_type_uv).await;
    assert!(result_uv.is_ok(), "uv installation failed: {:?}", result_uv.err());
    assert!(gola_home.join("bin").join("uv").exists(), "uv binary not found after installation");
}

#[tokio::test]
async fn test_install_rustup_real() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let bin_dir = gola_home.join("bin");
    let cargo_bin = gola_home.join("cargo").join("bin");
    fs::create_dir_all(&cargo_bin).unwrap();
    create_fake_executable(&bin_dir.join("cargo"), &format!(
        "#!/bin/sh
         echo 'fake cargo install'
         /bin/mkdir -p {}
         echo '#!/bin/sh' > {}/mcp-server-openmeteo
         echo 'echo fake openmeteo server' >> {}/mcp-server-openmeteo
         /bin/chmod +x {}/mcp-server-openmeteo
        ",
        cargo_bin.display(),
        cargo_bin.display(),
        cargo_bin.display(),
        cargo_bin.display()
    ));

    let original_path = env::var("PATH").unwrap_or_default();
    env::set_var("PATH", &bin_dir);
    env::set_var("GOLA_TEST_FAKE_INSTALL", "true");
    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone())
        .with_cargo_home(gola_home.join("cargo"));

    // Test rustup installation
    let exec_type_rust = McpExecutionType::Runtime {
        runtime: "rust".to_string(),
        entry_point: "https://github.com/gbrigandi/mcp-server-openmeteo.git".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    let result_rust = runtime_manager.resolve_command(&exec_type_rust).await;
    env::remove_var("GOLA_TEST_FAKE_INSTALL");
    env::set_var("PATH", original_path);
    assert!(result_rust.is_ok(), "rustup installation failed: {:?}", result_rust.err());
    assert!(gola_home.join("bin").join("cargo").exists(), "cargo binary not found after installation");
}

#[tokio::test]
#[ignore]
async fn test_install_cargo_real() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let cargo_home = gola_home.join("cargo");
    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone())
        .with_cargo_home(cargo_home.clone());

    // Test cargo/rustup installation
    let exec_type_rust = McpExecutionType::Runtime {
        runtime: "rust".to_string(),
        entry_point: "https://github.com/gbrigandi/mcp-server-openmeteo.git".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    
    let result_rust = runtime_manager.resolve_command(&exec_type_rust).await;
    assert!(result_rust.is_ok(), "cargo installation failed: {:?}", result_rust.err());
    assert!(cargo_home.join("bin").join("cargo").exists(), "cargo binary not found after installation");
}

#[tokio::test]
#[ignore] // Requires UV to be installed on the system
async fn test_no_install_if_exists() {
    let temp_home = tempdir().unwrap();
    let gola_home = temp_home.path().to_path_buf();
    let bin_dir = gola_home.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    create_fake_executable(&bin_dir.join("bun"), "echo 'fake bun'");
    create_fake_executable(&bin_dir.join("uv"), "echo 'fake uv'");

    let runtime_manager = RuntimeManager::new(true, true) // local-only, non-interactive
        .with_gola_home(gola_home.clone());

    // Test bun
    let exec_type_bun = McpExecutionType::Runtime {
        runtime: "nodejs".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    let result_bun = runtime_manager.resolve_command(&exec_type_bun).await;
    assert!(result_bun.is_ok());

    // Test uv
    let exec_type_uv = McpExecutionType::Runtime {
        runtime: "python".to_string(),
        entry_point: "some-package".to_string(),
        args: vec![],
        env: Default::default(),
        working_dir: None,
    };
    let result_uv = runtime_manager.resolve_command(&exec_type_uv).await;
    assert!(result_uv.is_ok(), "UV command resolution failed: {:?}", result_uv.err());
}