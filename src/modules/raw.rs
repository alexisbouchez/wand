use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let cmd = match args.get("_raw") {
        Some(c) => c.clone(),
        None => match args.require("cmd") {
            Ok(c) => c.clone(),
            Err(e) => return ModuleResult::failed(&e),
        },
    };

    let chdir = args.get("chdir");
    let creates = args.get("creates");
    let removes = args.get("removes");

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

    let full_cmd = if let Some(dir) = chdir {
        format!("cd {} && {}", dir, cmd)
    } else {
        cmd
    };

    match conn.exec(&full_cmd) {
        Ok(result) => ModuleResult::changed("raw command executed")
            .with_output(&result.stdout, &result.stderr, result.exit_code),
        Err(e) => ModuleResult::failed(&format!("raw command failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parsing() {
        let mut args = ModuleArgs::new();
        args.insert("_raw", "echo hello");
        assert_eq!(args.get("_raw"), Some(&"echo hello".to_string()));
    }

    #[test]
    fn missing_command_fails() {
        let args = ModuleArgs::new();
        assert!(args.require("cmd").is_err());
    }
}
