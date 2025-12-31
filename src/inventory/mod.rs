use std::collections::HashMap;

fn expand_host_pattern(pattern: &str) -> Vec<String> {
    if let Some(start) = pattern.find('[') {
        if let Some(end) = pattern.find(']') {
            let prefix = &pattern[..start];
            let suffix = &pattern[end + 1..];
            let range_spec = &pattern[start + 1..end];

            if let Some((from, to)) = range_spec.split_once(':') {
                if let (Ok(from_num), Ok(to_num)) = (from.parse::<u32>(), to.parse::<u32>()) {
                    let width = from.len();
                    return (from_num..=to_num)
                        .map(|n| format!("{}{:0width$}{}", prefix, n, suffix, width = width))
                        .collect();
                } else if from.len() == 1 && to.len() == 1 {
                    let from_char = from.chars().next().unwrap();
                    let to_char = to.chars().next().unwrap();
                    return (from_char..=to_char)
                        .map(|c| format!("{}{}{}", prefix, c, suffix))
                        .collect();
                }
            }
        }
    }
    vec![pattern.to_string()]
}

fn parse_host_line(line: &str) -> (Vec<String>, HashMap<String, String>) {
    let mut parts = line.split_whitespace();
    let host_pattern = parts.next().unwrap_or("").to_string();
    let mut vars = HashMap::new();

    for part in parts {
        if let Some((key, value)) = part.split_once('=') {
            vars.insert(key.to_string(), value.to_string());
        }
    }

    let hosts = expand_host_pattern(&host_pattern);
    (hosts, vars)
}

#[derive(Debug, Default, PartialEq)]
pub struct Inventory {
    pub hosts: HashMap<String, Host>,
    pub groups: HashMap<String, Group>,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Host {
    pub name: String,
    pub vars: HashMap<String, String>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Group {
    pub name: String,
    pub hosts: Vec<String>,
    pub children: Vec<String>,
    pub vars: HashMap<String, String>,
}

impl Inventory {
    pub fn from_ini(content: &str) -> Self {
        let mut inventory = Inventory::default();
        let mut current_group: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let group_spec = &line[1..line.len() - 1];

                if let Some((group_name, suffix)) = group_spec.split_once(':') {
                    current_group = Some(format!("{}:{}", group_name, suffix));
                    inventory.groups.entry(group_name.to_string()).or_insert(Group {
                        name: group_name.to_string(),
                        ..Default::default()
                    });
                } else {
                    current_group = Some(group_spec.to_string());
                    inventory.groups.entry(group_spec.to_string()).or_insert(Group {
                        name: group_spec.to_string(),
                        ..Default::default()
                    });
                }
            } else if let Some(ref group) = current_group {
                if group.ends_with(":vars") {
                    let group_name = group.strip_suffix(":vars").unwrap();
                    if let Some((key, value)) = line.split_once('=') {
                        if let Some(g) = inventory.groups.get_mut(group_name) {
                            g.vars.insert(key.trim().to_string(), value.trim().to_string());
                        }
                    }
                } else if group.ends_with(":children") {
                    let group_name = group.strip_suffix(":children").unwrap();
                    if let Some(g) = inventory.groups.get_mut(group_name) {
                        g.children.push(line.to_string());
                    }
                } else {
                    let (host_names, vars) = parse_host_line(line);

                    for host_name in host_names {
                        inventory.hosts.entry(host_name.clone()).or_insert(Host {
                            name: host_name.clone(),
                            vars: vars.clone(),
                        });

                        if let Some(host) = inventory.hosts.get_mut(&host_name) {
                            host.vars.extend(vars.clone());
                        }

                        if let Some(g) = inventory.groups.get_mut(group) {
                            if !g.hosts.contains(&host_name) {
                                g.hosts.push(host_name);
                            }
                        }
                    }
                }
            } else {
                let (host_names, vars) = parse_host_line(line);

                for host_name in host_names {
                    inventory.hosts.entry(host_name.clone()).or_insert(Host {
                        name: host_name.clone(),
                        vars: vars.clone(),
                    });

                    if let Some(host) = inventory.hosts.get_mut(&host_name) {
                        host.vars.extend(vars.clone());
                    }

                    inventory.groups.entry("ungrouped".to_string()).or_insert(Group {
                        name: "ungrouped".to_string(),
                        ..Default::default()
                    });

                    if let Some(g) = inventory.groups.get_mut("ungrouped") {
                        if !g.hosts.contains(&host_name) {
                            g.hosts.push(host_name);
                        }
                    }
                }
            }
        }

        inventory
    }

