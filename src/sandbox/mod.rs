pub mod docker;
pub mod limits;
pub mod pty;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

pub use docker::{DockerSandboxBackend, LocalSandboxBackend};
pub use limits::ResourceLimitConfig;

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub agent_id: String,
    pub limits: ResourceLimitConfig,
    pub workspace_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SandboxInfo {
    pub sandbox_id: String,
    pub pty_path: String,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone)]
pub struct SandboxStatus {
    pub alive: bool,
    pub last_seen: i64,
}

#[derive(Debug, Clone)]
pub struct SandboxRecord {
    pub info: SandboxInfo,
    pub agent_id: String,
    pub last_seen: i64,
}

#[async_trait]
pub trait SandboxBackend: Send + Sync {
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInfo>;
    async fn execute(&self, sandbox_id: &str, code: &str, language: &str) -> Result<ExecutionResult>;
    async fn terminate(&self, sandbox_id: &str) -> Result<()>;
    async fn heartbeat(&self, sandbox_id: &str) -> Result<SandboxStatus>;
}

#[derive(Clone)]
pub struct SandboxManager {
    backend: Arc<dyn SandboxBackend>,
    registry: Arc<RwLock<HashMap<String, SandboxRecord>>>,
}

impl SandboxManager {
    pub fn new(backend: Arc<dyn SandboxBackend>) -> Self {
        Self {
            backend,
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_sandbox(&self, config: SandboxConfig) -> Result<SandboxInfo> {
        let info = self.backend.create(config.clone()).await?;
        self.registry.write().await.insert(
            info.sandbox_id.clone(),
            SandboxRecord {
                info: info.clone(),
                agent_id: config.agent_id,
                last_seen: unix_now(),
            },
        );
        Ok(info)
    }

    pub async fn execute_code(
        &self,
        sandbox_id: &str,
        code: &str,
        language: &str,
    ) -> Result<ExecutionResult> {
        let result = self.backend.execute(sandbox_id, code, language).await?;
        if let Some(record) = self.registry.write().await.get_mut(sandbox_id) {
            record.last_seen = unix_now();
        }
        Ok(result)
    }

    pub async fn terminate_sandbox(&self, sandbox_id: &str) -> Result<()> {
        self.backend.terminate(sandbox_id).await?;
        self.registry.write().await.remove(sandbox_id);
        Ok(())
    }

    pub async fn heartbeat(&self, sandbox_id: &str) -> Result<SandboxStatus> {
        let status = self.backend.heartbeat(sandbox_id).await?;
        if status.alive {
            if let Some(record) = self.registry.write().await.get_mut(sandbox_id) {
                record.last_seen = status.last_seen;
            }
        }
        Ok(status)
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs() as i64
}

