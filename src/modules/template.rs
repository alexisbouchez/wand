use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;
use crate::template as tpl;
use std::collections::HashMap;
use std::path::Path;

pub fn run(conn: &SshConnection, args: &ModuleArgs, vars: &HashMap<String, String>) -> ModuleResult {
    let src = match args.require("src") {
        Ok(s) => s.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let dest = match args.require("dest") {
        Ok(d) => d.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let mode = args.get_or("mode", "0644");
    let mode_int = i32::from_str_radix(&mode, 8).unwrap_or(0o644);

    // Read local template
    let src_path = Path::new(&src);
    let template_content = match std::fs::read_to_string(src_path) {
        Ok(c) => c,
        Err(e) => return ModuleResult::failed(&format!("failed to read template: {}", e)),
    };

    // Render template
    let rendered = tpl::render(&template_content, vars);

    // Check if remote file exists and has same content
    match conn.read_file(&dest) {
        Ok(existing) => {
            if existing == rendered.as_bytes() {
                return ModuleResult::ok("template unchanged");
            }
        }
        Err(_) => {} // File doesn't exist, will create
    }

    // Write rendered content
    match conn.write_file(&dest, rendered.as_bytes(), mode_int) {
        Ok(_) => ModuleResult::changed("template rendered"),
        Err(e) => ModuleResult::failed(&format!("failed to write template: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_src() {
        let args = ModuleArgs::new();
        assert!(args.require("src").is_err());
    }

    #[test]
    fn requires_dest() {
        let mut args = ModuleArgs::new();
        args.insert("src", "/tmp/template.j2");
        assert!(args.require("dest").is_err());
    }
}
