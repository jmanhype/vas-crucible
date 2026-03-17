use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PtySession {
    path: PathBuf,
}

impl PtySession {
    pub fn allocate(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;
        let path = base_dir.join(format!("{}.pty", Uuid::new_v4()));
        let mut file = File::create(&path)?;
        file.write_all(b"vas-crucible pty session\n")?;

        #[cfg(unix)]
        {
            let _ = nix::pty::openpty(None, None)?;
        }

        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_output(&self, stdout: &str, stderr: &str) -> Result<()> {
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        if !stdout.is_empty() {
            writeln!(file, "stdout:\n{stdout}")?;
        }
        if !stderr.is_empty() {
            writeln!(file, "stderr:\n{stderr}")?;
        }
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
