use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let name = match args.require("name") {
        Ok(n) => n.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get("state");
    let enabled = args.get("enabled");

    let mut changed = false;

    // Handle enabled state
    if let Some(en) = enabled {
        let should_enable = en == "true" || en == "yes";
        let is_enabled = conn
            .exec(&format!("systemctl is-enabled {} 2>/dev/null", name))
            .map(|r| r.exit_code == 0)
            .unwrap_or(false);

        if should_enable && !is_enabled {
            match conn.exec(&format!("systemctl enable {}", name)) {
                Ok(r) if r.exit_code == 0 => changed = true,
                _ => return ModuleResult::failed(&format!("failed to enable {}", name)),
            }
        } else if !should_enable && is_enabled {
            match conn.exec(&format!("systemctl disable {}", name)) {
                Ok(r) if r.exit_code == 0 => changed = true,
                _ => return ModuleResult::failed(&format!("failed to disable {}", name)),
            }
        }
    }

    // Handle running state
    if let Some(st) = state {
        let is_running = conn
            .exec(&format!("systemctl is-active {} 2>/dev/null", name))
            .map(|r| r.exit_code == 0)
            .unwrap_or(false);

        match st.as_str() {
            "started" => {
                if !is_running {
                    match conn.exec(&format!("systemctl start {}", name)) {
                        Ok(r) if r.exit_code == 0 => changed = true,
                        Ok(r) => return ModuleResult::failed(&format!("failed to start: {}", r.stderr)),
                        Err(e) => return ModuleResult::failed(&format!("failed to start: {}", e)),
                    }
                }
            }
            "stopped" => {
                if is_running {
                    match conn.exec(&format!("systemctl stop {}", name)) {
                        Ok(r) if r.exit_code == 0 => changed = true,
                        Ok(r) => return ModuleResult::failed(&format!("failed to stop: {}", r.stderr)),
                        Err(e) => return ModuleResult::failed(&format!("failed to stop: {}", e)),
                    }
                }
            }
            "restarted" => {
                match conn.exec(&format!("systemctl restart {}", name)) {
                    Ok(r) if r.exit_code == 0 => changed = true,
                    Ok(r) => return ModuleResult::failed(&format!("failed to restart: {}", r.stderr)),
                    Err(e) => return ModuleResult::failed(&format!("failed to restart: {}", e)),
                }
            }
            "reloaded" => {
                match conn.exec(&format!("systemctl reload {}", name)) {
                    Ok(r) if r.exit_code == 0 => changed = true,
                    Ok(r) => return ModuleResult::failed(&format!("failed to reload: {}", r.stderr)),
                    Err(e) => return ModuleResult::failed(&format!("failed to reload: {}", e)),
                }
            }
            _ => return ModuleResult::failed(&format!("unknown state: {}", st)),
        }
    }

    if changed {
        ModuleResult::changed("service updated")
    } else {
        ModuleResult::ok("service unchanged")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_name() {
        let args = ModuleArgs::new();
        assert!(args.require("name").is_err());
    }
}
