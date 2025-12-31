use crate::inventory::Inventory;
use crate::modules::{ModuleArgs, ModuleResult};
use crate::playbook::{Play, Task};
use crate::ssh::{Auth, CommandResult, LocalConnection, SshConnection};
use crate::template;
use anyhow::Result;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub enum Connection {
    Ssh(SshConnection),
    Local(LocalConnection),
}

impl Connection {
    pub fn exec(&self, command: &str) -> Result<CommandResult> {
        match self {
            Connection::Ssh(c) => c.exec(command),
            Connection::Local(c) => c.exec(command),
        }
    }

    pub fn write_file(&self, path: &str, content: &[u8], mode: i32) -> Result<()> {
        match self {
            Connection::Ssh(c) => c.write_file(path, content, mode),
            Connection::Local(c) => c.write_file(path, content, mode),
        }
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Connection::Ssh(c) => c.read_file(path),
            Connection::Local(c) => c.read_file(path),
        }
    }

    pub fn host(&self) -> &str {
        match self {
            Connection::Ssh(c) => c.host(),
            Connection::Local(c) => c.host(),
        }
    }
}

#[derive(Debug)]
pub struct Executor {
    inventory: Inventory,
    extra_vars: HashMap<String, String>,
    check_mode: bool,
    diff_mode: bool,
    forks: usize,
    tags: HashSet<String>,
    skip_tags: HashSet<String>,
    limit: Option<String>,
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
            extra_vars: HashMap::new(),
            check_mode: false,
            diff_mode: false,
            forks: 5,
            tags: HashSet::new(),
            skip_tags: HashSet::new(),
            limit: None,
        }
    }

    pub fn with_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.extra_vars = vars;
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

    pub fn forks(mut self, n: usize) -> Self {
        self.forks = n.max(1);
        self
    }

    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags.into_iter().collect();
        self
    }

    pub fn skip_tags(mut self, tags: Vec<String>) -> Self {
        self.skip_tags = tags.into_iter().collect();
        self
    }

    pub fn limit(mut self, pattern: Option<String>) -> Self {
        self.limit = pattern;
        self
    }

    fn should_run_task(&self, task: &Task) -> bool {
        // If skip_tags is set and task has any of those tags, skip it
        if !self.skip_tags.is_empty() {
            for tag in &task.tags {
                if self.skip_tags.contains(tag) {
                    return false;
                }
            }
        }

        // If tags is set, task must have at least one of those tags
        if !self.tags.is_empty() {
            // "always" tag always runs
            if task.tags.contains(&"always".to_string()) {
                return true;
            }
            // Check if any task tag matches
            for tag in &task.tags {
                if self.tags.contains(tag) {
                    return true;
                }
            }
            // No matching tags found
            return false;
        }

        // No tag filtering, run the task
        true
    }

    pub fn run_play(&self, play: &Play, auth: &Auth) -> Vec<PlayResult> {
        let hosts = self.resolve_hosts(&play.hosts);

        // Build a thread pool with forks threads
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.forks)
            .build()
            .unwrap();

        pool.install(|| {
            hosts
                .par_iter()
                .map(|host_name| self.run_play_on_host(play, host_name, auth))
                .collect()
        })
    }

    fn resolve_hosts(&self, pattern: &str) -> Vec<String> {
        let mut hosts = self.resolve_pattern(pattern);

        // Apply limit filter if set
        if let Some(limit) = &self.limit {
            let limit_hosts = self.resolve_limit(limit);
            hosts.retain(|h| limit_hosts.contains(h));
        }

        hosts
    }

    fn resolve_pattern(&self, pattern: &str) -> Vec<String> {
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

    fn resolve_limit(&self, limit: &str) -> HashSet<String> {
        let mut included: HashSet<String> = HashSet::new();
        let mut excluded: HashSet<String> = HashSet::new();

        for part in limit.split(':') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some(negated) = part.strip_prefix('!') {
                // Exclusion pattern
                for host in self.expand_limit_pattern(negated) {
                    excluded.insert(host);
                }
            } else {
                // Inclusion pattern
                for host in self.expand_limit_pattern(part) {
                    included.insert(host);
                }
            }
        }

        // If no inclusions specified, start with all hosts
        if included.is_empty() {
            included = self.inventory.hosts.keys().cloned().collect();
        }

        // Remove excluded hosts
        for host in &excluded {
            included.remove(host);
        }

        included
    }

    fn expand_limit_pattern(&self, pattern: &str) -> Vec<String> {
        // Check if it's a group
        if let Some(group) = self.inventory.groups.get(pattern) {
            return group.hosts.clone();
        }

        // Check for wildcard pattern
        if pattern.contains('*') {
            let regex_pattern = format!("^{}$", pattern.replace('*', ".*"));
            if let Ok(re) = regex::Regex::new(&regex_pattern) {
                return self
                    .inventory
                    .hosts
                    .keys()
                    .filter(|h| re.is_match(h))
                    .cloned()
                    .collect();
            }
        }

        // Check if it's a comma-separated list
        if pattern.contains(',') {
            return pattern
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|h| self.inventory.hosts.contains_key(h))
                .collect();
        }

        // Single host
        if self.inventory.hosts.contains_key(pattern) {
            return vec![pattern.to_string()];
        }

        vec![]
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
        let connection_type = host.vars.get("ansible_connection").map(|s| s.as_str());

        let conn: Connection = if connection_type == Some("local") {
            Connection::Local(LocalConnection::new())
        } else {
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

            match SshConnection::connect(connect_host, port, &user, auth.clone()) {
                Ok(c) => Connection::Ssh(c),
                Err(e) => {
                    result.failed = 1;
                    result.task_results.push(TaskResult {
                        task_name: "CONNECT".to_string(),
                        host: host_name.to_string(),
                        result: ModuleResult::failed(&format!("connection failed: {}", e)),
                    });
                    return result;
                }
            }
        };

        // Build variables for this host (order matters for precedence)
        let mut host_vars = HashMap::new();

        // 1. Host vars from inventory (lowest precedence)
        host_vars.insert("inventory_hostname".to_string(), host_name.to_string());
        host_vars.insert("ansible_host".to_string(), host.vars.get("ansible_host").unwrap_or(&host.name).clone());
        for (k, v) in &host.vars {
            host_vars.insert(k.clone(), v.clone());
        }

        // 2. Play vars
        for (k, v) in &play.vars {
            if let Some(s) = v.as_str() {
                host_vars.insert(k.clone(), s.to_string());
            }
        }

        // 3. Extra vars (highest precedence)
        for (k, v) in &self.extra_vars {
            host_vars.insert(k.clone(), v.clone());
        }

        // Track notified handlers
        let mut notified_handlers: HashSet<String> = HashSet::new();

        // Execute tasks
        for task in &play.tasks {
            // Check if task should run based on tags
            if !self.should_run_task(task) {
                let task_name = task.name.clone().unwrap_or_else(|| "unnamed".to_string());
                result.skipped += 1;
                result.task_results.push(TaskResult {
                    task_name,
                    host: conn.host().to_string(),
                    result: ModuleResult::ok("skipped (tags)"),
                });
                continue;
            }

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
        conn: &Connection,
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
    conn: &Connection,
    module: &str,
    args: &ModuleArgs,
    vars: &HashMap<String, String>,
) -> ModuleResult {
    match module {
        "command" => run_command(conn, args),
        "shell" => run_shell(conn, args),
        "raw" => run_raw(conn, args),
        "script" => run_script(conn, args),
        "copy" => run_copy(conn, args),
        "file" => run_file(conn, args),
        "template" => run_template(conn, args, vars),
        "apt" => run_apt(conn, args),
        "service" => run_service(conn, args),
        "lineinfile" => run_lineinfile(conn, args),
        _ => ModuleResult::failed(&format!("unknown module: {}", module)),
    }
}

