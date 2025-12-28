use crate::inventory::Inventory;
use crate::modules::{self, ModuleArgs, ModuleResult};
use crate::playbook::{Play, Task};
use crate::ssh::{Auth, SshConnection};
use crate::template;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Executor {
    inventory: Inventory,
    vars: HashMap<String, String>,
    check_mode: bool,
    diff_mode: bool,
}

#[derive(Debug, Default)]
pub struct PlayResult {
    pub host: String,
    pub ok: usize,
    pub changed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub task_results: Vec<TaskResult>,
}

#[derive(Debug)]
pub struct TaskResult {
    pub task_name: String,
    pub host: String,
    pub result: ModuleResult,
}

impl Executor {
    pub fn new(inventory: Inventory) -> Self {
        Self {
            inventory,
            vars: HashMap::new(),
            check_mode: false,
            diff_mode: false,
        }
    }

    pub fn with_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.vars = vars;
        self
    }

    pub fn check_mode(mut self, enabled: bool) -> Self {
        self.check_mode = enabled;
        self
    }

    pub fn diff_mode(mut self, enabled: bool) -> Self {
        self.diff_mode = enabled;
        self
    }

    pub fn run_play(&self, play: &Play, auth: &Auth) -> Vec<PlayResult> {
        let hosts = self.resolve_hosts(&play.hosts);
        let mut results = Vec::new();

        for host_name in hosts {
            let result = self.run_play_on_host(play, &host_name, auth);
            results.push(result);
        }

        results
    }

    fn resolve_hosts(&self, pattern: &str) -> Vec<String> {
        if pattern == "all" {
            return self.inventory.hosts.keys().cloned().collect();
        }

        if pattern == "localhost" {
            return vec!["localhost".to_string()];
        }

        // Check if it's a group
        if let Some(group) = self.inventory.groups.get(pattern) {
            return group.hosts.clone();
        }

        // Check if it's a host
        if self.inventory.hosts.contains_key(pattern) {
            return vec![pattern.to_string()];
        }

        // Comma-separated list
        pattern
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|h| self.inventory.hosts.contains_key(h))
            .collect()
    }

    fn run_play_on_host(&self, play: &Play, host_name: &str, auth: &Auth) -> PlayResult {
        let mut result = PlayResult {
            host: host_name.to_string(),
            ..Default::default()
        };

        // Get host info
        let host = match self.inventory.hosts.get(host_name) {
            Some(h) => h,
            None => {
                result.failed = 1;
                return result;
            }
        };

        // Build connection
        let connect_host = host.vars.get("ansible_host").unwrap_or(&host.name);
        let port: u16 = host
            .vars
            .get("ansible_port")
            .and_then(|p| p.parse().ok())
            .unwrap_or(22);
        let user = host
            .vars
            .get("ansible_user")
            .cloned()
            .unwrap_or_else(|| "root".to_string());

        let conn = match SshConnection::connect(connect_host, port, &user, auth.clone()) {
            Ok(c) => c,
            Err(e) => {
                result.failed = 1;
                result.task_results.push(TaskResult {
                    task_name: "CONNECT".to_string(),
                    host: host_name.to_string(),
                    result: ModuleResult::failed(&format!("connection failed: {}", e)),
                });
                return result;
            }
        };

        // Build variables for this host
        let mut host_vars = self.vars.clone();
        host_vars.insert("inventory_hostname".to_string(), host_name.to_string());
        host_vars.insert("ansible_host".to_string(), connect_host.to_string());
        for (k, v) in &host.vars {
            host_vars.insert(k.clone(), v.clone());
        }

        // Add play vars
        for (k, v) in &play.vars {
            if let Some(s) = v.as_str() {
                host_vars.insert(k.clone(), s.to_string());
            }
        }

        // Track notified handlers
        let mut notified_handlers: HashSet<String> = HashSet::new();

        // Execute tasks
        for task in &play.tasks {
            let task_result = self.run_task(&conn, task, &mut host_vars, &mut notified_handlers);

            match &task_result.result {
                r if r.failed => result.failed += 1,
                r if r.changed => result.changed += 1,
                _ => result.ok += 1,
            }

            result.task_results.push(task_result);
        }

        // Execute notified handlers
        for handler in &play.handlers {
            if let Some(name) = &handler.name {
                if notified_handlers.contains(name) {
                    let task_result = self.run_task(&conn, handler, &mut host_vars, &mut HashSet::new());

                    match &task_result.result {
                        r if r.failed => result.failed += 1,
                        r if r.changed => result.changed += 1,
                        _ => result.ok += 1,
                    }

                    result.task_results.push(task_result);
                }
            }
        }

        result
    }

    fn run_task(
        &self,
        conn: &SshConnection,
        task: &Task,
        vars: &mut HashMap<String, String>,
        notified: &mut HashSet<String>,
    ) -> TaskResult {
        let task_name = task.name.clone().unwrap_or_else(|| "unnamed".to_string());

        // Check 'when' condition
        if let Some(when) = &task.when {
            let rendered = template::render(when, vars);
            if !eval_when(&rendered, vars) {
                return TaskResult {
                    task_name,
                    host: conn.host().to_string(),
                    result: ModuleResult::ok("skipped"),
                };
            }
        }

        // Find module and args
        let (module_name, module_args) = match extract_module(task, vars) {
            Some(m) => m,
            None => {
                return TaskResult {
                    task_name,
                    host: conn.host().to_string(),
                    result: ModuleResult::failed("no module found in task"),
                };
            }
        };

        // Execute module
        let result = if self.check_mode {
            ModuleResult::ok("check mode")
        } else {
            run_module(conn, &module_name, &module_args, vars)
        };

        // Handle register
        if let Some(reg) = &task.register {
            vars.insert(format!("{}.stdout", reg), result.stdout.clone());
            vars.insert(format!("{}.stderr", reg), result.stderr.clone());
            vars.insert(format!("{}.rc", reg), result.rc.to_string());
            vars.insert(format!("{}.changed", reg), result.changed.to_string());
            vars.insert(format!("{}.failed", reg), result.failed.to_string());
        }

        // Handle notify
        if result.changed {
            if let Some(notify_list) = &task.notify {
                for handler_name in notify_list {
                    notified.insert(handler_name.clone());
                }
            }
        }

        TaskResult {
            task_name,
            host: conn.host().to_string(),
            result,
        }
    }
}

