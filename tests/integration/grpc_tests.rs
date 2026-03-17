use std::{net::SocketAddr, sync::Arc, time::Duration};

use tempfile::TempDir;
use tokio::time::sleep;
use tonic::Request;
use vas_crucible::{
    grpc::{
        client::connect,
        generated::{
            CreateSandboxRequest, ExecuteRequest, HeartbeatRequest, ResourceLimits,
            TerminateRequest,
        },
        server::SandboxControlService,
    },
    sandbox::{LocalSandboxBackend, SandboxBackend},
    sidecar::{SidecarConfig, SidecarRuntime},
};

use crate::common::{make_claims, sign_hs256};

fn local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    addr
}

fn current_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

fn test_command() -> &'static str {
    if cfg!(windows) {
        "Write-Output 'grpc'"
    } else {
        "printf 'grpc\n'"
    }
}

#[tokio::test]
async fn grpc_create_execute_terminate_and_heartbeat() {
    let root = TempDir::new().expect("temp dir");
    let addr = local_addr();
    let secret = "grpc-secret";

    let mut config = SidecarConfig::new_hs256(addr, secret);
    config.workspace_dir = root.path().to_path_buf();
    config.pty_dir = root.path().join("pty");

    let backend = Arc::new(LocalSandboxBackend::new(root.path().join("sandboxes"))) as Arc<dyn SandboxBackend>;
    let runtime = SidecarRuntime::new_with_backend(config.clone(), backend).expect("runtime");

    tokio::spawn(async move {
        runtime.serve().await.expect("server");
    });
    sleep(Duration::from_millis(150)).await;

    let mut client = connect(format!("http://{}", addr)).await.expect("connect");

    let limits = ResourceLimits {
        cpu_cores: 1,
        memory_mb: 512,
        network_enabled: false,
    };
    let now = current_time();
    let create_intent = SandboxControlService::create_intent_hash(
        "agent-1",
        &vas_crucible::sandbox::ResourceLimitConfig::default(),
    );
    let create_jwt = sign_hs256(secret, &make_claims(create_intent.clone(), now));

    let create = client
        .create_sandbox(Request::new(CreateSandboxRequest {
            agent_id: "agent-1".to_string(),
            jwt: create_jwt,
            intent_hash: create_intent,
            limits: Some(limits),
        }))
        .await
        .expect("create sandbox")
        .into_inner();

    let execute_intent = SandboxControlService::execute_intent_hash(
        &create.sandbox_id,
        "bash",
        test_command(),
    );
    let execute_jwt = sign_hs256(secret, &make_claims(execute_intent.clone(), now));

    let executed = client
        .execute_code(Request::new(ExecuteRequest {
            sandbox_id: create.sandbox_id.clone(),
            jwt: execute_jwt,
            code: test_command().to_string(),
            language: "bash".to_string(),
        }))
        .await
        .expect("execute")
        .into_inner();

    assert_eq!(executed.exit_code, 0);
    assert!(executed.stdout.to_lowercase().contains("grpc"));

    let heartbeat_intent = SandboxControlService::heartbeat_intent_hash(&create.sandbox_id);
    let heartbeat_jwt = sign_hs256(secret, &make_claims(heartbeat_intent.clone(), now));

    let heartbeat = client
        .heartbeat(Request::new(HeartbeatRequest {
            sandbox_id: create.sandbox_id.clone(),
            jwt: heartbeat_jwt,
        }))
        .await
        .expect("heartbeat")
        .into_inner();

    assert!(heartbeat.alive);

    let terminate_intent = SandboxControlService::terminate_intent_hash(&create.sandbox_id);
    let terminate_jwt = sign_hs256(secret, &make_claims(terminate_intent.clone(), now));

    client
        .terminate_sandbox(Request::new(TerminateRequest {
            sandbox_id: create.sandbox_id.clone(),
            jwt: terminate_jwt,
        }))
        .await
        .expect("terminate");
}