    pub fn get_all_hosts(&self) -> Vec<String> {
        self.hosts.keys().cloned().collect()
    }

    pub fn get_group_hosts(&self, group_name: &str) -> Vec<String> {
        let mut hosts = Vec::new();
        let mut visited = std::collections::HashSet::new();

        if self.groups.contains_key(group_name) {
            self.collect_hosts_recursive(group_name, &mut hosts, &mut visited);
        }

        hosts
    }

    fn collect_hosts_recursive(
        &self,
        group_name: &str,
        hosts: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(group_name) {
            return;
        }
        visited.insert(group_name.to_string());

        if let Some(group) = self.groups.get(group_name) {
            for host in &group.hosts {
                if !hosts.contains(host) {
                    hosts.push(host.clone());
                }
            }

            for child in &group.children {
                self.collect_hosts_recursive(child, hosts, visited);
            }
        }
    }

    pub fn get_host_groups(&self, host_name: &str) -> Vec<String> {
        let mut groups = Vec::new();

        for (group_name, group) in &self.groups {
            if group.hosts.contains(&host_name.to_string())
                || self.get_group_hosts(group_name).contains(&host_name.to_string())
            {
                groups.push(group_name.clone());
            }
        }

        groups
    }

    pub fn get_host_vars(&self, host_name: &str) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        let groups = self.get_host_groups(host_name);

