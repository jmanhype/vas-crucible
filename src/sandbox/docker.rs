use std::{
    collections::HashMap,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bollard::{
    container::{
        Config, CreateContainerOptions, InspectContainerOptions, LogOutput, RemoveContainerOptions,
        StartContainerOptions,
    },
    exec::{CreateExecOptions, StartExecResults},
    image::CreateImageOptions,
    models::HealthConfig,
    Docker,
};
use futures::StreamExt;
use tokio::{process::Command, sync::RwLock};
use uuid::Uuid;

use crate::sandbox::{
    limits::ResourceLimitConfig, pty::PtySession, unix_now, ExecutionResult, SandboxBackend,
    SandboxConfig, SandboxInfo, SandboxStatus,
};

#[derive(Debug, Clone)]
struct DockerSandboxEntry {
    container_id: String,
    pty: PtySession,
}

#[derive(Clone)]
pub struct DockerSandboxBackend {
    docker: Docker,
    image: String,
    pty_dir: PathBuf,
    entries: Arc<RwLock<HashMap<String, DockerSandboxEntry>>>,
}

impl DockerSandboxBackend {
    pub fn connect_local(image: impl Into<String>, pty_dir: PathBuf) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            image: image.into(),
            pty_dir,
            entries: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn ensure_image(&self) -> Result<()> {
        let options = Some(CreateImageOptions {
            from_image: self.image.clone(),
            ..Default::default()
        });

        let mut stream = self.docker.create_image(options, None, None);
        while let Some(progress) = stream.next().await {
            progress?;
        }
        Ok(())
    }

    fn command_for(language: &str, code: &str) -> Result<Vec<String>> {
        match language {
            "sh" | "bash" => Ok(vec!["/bin/sh".to_string(), "-lc".to_string(), code.to_string()]),
            "python" => Ok(vec![
                "/bin/sh".to_string(),
                "-lc".to_string(),
                format!("python3 - <<'PY'\n{code}\nPY"),
            ]),
            _ => Err(anyhow!("unsupported language: {language}")),
        }
    }
}

#[async_trait]
impl SandboxBackend for DockerSandboxBackend {
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInfo> {
        config.limits.validate()?;
        self.ensure_image().await?;

        let sandbox_id = format!("sandbox-{}", Uuid::new_v4());
        let pty = PtySession::allocate(&self.pty_dir)?;
        let pty_path = pty.path().display().to_string();
        let bind = format!("{}:/workspace:rw", config.workspace_dir.display());

        let create_options = Some(CreateContainerOptions {
            name: sandbox_id.clone(),
            platform: None,
        });
        let container = Config {
            image: Some(self.image.clone()),
            cmd: Some(vec!["/bin/sh".to_string(), "-lc".to_string(), "sleep infinity".to_string()]),
            working_dir: Some("/workspace".to_string()),
            tty: Some(true),
            open_stdin: Some(true),
            healthcheck: Some(HealthConfig {
                test: Some(vec!["CMD-SHELL".to_string(), "test -d /workspace".to_string()]),
                interval: Some(5_000_000_000),
                timeout: Some(2_000_000_000),
                retries: Some(3),
                start_period: Some(1_000_000_000),
                ..Default::default()
            }),
            host_config: Some(config.limits.to_host_config(vec![bind])),
            ..Default::default()
        };

        let created = self
            .docker
            .create_container(create_options, container)
            .await
            .context("failed to create docker container")?;

        self.docker
            .start_container(&created.id, None::<StartContainerOptions<String>>)
            .await
            .context("failed to start docker container")?;

        self.entries.write().await.insert(
            sandbox_id.clone(),
            DockerSandboxEntry {
                container_id: created.id,
                pty,
            },
        );

        Ok(SandboxInfo {
            sandbox_id,
            pty_path,
            created_at: unix_now(),
        })
    }

    async fn execute(&self, sandbox_id: &str, code: &str, language: &str) -> Result<ExecutionResult> {
        let entry = self
            .entries
            .read()
            .await
            .get(sandbox_id)
            .cloned()
            .ok_or_else(|| anyhow!("sandbox not found: {sandbox_id}"))?;
        let command = Self::command_for(language, code)?;
        let started = Instant::now();

        let exec = self
            .docker
            .create_exec(
                &entry.container_id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
                    ..Default::default()
                },
            )
            .await
            .context("failed to create exec in container")?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .context("failed to start exec in container")?
        {
            while let Some(item) = output.next().await {
                match item? {
                    LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message))
                    }
                    LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message))
                    }
                    _ => {}
                }
            }
        }

        let inspected = self.docker.inspect_exec(&exec.id).await?;
        let exit_code = inspected.exit_code.unwrap_or_default() as i32;
        entry.pty.record_output(&stdout, &stderr)?;

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis() as i64,
        })
    }

    async fn terminate(&self, sandbox_id: &str) -> Result<()> {
        let entry = self.entries.write().await.remove(sandbox_id);
        let Some(entry) = entry else {
            return Err(anyhow!("sandbox not found: {sandbox_id}"));
        };

        self.docker
            .remove_container(
                &entry.container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .context("failed to remove container")?;
        entry.pty.cleanup()?;
        Ok(())
    }

    async fn heartbeat(&self, sandbox_id: &str) -> Result<SandboxStatus> {
        let entry = self
            .entries
            .read()
            .await
            .get(sandbox_id)
            .cloned()
            .ok_or_else(|| anyhow!("sandbox not found: {sandbox_id}"))?;
        let details = self
            .docker
            .inspect_container(&entry.container_id, None::<InspectContainerOptions>)
            .await?;
        Ok(SandboxStatus {
            alive: details.state.and_then(|state| state.running).unwrap_or(false),
            last_seen: unix_now(),
        })
    }
}