fn extract_module(task: &Task, vars: &HashMap<String, String>) -> Option<(String, ModuleArgs)> {
    let known_modules = [
        "command", "shell", "copy", "file", "template",
        "apt", "service", "lineinfile", "raw", "script",
        "yum", "dnf", "pip", "user", "group",
    ];

    for (key, value) in &task.module {
        if known_modules.contains(&key.as_str()) {
            let mut args = ModuleArgs::new();

            // Handle string arg (e.g., command: "echo hello")
            if let Some(s) = value.as_str() {
                args.insert("_raw", &template::render(s, vars));
            }

            // Handle map args (e.g., apt: { name: nginx, state: present })
            if let Some(map) = value.as_mapping() {
                for (k, v) in map {
                    if let (Some(key_str), Some(val_str)) = (k.as_str(), v.as_str()) {
                        args.insert(key_str, &template::render(val_str, vars));
                    } else if let (Some(key_str), Some(val_bool)) = (k.as_str(), v.as_bool()) {
                        args.insert(key_str, if val_bool { "true" } else { "false" });
                    }
                }
            }

            return Some((key.clone(), args));
        }
    }

    None
}

fn run_module(
    conn: &SshConnection,
    module: &str,
    args: &ModuleArgs,
    vars: &HashMap<String, String>,
) -> ModuleResult {
    match module {
        "command" => modules::command::run(conn, args),
        "shell" => modules::shell::run(conn, args),
        "copy" => modules::copy::run(conn, args),
        "file" => modules::file::run(conn, args),
        "template" => modules::template::run(conn, args, vars),
        "apt" => modules::apt::run(conn, args),
        "service" => modules::service::run(conn, args),
        "lineinfile" => modules::lineinfile::run(conn, args),
        _ => ModuleResult::failed(&format!("unknown module: {}", module)),
    }
}

