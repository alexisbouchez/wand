use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;
use std::path::Path;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let dest = match args.require("dest") {
        Ok(d) => d.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let mode = args.get_or("mode", "0644");
    let mode_int = i32::from_str_radix(&mode, 8).unwrap_or(0o644);

    // Either src (file) or content (inline)
    if let Some(content) = args.get("content") {
        // Check if file exists and has same content
        match conn.read_file(&dest) {
            Ok(existing) => {
                if existing == content.as_bytes() {
                    return ModuleResult::ok("content unchanged");
                }
            }
            Err(_) => {} // File doesn't exist, will create
        }

        match conn.write_file(&dest, content.as_bytes(), mode_int) {
            Ok(_) => ModuleResult::changed("content copied"),
            Err(e) => ModuleResult::failed(&format!("failed to write content: {}", e)),
        }
    } else if let Some(src) = args.get("src") {
        let src_path = Path::new(src);

        if !src_path.exists() {
            return ModuleResult::failed(&format!("source file not found: {}", src));
        }

        // Read local file
        let content = match std::fs::read(src_path) {
            Ok(c) => c,
            Err(e) => return ModuleResult::failed(&format!("failed to read source: {}", e)),
        };

        // Check if remote file exists and has same content
        match conn.read_file(&dest) {
            Ok(existing) => {
                if existing == content {
                    return ModuleResult::ok("file unchanged");
                }
            }
            Err(_) => {} // File doesn't exist, will create
        }

        match conn.write_file(&dest, &content, mode_int) {
            Ok(_) => ModuleResult::changed("file copied"),
            Err(e) => ModuleResult::failed(&format!("failed to copy file: {}", e)),
        }
    } else {
        ModuleResult::failed("either 'src' or 'content' is required")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_dest() {
        let args = ModuleArgs::new();
        assert!(args.require("dest").is_err());
    }

    #[test]
    fn requires_src_or_content() {
        let mut args = ModuleArgs::new();
        args.insert("dest", "/tmp/test");
        // Would fail with "either 'src' or 'content' is required"
        // Can't test without SSH connection
    }

    #[test]
    fn mode_parsing() {
        let mode = "0755";
        let mode_int = i32::from_str_radix(mode, 8).unwrap();
        assert_eq!(mode_int, 0o755);
    }
}
