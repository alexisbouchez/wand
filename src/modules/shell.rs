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
    let executable = args.get_or("executable", "/bin/sh");

    // Check creates condition
    if let Some(path) = creates {
        match conn.exec(&format!("test -e {}", path)) {
            Ok(result) if result.exit_code == 0 => {
                return ModuleResult::ok("skipped, creates exists");
            }
            _ => {}
        }
    }

    // Check removes condition
    if let Some(path) = removes {
        match conn.exec(&format!("test -e {}", path)) {
            Ok(result) if result.exit_code != 0 => {
                return ModuleResult::ok("skipped, removes does not exist");
            }
            _ => {}
        }
    }

    let full_cmd = if let Some(dir) = chdir {
        format!("cd {} && {} -c '{}'", dir, executable, cmd.replace('\'', "'\\''"))
    } else {
        format!("{} -c '{}'", executable, cmd.replace('\'', "'\\''"))
    };

    match conn.exec(&full_cmd) {
        Ok(result) => ModuleResult::changed("shell command executed")
            .with_output(&result.stdout, &result.stderr, result.exit_code),
        Err(e) => ModuleResult::failed(&format!("shell command failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_executable() {
        let args = ModuleArgs::new();
        assert_eq!(args.get_or("executable", "/bin/sh"), "/bin/sh");
    }
}
