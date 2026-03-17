use std::sync::Arc;

use tempfile::TempDir;
use vas_crucible::sandbox::{
    LocalSandboxBackend, ResourceLimitConfig, SandboxConfig, SandboxManager,
};

fn test_command() -> &'static str {
    if cfg!(windows) {
        "Write-Output 'hello'"
    } else {
        "printf 'hello\n'"
    }
}

#[tokio::test]
async fn create_sandbox_and_execute_code() {
    let root = TempDir::new().expect("temp dir");
    let backend = Arc::new(LocalSandboxBackend::new(root.path().to_path_buf()));
    let manager = SandboxManager::new(backend);

    let sandbox = manager
        .create_sandbox(SandboxConfig {
            agent_id: "agent-1".to_string(),
            limits: ResourceLimitConfig::default(),
            workspace_dir: root.path().to_path_buf(),
        })
        .await
        .expect("sandbox created");

    let result = manager
        .execute_code(&sandbox.sandbox_id, test_command(), "bash")
        .await
        .expect("command executed");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.to_lowercase().contains("hello"));
}

#[tokio::test]
async fn enforces_resource_limit_validation() {
    let root = TempDir::new().expect("temp dir");
    let backend = Arc::new(LocalSandboxBackend::new(root.path().to_path_buf()));
    let manager = SandboxManager::new(backend);

    let error = manager
        .create_sandbox(SandboxConfig {
            agent_id: "agent-1".to_string(),
            limits: ResourceLimitConfig {
                cpu_cores: 2,
                memory_mb: 512,
                network_enabled: false,
            },
            workspace_dir: root.path().to_path_buf(),
        })
        .await
        .expect_err("limits should fail");

    assert!(error.to_string().contains("cpu_cores"));
}

#[tokio::test]
async fn terminates_sandbox() {
    let root = TempDir::new().expect("temp dir");
    let backend = Arc::new(LocalSandboxBackend::new(root.path().to_path_buf()));
    let manager = SandboxManager::new(backend);

    let sandbox = manager
        .create_sandbox(SandboxConfig {
            agent_id: "agent-1".to_string(),
            limits: ResourceLimitConfig::default(),
            workspace_dir: root.path().to_path_buf(),
        })
        .await
        .expect("sandbox created");

    manager
        .terminate_sandbox(&sandbox.sandbox_id)
        .await
        .expect("sandbox terminated");

    let error = manager
        .heartbeat(&sandbox.sandbox_id)
        .await
        .expect_err("sandbox should be gone");
    assert!(error.to_string().contains("sandbox not found"));
}
