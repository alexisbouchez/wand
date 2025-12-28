use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let name = match args.require("name") {
        Ok(n) => n.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get_or("state", "present");
    let update_cache = args.get_bool("update_cache");

    if update_cache {
        match conn.exec("apt-get update -qq") {
            Ok(r) if r.exit_code != 0 => {
                return ModuleResult::failed("apt-get update failed");
            }
            Err(e) => return ModuleResult::failed(&format!("apt-get update failed: {}", e)),
            _ => {}
        }
    }

    // Check current state
    let is_installed = conn
        .exec(&format!("dpkg-query -W -f='${{Status}}' {} 2>/dev/null | grep -q 'ok installed'", name))
        .map(|r| r.exit_code == 0)
        .unwrap_or(false);

    match state.as_str() {
        "present" | "installed" => {
            if is_installed {
                return ModuleResult::ok("package already installed");
            }

            match conn.exec(&format!("DEBIAN_FRONTEND=noninteractive apt-get install -y -qq {}", name)) {
                Ok(r) if r.exit_code == 0 => ModuleResult::changed("package installed"),
                Ok(r) => ModuleResult::failed(&format!("apt install failed: {}", r.stderr)),
                Err(e) => ModuleResult::failed(&format!("apt install failed: {}", e)),
            }
        }
        "absent" | "removed" => {
            if !is_installed {
                return ModuleResult::ok("package already absent");
            }

            match conn.exec(&format!("DEBIAN_FRONTEND=noninteractive apt-get remove -y -qq {}", name)) {
                Ok(r) if r.exit_code == 0 => ModuleResult::changed("package removed"),
                Ok(r) => ModuleResult::failed(&format!("apt remove failed: {}", r.stderr)),
                Err(e) => ModuleResult::failed(&format!("apt remove failed: {}", e)),
            }
        }
        "latest" => {
            let cmd = if is_installed {
                format!("DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --only-upgrade {}", name)
            } else {
                format!("DEBIAN_FRONTEND=noninteractive apt-get install -y -qq {}", name)
            };

            match conn.exec(&cmd) {
                Ok(r) if r.exit_code == 0 => {
                    if r.stdout.contains("0 upgraded") && is_installed {
                        ModuleResult::ok("package already latest")
                    } else {
                        ModuleResult::changed("package installed/upgraded")
                    }
                }
                Ok(r) => ModuleResult::failed(&format!("apt install failed: {}", r.stderr)),
                Err(e) => ModuleResult::failed(&format!("apt install failed: {}", e)),
            }
        }
        _ => ModuleResult::failed(&format!("unknown state: {}", state)),
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

    #[test]
    fn default_state_present() {
        let args = ModuleArgs::new();
        assert_eq!(args.get_or("state", "present"), "present");
    }
}
