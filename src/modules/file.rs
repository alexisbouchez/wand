use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let path = match args.require("path") {
        Ok(p) => p.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get_or("state", "file");
    let mode = args.get("mode");
    let owner = args.get("owner");
    let group = args.get("group");

    match state.as_str() {
        "file" => ensure_file(conn, &path, mode, owner, group),
        "directory" => ensure_directory(conn, &path, mode, owner, group),
        "absent" => ensure_absent(conn, &path),
        "link" => {
            let src = match args.require("src") {
                Ok(s) => s.clone(),
                Err(e) => return ModuleResult::failed(&e),
            };
            ensure_link(conn, &path, &src)
        }
        "touch" => ensure_touch(conn, &path, mode, owner, group),
        _ => ModuleResult::failed(&format!("unknown state: {}", state)),
    }
}

fn ensure_file(
    conn: &SshConnection,
    path: &str,
    mode: Option<&String>,
    owner: Option<&String>,
    group: Option<&String>,
) -> ModuleResult {
    // Check if file exists
    let exists = conn
        .exec(&format!("test -f {}", path))
        .map(|r| r.exit_code == 0)
        .unwrap_or(false);

    if !exists {
        return ModuleResult::failed(&format!("path does not exist: {}", path));
    }

    let mut changed = false;

    if let Some(m) = mode {
        if conn.exec(&format!("chmod {} {}", m, path)).is_ok() {
            changed = true;
        }
    }

    if let Some(o) = owner {
        if conn.exec(&format!("chown {} {}", o, path)).is_ok() {
            changed = true;
        }
    }

    if let Some(g) = group {
        if conn.exec(&format!("chgrp {} {}", g, path)).is_ok() {
            changed = true;
        }
    }

    if changed {
        ModuleResult::changed("file attributes updated")
    } else {
        ModuleResult::ok("file unchanged")
    }
}

fn ensure_directory(
    conn: &SshConnection,
    path: &str,
    mode: Option<&String>,
    owner: Option<&String>,
    group: Option<&String>,
) -> ModuleResult {
    let exists = conn
        .exec(&format!("test -d {}", path))
        .map(|r| r.exit_code == 0)
        .unwrap_or(false);

    let mut changed = false;

    if !exists {
        match conn.exec(&format!("mkdir -p {}", path)) {
            Ok(r) if r.exit_code == 0 => changed = true,
            _ => return ModuleResult::failed(&format!("failed to create directory: {}", path)),
        }
    }

    if let Some(m) = mode {
        if conn.exec(&format!("chmod {} {}", m, path)).is_ok() {
            changed = true;
        }
    }

    if let Some(o) = owner {
        if conn.exec(&format!("chown {} {}", o, path)).is_ok() {
            changed = true;
        }
    }

    if let Some(g) = group {
        if conn.exec(&format!("chgrp {} {}", g, path)).is_ok() {
            changed = true;
        }
    }

    if changed {
        ModuleResult::changed("directory created/updated")
    } else {
        ModuleResult::ok("directory unchanged")
    }
}

fn ensure_absent(conn: &SshConnection, path: &str) -> ModuleResult {
    let exists = conn
        .exec(&format!("test -e {}", path))
        .map(|r| r.exit_code == 0)
        .unwrap_or(false);

    if !exists {
        return ModuleResult::ok("path already absent");
    }

    match conn.exec(&format!("rm -rf {}", path)) {
        Ok(r) if r.exit_code == 0 => ModuleResult::changed("path removed"),
        _ => ModuleResult::failed(&format!("failed to remove: {}", path)),
    }
}

fn ensure_link(conn: &SshConnection, path: &str, src: &str) -> ModuleResult {
    // Check if link exists and points to correct target
    let current_target = conn
        .exec(&format!("readlink {}", path))
        .ok()
        .filter(|r| r.exit_code == 0)
        .map(|r| r.stdout.trim().to_string());

    if current_target.as_deref() == Some(src) {
        return ModuleResult::ok("link unchanged");
    }

    // Remove existing if needed
    let _ = conn.exec(&format!("rm -f {}", path));

    match conn.exec(&format!("ln -s {} {}", src, path)) {
        Ok(r) if r.exit_code == 0 => ModuleResult::changed("link created"),
        _ => ModuleResult::failed(&format!("failed to create link: {}", path)),
    }
}

fn ensure_touch(
    conn: &SshConnection,
    path: &str,
    mode: Option<&String>,
    owner: Option<&String>,
    group: Option<&String>,
) -> ModuleResult {
    match conn.exec(&format!("touch {}", path)) {
        Ok(r) if r.exit_code == 0 => {}
        _ => return ModuleResult::failed(&format!("failed to touch: {}", path)),
    }

    if let Some(m) = mode {
        let _ = conn.exec(&format!("chmod {} {}", m, path));
    }

    if let Some(o) = owner {
        let _ = conn.exec(&format!("chown {} {}", o, path));
    }

    if let Some(g) = group {
        let _ = conn.exec(&format!("chgrp {} {}", g, path));
    }

    ModuleResult::changed("file touched")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_path() {
        let args = ModuleArgs::new();
        assert!(args.require("path").is_err());
    }

    #[test]
    fn default_state_is_file() {
        let args = ModuleArgs::new();
        assert_eq!(args.get_or("state", "file"), "file");
    }

    #[test]
    fn link_requires_src() {
        let mut args = ModuleArgs::new();
        args.insert("path", "/tmp/link");
        args.insert("state", "link");
        assert!(args.require("src").is_err());
    }
}
