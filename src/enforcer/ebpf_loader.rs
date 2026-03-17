use anyhow::{anyhow, Result};

#[derive(Debug, Default)]
pub struct EbpfEnforcer;

impl EbpfEnforcer {
    #[cfg(feature = "ebpf")]
    pub fn load() -> Result<Self> {
        use aya::{maps::RingBuf, programs::TracePoint, Ebpf};

        let object_path = std::env::var("VAS_CRUCIBLE_EBPF_OBJECT")
            .unwrap_or_else(|_| "ebpf/target/bpfel-unknown-none/debug/ebpf".to_string());
        let object = std::fs::read(&object_path)
            .map_err(|err| anyhow!("failed to read eBPF object {object_path}: {err}"))?;
        let mut bpf = Ebpf::load(&object)?;
        let program: &mut TracePoint = bpf
            .program_mut("sys_enter")
            .ok_or_else(|| anyhow!("sys_enter tracepoint program missing"))?
            .try_into()?;
        program.load()?;
        program.attach("raw_syscalls", "sys_enter")?;

        let _ring = RingBuf::try_from(
            bpf.take_map("SECURITY_EVENTS")
                .ok_or_else(|| anyhow!("SECURITY_EVENTS ring buffer missing"))?,
        )?;
        Ok(Self)
    }

    #[cfg(not(feature = "ebpf"))]
    pub fn load() -> Result<Self> {
        Err(anyhow!("ebpf feature not enabled"))
    }
}
