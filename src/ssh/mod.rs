pub mod local;

use anyhow::{anyhow, Result};
use ssh2::Session;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

pub use local::LocalConnection;

pub struct SshConnection {
    session: Session,
    host: String,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl SshConnection {
    pub fn connect(host: &str, port: u16, user: &str, auth: Auth) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        let tcp = TcpStream::connect(&addr)?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        match auth {
            Auth::Key { private_key, passphrase } => {
                let passphrase = passphrase.as_deref();
                session.userauth_pubkey_file(user, None, Path::new(&private_key), passphrase)?;
            }
            Auth::Password(password) => {
                session.userauth_password(user, &password)?;
            }
            Auth::Agent => {
                let mut agent = session.agent()?;
                agent.connect()?;
                agent.list_identities()?;

                let identities = agent.identities()?;
                let mut authenticated = false;

                for identity in identities {
                    if agent.userauth(user, &identity).is_ok() {
                        authenticated = true;
                        break;
                    }
                }

                if !authenticated {
                    return Err(anyhow!("SSH agent authentication failed"));
                }
            }
        }

        if !session.authenticated() {
            return Err(anyhow!("SSH authentication failed"));
        }

        Ok(Self {
            session,
            host: host.to_string(),
        })
    }

    pub fn exec(&self, command: &str) -> Result<CommandResult> {
        let mut channel = self.session.channel_session()?;
        channel.exec(command)?;

        let mut stdout = String::new();
        channel.read_to_string(&mut stdout)?;

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr)?;

        channel.wait_close()?;
        let exit_code = channel.exit_status()?;

        Ok(CommandResult {
            stdout,
            stderr,
            exit_code,
        })
    }

    pub fn exec_sudo(&self, command: &str, password: &str) -> Result<CommandResult> {
        let sudo_cmd = format!("echo '{}' | sudo -S {}", password, command);
        self.exec(&sudo_cmd)
    }

    pub fn upload(&self, local_path: &Path, remote_path: &str, mode: i32) -> Result<()> {
        let content = std::fs::read(local_path)?;
        let mut remote_file = self.session.scp_send(
            Path::new(remote_path),
            mode,
            content.len() as u64,
            None,
        )?;

        remote_file.write_all(&content)?;
        remote_file.send_eof()?;
        remote_file.wait_eof()?;
        remote_file.close()?;
        remote_file.wait_close()?;

        Ok(())
    }

    pub fn download(&self, remote_path: &str, local_path: &Path) -> Result<()> {
        let (mut remote_file, _stat) = self.session.scp_recv(Path::new(remote_path))?;

        let mut content = Vec::new();
        remote_file.read_to_end(&mut content)?;

        std::fs::write(local_path, content)?;

        Ok(())
    }

    pub fn write_file(&self, remote_path: &str, content: &[u8], mode: i32) -> Result<()> {
        let mut remote_file = self.session.scp_send(
            Path::new(remote_path),
            mode,
            content.len() as u64,
            None,
        )?;

        remote_file.write_all(content)?;
        remote_file.send_eof()?;
        remote_file.wait_eof()?;
        remote_file.close()?;
        remote_file.wait_close()?;

        Ok(())
    }

    pub fn read_file(&self, remote_path: &str) -> Result<Vec<u8>> {
        let (mut remote_file, _stat) = self.session.scp_recv(Path::new(remote_path))?;

        let mut content = Vec::new();
        remote_file.read_to_end(&mut content)?;

        Ok(content)
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Clone)]
pub enum Auth {
    Key {
        private_key: String,
        passphrase: Option<String>,
    },
    Password(String),
    Agent,
}

impl Auth {
    pub fn key(path: &str) -> Self {
        Auth::Key {
            private_key: path.to_string(),
            passphrase: None,
        }
    }

    pub fn key_with_passphrase(path: &str, passphrase: &str) -> Self {
        Auth::Key {
            private_key: path.to_string(),
            passphrase: Some(passphrase.to_string()),
        }
    }

    pub fn password(password: &str) -> Self {
        Auth::Password(password.to_string())
    }

    pub fn agent() -> Self {
        Auth::Agent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_key_creation() {
        let auth = Auth::key("/home/user/.ssh/id_rsa");
        match auth {
            Auth::Key { private_key, passphrase } => {
                assert_eq!(private_key, "/home/user/.ssh/id_rsa");
                assert!(passphrase.is_none());
            }
            _ => panic!("Expected Key auth"),
        }
    }

    #[test]
    fn auth_password_creation() {
        let auth = Auth::password("secret");
        match auth {
            Auth::Password(p) => assert_eq!(p, "secret"),
            _ => panic!("Expected Password auth"),
        }
    }

    #[test]
    fn command_result_fields() {
        let result = CommandResult {
            stdout: "output".to_string(),
            stderr: "error".to_string(),
            exit_code: 0,
        };
        assert_eq!(result.stdout, "output");
        assert_eq!(result.stderr, "error");
        assert_eq!(result.exit_code, 0);
    }
}