fn run_command(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let cmd = match args.get("_raw") {
        Some(c) => c.clone(),
        None => match args.require("cmd") {
            Ok(c) => c.clone(),
            Err(e) => return ModuleResult::failed(&e),
        },
    };

    let chdir = args.get("chdir");
    let full_cmd = if let Some(dir) = chdir {
        format!("cd {} && {}", dir, cmd)
    } else {
        cmd
    };

    match conn.exec(&full_cmd) {
        Ok(result) => ModuleResult::changed("command executed")
            .with_output(&result.stdout, &result.stderr, result.exit_code),
        Err(e) => ModuleResult::failed(&format!("command failed: {}", e)),
    }
}

fn run_shell(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let cmd = match args.get("_raw") {
        Some(c) => c.clone(),
        None => match args.require("cmd") {
            Ok(c) => c.clone(),
            Err(e) => return ModuleResult::failed(&e),
        },
    };

    let chdir = args.get("chdir");
    let full_cmd = if let Some(dir) = chdir {
        format!("cd {} && sh -c '{}'", dir, cmd.replace('\'', "'\\''"))
    } else {
        format!("sh -c '{}'", cmd.replace('\'', "'\\''"))
    };

    match conn.exec(&full_cmd) {
        Ok(result) => ModuleResult::changed("shell executed")
            .with_output(&result.stdout, &result.stderr, result.exit_code),
        Err(e) => ModuleResult::failed(&format!("shell failed: {}", e)),
    }
}

