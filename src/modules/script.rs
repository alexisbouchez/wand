use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;
use std::path::Path;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let script_path = match args.get("_raw_params") {
        Some(p) => p.clone(),
        None => match args.require("cmd") {
            Ok(p) => p.clone(),
            Err(e) => return ModuleResult::failed(&e),
        },
    };

    let chdir = args.get("chdir");
    let creates = args.get("creates");
    let removes = args.get("removes");

    if !Path::new(&script_path).exists() {
        return ModuleResult::failed(&format!("script not found: {}", script_path));
    }

    if let Some(path) = creates {
        match conn.exec(&format!("test -e {}", path)) {
            Ok(result) if result.exit_code == 0 => {
                return ModuleResult::ok("skipped, creates exists");
            }
            _ => {}
        }
    }

    if let Some(path) = removes {
        match conn.exec(&format!("test -e {}", path)) {
            Ok(result) if result.exit_code != 0 => {
                return ModuleResult::ok("skipped, removes does not exist");
            }
            _ => {}
        }
    }

    let script_content = match std::fs::read(&script_path) {
        Ok(content) => content,
        Err(e) => return ModuleResult::failed(&format!("failed to read script: {}", e)),
    };

    let remote_path = "/tmp/.ansible_script";

    match conn.write_file(remote_path, &script_content, 0o700) {
        Ok(_) => {}
        Err(e) => return ModuleResult::failed(&format!("failed to upload script: {}", e)),
    }

    let full_cmd = if let Some(dir) = chdir {
        format!("cd {} && {}", dir, remote_path)
    } else {
        remote_path.to_string()
    };

    match conn.exec(&full_cmd) {
        Ok(result) => ModuleResult::changed("script executed")
            .with_output(&result.stdout, &result.stderr, result.exit_code),
        Err(e) => ModuleResult::failed(&format!("script failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parsing() {
        let mut args = ModuleArgs::new();
        args.insert("_raw_params", "/tmp/script.sh");
        assert_eq!(args.get("_raw_params"), Some(&"/tmp/script.sh".to_string()));
    }

    #[test]
    fn missing_script_fails() {
        let args = ModuleArgs::new();
        assert!(args.require("cmd").is_err());
    }
}