        for group_name in &groups {
            if let Some(group) = self.groups.get(group_name) {
                for (k, v) in &group.vars {
                    if !vars.contains_key(k) {
                        vars.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        if let Some(host) = self.hosts.get(host_name) {
            for (k, v) in &host.vars {
                vars.insert(k.clone(), v.clone());
            }
        }

        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_host() {
        let inv = Inventory::from_ini("192.168.1.1");
        assert!(inv.hosts.contains_key("192.168.1.1"));
    }

    #[test]
    fn parse_multiple_hosts() {
        let inv = Inventory::from_ini("192.168.1.1\n192.168.1.2\nweb.example.com");
        assert_eq!(inv.hosts.len(), 3);
        assert!(inv.hosts.contains_key("192.168.1.1"));
        assert!(inv.hosts.contains_key("192.168.1.2"));
        assert!(inv.hosts.contains_key("web.example.com"));
    }

    #[test]
    fn parse_group_with_hosts() {
        let inv = Inventory::from_ini("[webservers]\nweb1\nweb2");
        assert_eq!(inv.groups.len(), 1);
        assert!(inv.groups.contains_key("webservers"));
        let group = inv.groups.get("webservers").unwrap();
        assert_eq!(group.hosts.len(), 2);
        assert!(group.hosts.contains(&"web1".to_string()));
        assert!(group.hosts.contains(&"web2".to_string()));
    }

    #[test]
    fn parse_multiple_groups() {
        let inv = Inventory::from_ini("[webservers]\nweb1\n\n[dbservers]\ndb1");
        assert_eq!(inv.groups.len(), 2);
        assert!(inv.groups.contains_key("webservers"));
        assert!(inv.groups.contains_key("dbservers"));
    }

    #[test]
    fn skip_comments() {
        let inv = Inventory::from_ini("# comment\nhost1\n; another comment\nhost2");
        assert_eq!(inv.hosts.len(), 2);
    }

    #[test]
    fn ungrouped_hosts() {
        let inv = Inventory::from_ini("host1\nhost2");
        assert!(inv.groups.contains_key("ungrouped"));
        let group = inv.groups.get("ungrouped").unwrap();
        assert_eq!(group.hosts.len(), 2);
    }

    #[test]
    fn parse_host_variables() {
        let inv = Inventory::from_ini("web1 ansible_host=192.168.1.1 ansible_user=admin");
        let host = inv.hosts.get("web1").unwrap();
        assert_eq!(host.vars.get("ansible_host").unwrap(), "192.168.1.1");
        assert_eq!(host.vars.get("ansible_user").unwrap(), "admin");
    }

    #[test]
    fn parse_host_variables_in_group() {
        let inv = Inventory::from_ini("[web]\nweb1 ansible_port=2222");
        let host = inv.hosts.get("web1").unwrap();
        assert_eq!(host.vars.get("ansible_port").unwrap(), "2222");
    }

    #[test]
    fn parse_group_variables() {
        let inv = Inventory::from_ini("[web]\nweb1\n\n[web:vars]\nhttp_port=80\nmax_clients=200");
        let group = inv.groups.get("web").unwrap();
        assert_eq!(group.vars.get("http_port").unwrap(), "80");
        assert_eq!(group.vars.get("max_clients").unwrap(), "200");
    }

    #[test]
    fn parse_group_children() {
        let inv = Inventory::from_ini("[web]\nweb1\n\n[db]\ndb1\n\n[all:children]\nweb\ndb");
        let group = inv.groups.get("all").unwrap();
        assert!(group.children.contains(&"web".to_string()));
        assert!(group.children.contains(&"db".to_string()));
    }

    #[test]
    fn parse_host_range_numeric() {
        let inv = Inventory::from_ini("web[1:3].example.com");
        assert!(inv.hosts.contains_key("web1.example.com"));
        assert!(inv.hosts.contains_key("web2.example.com"));
        assert!(inv.hosts.contains_key("web3.example.com"));
        assert_eq!(inv.hosts.len(), 3);
    }

    #[test]
    fn parse_host_range_alpha() {
        let inv = Inventory::from_ini("db[a:c].local");
        assert!(inv.hosts.contains_key("dba.local"));
        assert!(inv.hosts.contains_key("dbb.local"));
        assert!(inv.hosts.contains_key("dbc.local"));
        assert_eq!(inv.hosts.len(), 3);
    }

    #[test]
    fn parse_host_range_padded() {
        let inv = Inventory::from_ini("web[01:03].example.com");
        assert!(inv.hosts.contains_key("web01.example.com"));
        assert!(inv.hosts.contains_key("web02.example.com"));
        assert!(inv.hosts.contains_key("web03.example.com"));
    }

    #[test]
    fn get_group_hosts_simple() {
        let inv = Inventory::from_ini("[web]\nweb1\nweb2");
        let hosts = inv.get_group_hosts("web");
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains(&"web1".to_string()));
        assert!(hosts.contains(&"web2".to_string()));
    }

    #[test]
    fn get_group_hosts_with_children() {
        let inv = Inventory::from_ini("[web]\nweb1\n[db]\ndb1\n[all:children]\nweb\ndb");
        let hosts = inv.get_group_hosts("all");
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains(&"web1".to_string()));
        assert!(hosts.contains(&"db1".to_string()));
    }

    #[test]
    fn get_group_hosts_nested_children() {
        let inv = Inventory::from_ini("[web]\nweb1\n[production:children]\nweb\n[all:children]\nproduction");
        let hosts = inv.get_group_hosts("all");
        assert_eq!(hosts.len(), 1);
        assert!(hosts.contains(&"web1".to_string()));
    }

    #[test]
    fn get_host_groups_single() {
        let inv = Inventory::from_ini("[web]\nweb1");
        let groups = inv.get_host_groups("web1");
        assert_eq!(groups, vec!["web"]);
    }

    #[test]
    fn get_host_groups_multiple() {
        let inv = Inventory::from_ini("[web]\nweb1\n[servers:children]\nweb\n[all:children]\nservers");
        let mut groups = inv.get_host_groups("web1");
        groups.sort();
        assert_eq!(groups, vec!["all", "servers", "web"]);
    }

    #[test]
    fn get_host_vars() {
        let inv = Inventory::from_ini(
            "[web]\nweb1 ansible_host=192.168.1.1\n[web:vars]\nhttp_port=80"
        );
        let vars = inv.get_host_vars("web1");
        assert_eq!(vars.get("ansible_host").unwrap(), "192.168.1.1");
        assert_eq!(vars.get("http_port").unwrap(), "80");
    }

    #[test]
    fn get_host_vars_precedence() {
        let inv = Inventory::from_ini(
            "[web]\nweb1 ansible_host=192.168.1.1\n[web:vars]\nansible_host=192.168.1.2"
        );
        let vars = inv.get_host_vars("web1");
        assert_eq!(vars.get("ansible_host").unwrap(), "192.168.1.1");
    }

    #[test]
    fn get_all_hosts() {
        let inv = Inventory::from_ini("[web]\nweb1\nweb2\n[db]\ndb1");
        let mut hosts = inv.get_all_hosts();
        hosts.sort();
        assert_eq!(hosts, vec!["db1", "web1", "web2"]);
    }

    #[test]
    fn get_all_hosts_empty() {
        let inv = Inventory::from_ini("");
        let hosts = inv.get_all_hosts();
        assert!(hosts.is_empty());
    }
}
