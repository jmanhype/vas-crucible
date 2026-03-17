#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum InterceptedSyscall {
    Execve = 59,
    Open = 2,
    Connect = 42,
    Socket = 41,
}

#[derive(Debug, Clone)]
pub struct SecurityEvent {
    pub pid: u32,
    pub syscall: InterceptedSyscall,
    pub allowed: bool,
    pub intent_hash: [u8; 32],
}

