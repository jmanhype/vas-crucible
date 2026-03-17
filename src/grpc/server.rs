use std::{path::PathBuf, sync::Arc};

use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use tracing::{info, warn};

use crate::{
    grpc::generated::{
        sandbox_control_server::SandboxControl, CreateSandboxRequest, CreateSandboxResponse,
        ExecuteRequest, ExecuteResponse, HeartbeatRequest, HeartbeatResponse, ResourceLimits,
        TerminateRequest,
    },
    jwt::verifier::JwtVerifier,
    sandbox::{ResourceLimitConfig, SandboxConfig, SandboxManager},
};

#[derive(Clone)]
pub struct SandboxControlService {
    verifier: Arc<JwtVerifier>,
    manager: SandboxManager,
    workspace_dir: PathBuf,
}

impl SandboxControlService {
    pub fn new(verifier: Arc<JwtVerifier>, manager: SandboxManager, workspace_dir: PathBuf) -> Self {
        Self {
            verifier,
            manager,
            workspace_dir,
        }
    }

    pub fn create_intent_hash(agent_id: &str, limits: &ResourceLimitConfig) -> String {
        hash_payload(format!(
            "create:{agent_id}:{}:{}:{}",
            limits.cpu_cores, limits.memory_mb, limits.network_enabled
        ))
    }

    pub fn execute_intent_hash(sandbox_id: &str, language: &str, code: &str) -> String {
        hash_payload(format!("execute:{sandbox_id}:{language}:{code}"))
    }

    pub fn terminate_intent_hash(sandbox_id: &str) -> String {
        hash_payload(format!("terminate:{sandbox_id}"))
    }

    pub fn heartbeat_intent_hash(sandbox_id: &str) -> String {
        hash_payload(format!("heartbeat:{sandbox_id}"))
    }
}

fn hash_payload(payload: String) -> String {
    let digest = Sha256::digest(payload.as_bytes());
    format!("{digest:x}")
}

fn map_limits(
    limits: Option<ResourceLimits>,
) -> std::result::Result<ResourceLimitConfig, Status> {
    let mapped = limits
        .map(|item| ResourceLimitConfig {
            cpu_cores: item.cpu_cores,
            memory_mb: item.memory_mb,
            network_enabled: item.network_enabled,
        })
        .unwrap_or_default();
    mapped.validate().map_err(internal_status)?;
    Ok(mapped)
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

#[tonic::async_trait]
impl SandboxControl for SandboxControlService {
    async fn create_sandbox(
        &self,
        request: Request<CreateSandboxRequest>,
    ) -> std::result::Result<Response<CreateSandboxResponse>, Status> {
        let request = request.into_inner();
        let limits = map_limits(request.limits)?;
        let expected_intent_hash = Self::create_intent_hash(&request.agent_id, &limits);
        if request.intent_hash != expected_intent_hash {
            warn!("create_sandbox intent mismatch for agent {}", request.agent_id);
            return Err(Status::permission_denied("request intent hash mismatch"));
        }
        self.verifier
            .verify(&request.jwt, &expected_intent_hash)
            .map_err(|err| Status::permission_denied(err.to_string()))?;

        let info = self
            .manager
            .create_sandbox(SandboxConfig {
                agent_id: request.agent_id,
                limits,
                workspace_dir: self.workspace_dir.clone(),
            })
            .await
            .map_err(internal_status)?;

        info!("created sandbox {}", info.sandbox_id);
        Ok(Response::new(CreateSandboxResponse {
            sandbox_id: info.sandbox_id,
            pty_path: info.pty_path,
            created_at: info.created_at,
        }))
    }

    async fn execute_code(
        &self,
        request: Request<ExecuteRequest>,
    ) -> std::result::Result<Response<ExecuteResponse>, Status> {
        let request = request.into_inner();
        let intent_hash =
            Self::execute_intent_hash(&request.sandbox_id, &request.language, &request.code);
        self.verifier
            .verify(&request.jwt, &intent_hash)
            .map_err(|err| Status::permission_denied(err.to_string()))?;

        let result = self
            .manager
            .execute_code(&request.sandbox_id, &request.code, &request.language)
            .await
            .map_err(internal_status)?;

        Ok(Response::new(ExecuteResponse {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms: result.duration_ms,
        }))
    }

    async fn terminate_sandbox(
        &self,
        request: Request<TerminateRequest>,
    ) -> std::result::Result<Response<prost_types::Empty>, Status> {
        let request = request.into_inner();
        let intent_hash = Self::terminate_intent_hash(&request.sandbox_id);
        self.verifier
            .verify(&request.jwt, &intent_hash)
            .map_err(|err| Status::permission_denied(err.to_string()))?;

        self.manager
            .terminate_sandbox(&request.sandbox_id)
            .await
            .map_err(internal_status)?;

        Ok(Response::new(prost_types::Empty {}))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> std::result::Result<Response<HeartbeatResponse>, Status> {
        let request = request.into_inner();
        let intent_hash = Self::heartbeat_intent_hash(&request.sandbox_id);
        self.verifier
            .verify(&request.jwt, &intent_hash)
            .map_err(|err| Status::permission_denied(err.to_string()))?;

        let status = self
            .manager
            .heartbeat(&request.sandbox_id)
            .await
            .map_err(internal_status)?;

        Ok(Response::new(HeartbeatResponse {
            alive: status.alive,
            last_seen: status.last_seen,
        }))
    }
}