fn eval_when(condition: &str, vars: &HashMap<String, String>) -> bool {
    let condition = condition.trim();

    // Handle comparison operators
    if let Some((left, right)) = condition.split_once("==") {
        let left = eval_expr(left.trim(), vars);
        let right = eval_expr(right.trim(), vars);
        return left == right;
    }

    if let Some((left, right)) = condition.split_once("!=") {
        let left = eval_expr(left.trim(), vars);
        let right = eval_expr(right.trim(), vars);
        return left != right;
    }

    if condition.starts_with("not ") {
        let inner = condition.strip_prefix("not ").unwrap().trim();
        return !eval_when(inner, vars);
    }

    // Check defined/undefined
    if condition.ends_with(" is defined") {
        let var = condition.strip_suffix(" is defined").unwrap().trim();
        return vars.contains_key(var);
    }

    if condition.ends_with(" is undefined") {
        let var = condition.strip_suffix(" is undefined").unwrap().trim();
        return !vars.contains_key(var);
    }

    // Truthy check
    let value = vars.get(condition).cloned().unwrap_or_default();
    !value.is_empty() && value != "false" && value != "0"
}

fn eval_expr(expr: &str, vars: &HashMap<String, String>) -> String {
    let expr = expr.trim();
    if expr.starts_with('"') || expr.starts_with('\'') {
        expr.trim_matches(|c| c == '"' || c == '\'').to_string()
    } else {
        vars.get(expr).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn eval_when_equals() {
        assert!(eval_when("os == \"linux\"", &vars(&[("os", "linux")])));
        assert!(!eval_when("os == \"linux\"", &vars(&[("os", "windows")])));
    }

    #[test]
    fn eval_when_not_equals() {
        assert!(eval_when("os != \"windows\"", &vars(&[("os", "linux")])));
    }

    #[test]
    fn eval_when_defined() {
        assert!(eval_when("myvar is defined", &vars(&[("myvar", "value")])));
        assert!(!eval_when("myvar is defined", &vars(&[])));
    }

    #[test]
    fn eval_when_undefined() {
        assert!(eval_when("myvar is undefined", &vars(&[])));
        assert!(!eval_when("myvar is undefined", &vars(&[("myvar", "value")])));
    }

    #[test]
    fn eval_when_truthy() {
        assert!(eval_when("enabled", &vars(&[("enabled", "true")])));
        assert!(!eval_when("enabled", &vars(&[("enabled", "false")])));
        assert!(!eval_when("enabled", &vars(&[])));
    }

    #[test]
    fn eval_when_not() {
        assert!(eval_when("not disabled", &vars(&[("disabled", "false")])));
        assert!(!eval_when("not enabled", &vars(&[("enabled", "true")])));
    }

    #[test]
    fn resolve_hosts_all() {
        let mut inv = Inventory::default();
        inv.hosts.insert("host1".to_string(), crate::inventory::Host {
            name: "host1".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("host2".to_string(), crate::inventory::Host {
            name: "host2".to_string(),
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv);
        let hosts = exec.resolve_hosts("all");
        assert_eq!(hosts.len(), 2);
    }

    #[test]
    fn resolve_hosts_single() {
        let mut inv = Inventory::default();
        inv.hosts.insert("host1".to_string(), crate::inventory::Host {
            name: "host1".to_string(),
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv);
        let hosts = exec.resolve_hosts("host1");
        assert_eq!(hosts, vec!["host1".to_string()]);
    }

    #[test]
    fn resolve_hosts_group() {
        let mut inv = Inventory::default();
        inv.hosts.insert("web1".to_string(), crate::inventory::Host {
            name: "web1".to_string(),
            vars: HashMap::new(),
        });
        inv.groups.insert("webservers".to_string(), crate::inventory::Group {
            name: "webservers".to_string(),
            hosts: vec!["web1".to_string()],
            children: vec![],
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv);
        let hosts = exec.resolve_hosts("webservers");
        assert_eq!(hosts, vec!["web1".to_string()]);
    }

    #[test]
    fn play_result_default() {
        let result = PlayResult::default();
        assert_eq!(result.ok, 0);
        assert_eq!(result.changed, 0);
        assert_eq!(result.failed, 0);
    }
}
