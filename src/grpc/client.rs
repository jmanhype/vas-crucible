use tonic::transport::Channel;

use crate::grpc::generated::sandbox_control_client::SandboxControlClient;

pub async fn connect(dst: String) -> Result<SandboxControlClient<Channel>, tonic::transport::Error> {
    SandboxControlClient::connect(dst).await
}