#[derive(Debug, Clone)]
struct LocalEntry {
    pty: PtySession,
    created_at: i64,
    limits: ResourceLimitConfig,
}

#[derive(Clone)]
pub struct LocalSandboxBackend {
    root: PathBuf,
    entries: Arc<RwLock<HashMap<String, LocalEntry>>>,
}

impl LocalSandboxBackend {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn shell_command(language: &str, code: &str) -> Result<Command> {
        let mut command = if cfg!(windows) {
            let mut cmd = Command::new("powershell");
            cmd.arg("-NoProfile").arg("-Command");
            cmd
        } else {
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-lc");
            cmd
        };

        let script = match language {
            "sh" | "bash" => code.to_string(),
            "python" => {
                if cfg!(windows) {
                    format!("@'\n{code}\n'@ | python -")
                } else {
                    format!("python3 - <<'PY'\n{code}\nPY")
                }
            }
            _ => return Err(anyhow!("unsupported language: {language}")),
        };

        command.arg(script);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        Ok(command)
    }
}

#[async_trait]
impl SandboxBackend for LocalSandboxBackend {
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInfo> {
        config.limits.validate()?;
        std::fs::create_dir_all(&self.root)?;
        let sandbox_id = format!("local-{}", Uuid::new_v4());
        let pty = PtySession::allocate(&self.root)?;
        let created_at = unix_now();

        self.entries.write().await.insert(
            sandbox_id.clone(),
            LocalEntry {
                pty: pty.clone(),
                created_at,
                limits: config.limits,
            },
        );

        Ok(SandboxInfo {
            sandbox_id,
            pty_path: pty.path().display().to_string(),
            created_at,
        })
    }

    async fn execute(&self, sandbox_id: &str, code: &str, language: &str) -> Result<ExecutionResult> {
        let entry = self
            .entries
            .read()
            .await
            .get(sandbox_id)
            .cloned()
            .ok_or_else(|| anyhow!("sandbox not found: {sandbox_id}"))?;

        if entry.limits.memory_mb > 512 || entry.limits.cpu_cores > 1 {
            return Err(anyhow!("resource limits exceed local backend bounds"));
        }

        let started = Instant::now();
        let output = Self::shell_command(language, code)?
            .output()
            .await
            .context("failed to execute local sandbox command")?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        entry.pty.record_output(&stdout, &stderr)?;

        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis() as i64,
        })
    }

    async fn terminate(&self, sandbox_id: &str) -> Result<()> {
        let entry = self.entries.write().await.remove(sandbox_id);
        let Some(entry) = entry else {
            return Err(anyhow!("sandbox not found: {sandbox_id}"));
        };
        entry.pty.cleanup()?;
        Ok(())
    }

    async fn heartbeat(&self, sandbox_id: &str) -> Result<SandboxStatus> {
        let entry = self
            .entries
            .read()
            .await
            .get(sandbox_id)
            .cloned()
            .ok_or_else(|| anyhow!("sandbox not found: {sandbox_id}"))?;
        Ok(SandboxStatus {
            alive: true,
            last_seen: entry.created_at.max(unix_now()),
        })
    }
}
