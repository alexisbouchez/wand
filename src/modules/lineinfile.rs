use super::{ModuleArgs, ModuleResult};
use crate::ssh::SshConnection;

pub fn run(conn: &SshConnection, args: &ModuleArgs) -> ModuleResult {
    let path = match args.require("path") {
        Ok(p) => p.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get_or("state", "present");

    // Read current file content
    let content = match conn.read_file(&path) {
        Ok(c) => String::from_utf8_lossy(&c).to_string(),
        Err(_) if state == "absent" => return ModuleResult::ok("file does not exist"),
        Err(_) => String::new(), // File will be created
    };

    let lines: Vec<&str> = content.lines().collect();

    match state.as_str() {
        "present" => {
            let line = match args.require("line") {
                Ok(l) => l.clone(),
                Err(e) => return ModuleResult::failed(&e),
            };

            let regexp = args.get("regexp");
            let insertafter = args.get("insertafter");
            let insertbefore = args.get("insertbefore");
            let create = args.get_bool("create");

            // Check if line already exists
            if lines.iter().any(|l| *l == line) {
                return ModuleResult::ok("line already present");
            }

            let new_content = if let Some(re) = regexp {
                // Replace matching line
                let mut found = false;
                let new_lines: Vec<String> = lines
                    .iter()
                    .map(|l| {
                        if l.contains(re.as_str()) {
                            found = true;
                            line.clone()
                        } else {
                            l.to_string()
                        }
                    })
                    .collect();

                if found {
                    new_lines.join("\n")
                } else {
                    // Append if no match
                    format!("{}\n{}", content.trim_end(), line)
                }
            } else if let Some(after) = insertafter {
                insert_after(&lines, after, &line)
            } else if let Some(before) = insertbefore {
                insert_before(&lines, before, &line)
            } else {
                // Append to end
                if content.is_empty() && !create {
                    return ModuleResult::failed("file does not exist and create=false");
                }
                format!("{}\n{}", content.trim_end(), line)
            };

            match conn.write_file(&path, new_content.as_bytes(), 0o644) {
                Ok(_) => ModuleResult::changed("line added"),
                Err(e) => ModuleResult::failed(&format!("failed to write file: {}", e)),
            }
        }
        "absent" => {
            let line = args.get("line");
            let regexp = args.get("regexp");

            if line.is_none() && regexp.is_none() {
                return ModuleResult::failed("either 'line' or 'regexp' required for state=absent");
            }

            let new_lines: Vec<&str> = lines
                .iter()
                .filter(|l| {
                    if let Some(ln) = line {
                        if **l == ln.as_str() {
                            return false;
                        }
                    }
                    if let Some(re) = regexp {
                        if l.contains(re.as_str()) {
                            return false;
                        }
                    }
                    true
                })
                .copied()
                .collect();

            if new_lines.len() == lines.len() {
                return ModuleResult::ok("line not found");
            }

            let new_content = new_lines.join("\n");

            match conn.write_file(&path, new_content.as_bytes(), 0o644) {
                Ok(_) => ModuleResult::changed("line removed"),
                Err(e) => ModuleResult::failed(&format!("failed to write file: {}", e)),
            }
        }
        _ => ModuleResult::failed(&format!("unknown state: {}", state)),
    }
}

fn insert_after(lines: &[&str], after: &str, line: &str) -> String {
    let mut result = Vec::new();
    let mut inserted = false;

    for l in lines {
        result.push(l.to_string());
        if !inserted && (after == "EOF" || l.contains(after)) {
            result.push(line.to_string());
            inserted = true;
        }
    }

    if !inserted {
        result.push(line.to_string());
    }

    result.join("\n")
}

fn insert_before(lines: &[&str], before: &str, line: &str) -> String {
    let mut result = Vec::new();
    let mut inserted = false;

    for l in lines {
        if !inserted && (before == "BOF" || l.contains(before)) {
            result.push(line.to_string());
            inserted = true;
        }
        result.push(l.to_string());
    }

    if !inserted {
        result.insert(0, line.to_string());
    }

    result.join("\n")
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
    fn insert_after_works() {
        let lines = vec!["first", "second", "third"];
        let result = insert_after(&lines, "second", "new");
        assert_eq!(result, "first\nsecond\nnew\nthird");
    }

    #[test]
    fn insert_before_works() {
        let lines = vec!["first", "second", "third"];
        let result = insert_before(&lines, "second", "new");
        assert_eq!(result, "first\nnew\nsecond\nthird");
    }
}
