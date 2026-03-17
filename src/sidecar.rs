use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Result;
use tonic::transport::Server;

use crate::{
    grpc::{
        generated::sandbox_control_server::SandboxControlServer,
        server::SandboxControlService,
    },
    jwt::verifier::{JwtKeySource, JwtVerifier},
    sandbox::{DockerSandboxBackend, SandboxBackend, SandboxManager},
};

#[derive(Clone, Debug)]
pub struct SidecarConfig {
    pub listen_addr: SocketAddr,
    pub jwt_key: JwtKeySource,
    pub workspace_dir: PathBuf,
    pub pty_dir: PathBuf,
    pub docker_image: String,
}

impl SidecarConfig {
    pub fn new_hs256(listen_addr: SocketAddr, secret: impl Into<String>) -> Self {
        Self {
            listen_addr,
            jwt_key: JwtKeySource::Hs256 {
                secret: secret.into().into_bytes().into(),
            },
            workspace_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            pty_dir: std::env::temp_dir().join("vas-crucible-pty"),
            docker_image: "alpine:3.20".to_string(),
        }
    }
}

pub struct SidecarRuntime {
    listen_addr: SocketAddr,
    service: SandboxControlService,
}

impl SidecarRuntime {
    pub fn new(config: SidecarConfig) -> Result<Self> {
        let backend = Arc::new(DockerSandboxBackend::connect_local(
            config.docker_image.clone(),
            config.pty_dir.clone(),
        )?) as Arc<dyn SandboxBackend>;
        Self::new_with_backend(config, backend)
    }

    pub fn new_with_backend(config: SidecarConfig, backend: Arc<dyn SandboxBackend>) -> Result<Self> {
        let verifier = Arc::new(JwtVerifier::new(config.jwt_key.clone()));
        let manager = SandboxManager::new(backend);
        let service = SandboxControlService::new(verifier, manager, config.workspace_dir);
        Ok(Self {
            listen_addr: config.listen_addr,
            service,
        })
    }

    pub async fn serve(self) -> Result<()> {
        Server::builder()
            .add_service(SandboxControlServer::new(self.service))
            .serve(self.listen_addr)
            .await?;
        Ok(())
    }
}
