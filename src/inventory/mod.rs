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
}
