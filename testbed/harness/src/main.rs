use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::process::{exit, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

// --- Data Structures for scenario.yaml ---

#[derive(Debug, Deserialize, Default)]
struct Scenario {
    description: String,
    command: Vec<String>,
    assertions: Assertions,
    // If the `long_running` key is present in the YAML, this will be Some.
    // This is a simpler and more direct mapping than a complex enum.
    long_running: Option<LongRunningMode>,
}

#[derive(Debug, Deserialize)]
struct LongRunningMode {
    startup_timeout_ms: u64,
}

#[derive(Debug, Deserialize, Default)]
struct Assertions {
    exit_code: Option<i32>,
    exit_signal: Option<i32>,
    stdout_contains: Option<String>,
    stderr_contains: Option<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Harness Error:\n{:#}", e);
        exit(1);
    }
}

fn run() -> Result<()> {
    let scenario_path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Path to scenario.yaml not provided."))?;

    println!("--- Running Test Scenario: {} ---", scenario_path);

    let scenario_content = fs::read_to_string(&scenario_path)
        .with_context(|| format!("Failed to read scenario file at '{}'", scenario_path))?;
    let scenario: Scenario = serde_yaml::from_str(&scenario_content)
        .with_context(|| "Failed to parse YAML from scenario file")?;

    println!("Description: {}", scenario.description);

    let mut cmd_parts = scenario.command.iter();
    let executable = cmd_parts
        .next()
        .ok_or_else(|| anyhow!("Command in scenario file cannot be empty"))?;

    // --- Execute Command based on RunMode ---
    if let Some(long_running_mode) = scenario.long_running {
        // For long-running processes, we spawn a child process and manage it directly.
        let mut child = Command::new(executable)
            .args(cmd_parts)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn command: {:?}", scenario.command))?;

        println!("Harness: Waiting {}ms for server to start...", long_running_mode.startup_timeout_ms);
        sleep(Duration::from_millis(long_running_mode.startup_timeout_ms));

        // Check if the process has already exited, which would be an error.
        if let Some(status) = child.try_wait()? {
            // If the process exited early, we must capture its output to see why.
            let mut stdout = Vec::new();
            child.stdout.take().unwrap().read_to_end(&mut stdout)?;
            let mut stderr = Vec::new();
            child.stderr.take().unwrap().read_to_end(&mut stderr)?;

            return Err(anyhow!(
                "Process exited prematurely with status: {}\n---\nSTDOUT:\n{}\n---\nSTDERR:\n{}",
                status,
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            ));
        }

        println!("Harness: Terminating process...");
        child.kill().context("Failed to kill child process")?;
        let status = child.wait().context("Failed to wait for child process")?;

        // Read all output from the pipes after the process is terminated.
        let mut stdout = Vec::new();
        child.stdout.take().unwrap().read_to_end(&mut stdout)?;
        let mut stderr = Vec::new();
        child.stderr.take().unwrap().read_to_end(&mut stderr)?;

        verify_assertions(&stdout, &stderr, status, &scenario.assertions)?;
    } else {
        // This is a standard one-shot command.
        let output = Command::new(executable)
            .args(cmd_parts)
            .output()
            .with_context(|| format!("Failed to execute command: {:?}", scenario.command))?;
        verify_assertions(&output.stdout, &output.stderr, output.status, &scenario.assertions)?;
    }

    println!("--- Scenario Passed ---");
    Ok(())
}

/// The assertion logic is now generalized to handle both exit codes and signals.
fn verify_assertions(
    stdout: &[u8],
    stderr: &[u8],
    status: std::process::ExitStatus,
    assertions: &Assertions,
) -> Result<()> {
    // 1. Verify Exit Code (if specified)
    if let Some(expected_code) = assertions.exit_code {
        if status.code() != Some(expected_code) {
            return Err(anyhow!(
                "Assertion failed: Exit code mismatch.\nExpected: {}\nActual: {:?}\n---\nSTDOUT:\n{}\n---\nSTDERR:\n{}",
                expected_code,
                status.code(),
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ));
        }
    }

    // 2. Verify Exit Signal (if specified)
    #[cfg(unix)]
    if let Some(expected_signal) = assertions.exit_signal {
        use std::os::unix::process::ExitStatusExt;
        if status.signal() != Some(expected_signal) {
             return Err(anyhow!(
                "Assertion failed: Exit signal mismatch.\nExpected: {}\nActual: {:?}\n---\nSTDOUT:\n{}\n---\nSTDERR:\n{}",
                expected_signal,
                status.signal(),
                String::from_utf8_lossy(stdout),
                String::from_utf8_lossy(stderr)
            ));
        }
    }

    // 3. Verify STDOUT
    if let Some(expected) = &assertions.stdout_contains {
        let stdout_str = String::from_utf8_lossy(stdout);
        if !stdout_str.contains(expected) {
            return Err(anyhow!(
                "Assertion failed: STDOUT did not contain expected text.\nExpected: '{}'\n---\nActual STDOUT:\n{}",
                expected,
                stdout_str
            ));
        }
    }

    // 4. Verify STDERR
    if let Some(expected) = &assertions.stderr_contains {
        let stderr_str = String::from_utf8_lossy(stderr);
        if !stderr_str.contains(expected) {
            return Err(anyhow!(
                "Assertion failed: STDERR did not contain expected text.\nExpected: '{}'\n---\nActual STDERR:\n{}",
                expected,
                stderr_str
            ));
        }
    }

    Ok(())
}