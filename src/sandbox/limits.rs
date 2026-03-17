use anyhow::{anyhow, Result};
use bollard::models::HostConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimitConfig {
    pub cpu_cores: i32,
    pub memory_mb: i64,
    pub network_enabled: bool,
}

impl Default for ResourceLimitConfig {
    fn default() -> Self {
        Self {
            cpu_cores: 1,
            memory_mb: 512,
            network_enabled: false,
        }
    }
}

impl ResourceLimitConfig {
    pub fn validate(&self) -> Result<()> {
        if self.cpu_cores <= 0 || self.cpu_cores > 1 {
            return Err(anyhow!("cpu_cores must be between 1 and 1"));
        }
        if self.memory_mb <= 0 || self.memory_mb > 512 {
            return Err(anyhow!("memory_mb must be between 1 and 512"));
        }
        Ok(())
    }

    pub fn to_host_config(&self, binds: Vec<String>) -> HostConfig {
        HostConfig {
            binds: Some(binds),
            memory: Some(self.memory_mb * 1024 * 1024),
            nano_cpus: Some(self.cpu_cores as i64 * 1_000_000_000),
            network_mode: Some(if self.network_enabled {
                "bridge".to_string()
            } else {
                "none".to_string()
            }),
            auto_remove: Some(true),
            ..Default::default()
        }
    }
}
