use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod apt;
pub mod command;
pub mod copy;
pub mod file;
pub mod lineinfile;
pub mod raw;
pub mod script;
pub mod service;
pub mod shell;
pub mod template;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleResult {
    pub changed: bool,
    pub failed: bool,
    pub msg: String,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub rc: i32,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

impl ModuleResult {
    pub fn ok(msg: &str) -> Self {
        Self {
            changed: false,
            failed: false,
            msg: msg.to_string(),
            stdout: String::new(),
            stderr: String::new(),
            rc: 0,
            diff: None,
            extra: HashMap::new(),
        }
    }

    pub fn changed(msg: &str) -> Self {
        Self {
            changed: true,
            failed: false,
            msg: msg.to_string(),
            stdout: String::new(),
            stderr: String::new(),
            rc: 0,
            diff: None,
            extra: HashMap::new(),
        }
    }

    pub fn failed(msg: &str) -> Self {
        Self {
            changed: false,
            failed: true,
            msg: msg.to_string(),
            stdout: String::new(),
            stderr: String::new(),
            rc: 1,
            diff: None,
            extra: HashMap::new(),
        }
    }

    pub fn with_output(mut self, stdout: &str, stderr: &str, rc: i32) -> Self {
        self.stdout = stdout.to_string();
        self.stderr = stderr.to_string();
        self.rc = rc;
        if rc != 0 {
            self.failed = true;
        }
        self
    }

    pub fn with_diff(mut self, diff: String) -> Self {
        self.diff = Some(diff);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ModuleArgs {
    args: HashMap<String, String>,
}

impl ModuleArgs {
    pub fn new() -> Self {
        Self {
            args: HashMap::new(),
        }
    }

    pub fn from_map(map: HashMap<String, String>) -> Self {
        Self { args: map }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.args.get(key)
    }

    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.args.get(key).cloned().unwrap_or(default.to_string())
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.args
            .get(key)
            .map(|v| v == "true" || v == "yes" || v == "1")
            .unwrap_or(false)
    }

    pub fn require(&self, key: &str) -> Result<&String, String> {
        self.args
            .get(key)
            .ok_or_else(|| format!("missing required argument: {}", key))
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        self.args.insert(key.to_string(), value.to_string());
    }
}

impl Default for ModuleArgs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_result_ok() {
        let result = ModuleResult::ok("all good");
        assert!(!result.changed);
        assert!(!result.failed);
        assert_eq!(result.msg, "all good");
    }

    #[test]
    fn module_result_changed() {
        let result = ModuleResult::changed("file created");
        assert!(result.changed);
        assert!(!result.failed);
    }

    #[test]
    fn module_result_failed() {
        let result = ModuleResult::failed("error occurred");
        assert!(!result.changed);
        assert!(result.failed);
    }

    #[test]
    fn module_result_with_output() {
        let result = ModuleResult::changed("done").with_output("out", "err", 0);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.rc, 0);
        assert!(!result.failed);
    }

    #[test]
    fn module_result_with_nonzero_exit() {
        let result = ModuleResult::changed("done").with_output("", "error", 1);
        assert!(result.failed);
        assert_eq!(result.rc, 1);
    }

    #[test]
    fn module_args_get() {
        let mut args = ModuleArgs::new();
        args.insert("name", "value");
        assert_eq!(args.get("name"), Some(&"value".to_string()));
        assert_eq!(args.get("missing"), None);
    }

    #[test]
    fn module_args_get_or() {
        let args = ModuleArgs::new();
        assert_eq!(args.get_or("missing", "default"), "default");
    }

    #[test]
    fn module_args_get_bool() {
        let mut args = ModuleArgs::new();
        args.insert("enabled", "true");
        args.insert("disabled", "false");
        assert!(args.get_bool("enabled"));
        assert!(!args.get_bool("disabled"));
        assert!(!args.get_bool("missing"));
    }

    #[test]
    fn module_args_require() {
        let mut args = ModuleArgs::new();
        args.insert("name", "value");
        assert!(args.require("name").is_ok());
        assert!(args.require("missing").is_err());
    }
}