fn run_raw(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let cmd = match args.get("_raw") {
        Some(c) => c.clone(),
        None => match args.require("cmd") {
            Ok(c) => c.clone(),
            Err(e) => return ModuleResult::failed(&e),
        },
    };

    let chdir = args.get("chdir");

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

fn run_script(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
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

    if !std::path::Path::new(&script_path).exists() {
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

fn run_copy(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let dest = match args.require("dest") {
        Ok(d) => d.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let mode = args.get_or("mode", "0644");
    let mode_int = i32::from_str_radix(&mode, 8).unwrap_or(0o644);

    if let Some(content) = args.get("content") {
        match conn.read_file(&dest) {
            Ok(existing) if existing == content.as_bytes() => {
                return ModuleResult::ok("content unchanged");
            }
            _ => {}
        }

        match conn.write_file(&dest, content.as_bytes(), mode_int) {
            Ok(_) => ModuleResult::changed("content copied"),
            Err(e) => ModuleResult::failed(&format!("failed to write: {}", e)),
        }
    } else if let Some(src) = args.get("src") {
        let content = match std::fs::read(src) {
            Ok(c) => c,
            Err(e) => return ModuleResult::failed(&format!("failed to read source: {}", e)),
        };

        match conn.read_file(&dest) {
            Ok(existing) if existing == content => {
                return ModuleResult::ok("file unchanged");
            }
            _ => {}
        }

        match conn.write_file(&dest, &content, mode_int) {
            Ok(_) => ModuleResult::changed("file copied"),
            Err(e) => ModuleResult::failed(&format!("failed to copy: {}", e)),
        }
    } else {
        ModuleResult::failed("either 'src' or 'content' required")
    }
}

fn run_file(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let path = match args.require("path") {
        Ok(p) => p.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get_or("state", "file");

    match state.as_str() {
        "directory" => {
            match conn.exec(&format!("test -d {}", path)) {
                Ok(r) if r.exit_code == 0 => ModuleResult::ok("directory exists"),
                _ => match conn.exec(&format!("mkdir -p {}", path)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("directory created"),
                    _ => ModuleResult::failed("failed to create directory"),
                },
            }
        }
        "absent" => {
            match conn.exec(&format!("test -e {}", path)) {
                Ok(r) if r.exit_code != 0 => ModuleResult::ok("already absent"),
                _ => match conn.exec(&format!("rm -rf {}", path)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("removed"),
                    _ => ModuleResult::failed("failed to remove"),
                },
            }
        }
        "touch" => {
            match conn.exec(&format!("touch {}", path)) {
                Ok(r) if r.exit_code == 0 => ModuleResult::changed("touched"),
                _ => ModuleResult::failed("failed to touch"),
            }
        }
        _ => ModuleResult::ok("file exists"),
    }
}

fn run_template(conn: &Connection, args: &ModuleArgs, vars: &HashMap<String, String>) -> ModuleResult {
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

    let template_content = match std::fs::read_to_string(&src) {
        Ok(c) => c,
        Err(e) => return ModuleResult::failed(&format!("failed to read template: {}", e)),
    };

    let rendered = template::render(&template_content, vars);

    match conn.read_file(&dest) {
        Ok(existing) if existing == rendered.as_bytes() => {
            return ModuleResult::ok("template unchanged");
        }
        _ => {}
    }

    match conn.write_file(&dest, rendered.as_bytes(), mode_int) {
        Ok(_) => ModuleResult::changed("template rendered"),
        Err(e) => ModuleResult::failed(&format!("failed to write: {}", e)),
    }
}

fn run_apt(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let name = match args.require("name") {
        Ok(n) => n.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get_or("state", "present");

    let is_installed = conn
        .exec(&format!("dpkg-query -W -f='${{Status}}' {} 2>/dev/null | grep -q 'ok installed'", name))
        .map(|r| r.exit_code == 0)
        .unwrap_or(false);

    match state.as_str() {
        "present" | "installed" => {
            if is_installed {
                ModuleResult::ok("already installed")
            } else {
                match conn.exec(&format!("DEBIAN_FRONTEND=noninteractive apt-get install -y -qq {}", name)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("installed"),
                    Ok(r) => ModuleResult::failed(&r.stderr),
                    Err(e) => ModuleResult::failed(&format!("{}", e)),
                }
            }
        }
        "absent" => {
            if !is_installed {
                ModuleResult::ok("already absent")
            } else {
                match conn.exec(&format!("DEBIAN_FRONTEND=noninteractive apt-get remove -y -qq {}", name)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("removed"),
                    Ok(r) => ModuleResult::failed(&r.stderr),
                    Err(e) => ModuleResult::failed(&format!("{}", e)),
                }
            }
        }
        _ => ModuleResult::failed(&format!("unknown state: {}", state)),
    }
}

fn run_service(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let name = match args.require("name") {
        Ok(n) => n.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let state = args.get("state");

    if let Some(st) = state {
        match st.as_str() {
            "started" => {
                match conn.exec(&format!("systemctl start {}", name)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("started"),
                    Ok(r) => ModuleResult::failed(&r.stderr),
                    Err(e) => ModuleResult::failed(&format!("{}", e)),
                }
            }
            "stopped" => {
                match conn.exec(&format!("systemctl stop {}", name)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("stopped"),
                    Ok(r) => ModuleResult::failed(&r.stderr),
                    Err(e) => ModuleResult::failed(&format!("{}", e)),
                }
            }
            "restarted" => {
                match conn.exec(&format!("systemctl restart {}", name)) {
                    Ok(r) if r.exit_code == 0 => ModuleResult::changed("restarted"),
                    Ok(r) => ModuleResult::failed(&r.stderr),
                    Err(e) => ModuleResult::failed(&format!("{}", e)),
                }
            }
            _ => ModuleResult::failed(&format!("unknown state: {}", st)),
        }
    } else {
        ModuleResult::ok("no state specified")
    }
}

fn run_lineinfile(conn: &Connection, args: &ModuleArgs) -> ModuleResult {
    let path = match args.require("path") {
        Ok(p) => p.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let line = match args.require("line") {
        Ok(l) => l.clone(),
        Err(e) => return ModuleResult::failed(&e),
    };

    let content = conn.read_file(&path).unwrap_or_default();
    let content_str = String::from_utf8_lossy(&content);

    if content_str.lines().any(|l| l == line) {
        return ModuleResult::ok("line present");
    }

    let new_content = format!("{}\n{}", content_str.trim_end(), line);

    match conn.write_file(&path, new_content.as_bytes(), 0o644) {
        Ok(_) => ModuleResult::changed("line added"),
        Err(e) => ModuleResult::failed(&format!("{}", e)),
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

    #[test]
    fn limit_single_host() {
        let mut inv = Inventory::default();
        inv.hosts.insert("host1".to_string(), crate::inventory::Host {
            name: "host1".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("host2".to_string(), crate::inventory::Host {
            name: "host2".to_string(),
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv).limit(Some("host1".to_string()));
        let hosts = exec.resolve_hosts("all");
        assert_eq!(hosts, vec!["host1".to_string()]);
    }

    #[test]
    fn limit_multiple_hosts() {
        let mut inv = Inventory::default();
        inv.hosts.insert("host1".to_string(), crate::inventory::Host {
            name: "host1".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("host2".to_string(), crate::inventory::Host {
            name: "host2".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("host3".to_string(), crate::inventory::Host {
            name: "host3".to_string(),
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv).limit(Some("host1,host2".to_string()));
        let mut hosts = exec.resolve_hosts("all");
        hosts.sort();
        assert_eq!(hosts, vec!["host1".to_string(), "host2".to_string()]);
    }

    #[test]
    fn limit_exclusion() {
        let mut inv = Inventory::default();
        inv.hosts.insert("host1".to_string(), crate::inventory::Host {
            name: "host1".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("host2".to_string(), crate::inventory::Host {
            name: "host2".to_string(),
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv).limit(Some("!host1".to_string()));
        let hosts = exec.resolve_hosts("all");
        assert_eq!(hosts, vec!["host2".to_string()]);
    }

    #[test]
    fn limit_with_group() {
        let mut inv = Inventory::default();
        inv.hosts.insert("web1".to_string(), crate::inventory::Host {
            name: "web1".to_string(),
            vars: HashMap::new(),
        });
        inv.hosts.insert("web2".to_string(), crate::inventory::Host {
            name: "web2".to_string(),
            vars: HashMap::new(),
        });
        inv.groups.insert("webservers".to_string(), crate::inventory::Group {
            name: "webservers".to_string(),
            hosts: vec!["web1".to_string(), "web2".to_string()],
            children: vec![],
            vars: HashMap::new(),
        });

        let exec = Executor::new(inv).limit(Some("webservers".to_string()));
        let mut hosts = exec.resolve_hosts("all");
        hosts.sort();
        assert_eq!(hosts, vec!["web1".to_string(), "web2".to_string()]);
    }
}
