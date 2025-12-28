use anyhow::Result;
use std::process::{Command, Stdio};

use super::CommandResult;

pub struct LocalConnection {
    host: String,
}

impl LocalConnection {
    pub fn new() -> Self {
        Self {
            host: "localhost".to_string(),
        }
    }

    pub fn exec(&self, command: &str) -> Result<CommandResult> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        Ok(CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    pub fn write_file(&self, path: &str, content: &[u8], mode: i32) -> Result<()> {
        std::fs::write(path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(mode as u32);
            std::fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(path)?)
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

impl Default for LocalConnection {
    fn default() -> Self {
        Self::new()
    }
}
