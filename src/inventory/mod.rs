use std::collections::HashMap;

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
                let group_name = &line[1..line.len() - 1];
                current_group = Some(group_name.to_string());
                inventory.groups.entry(group_name.to_string()).or_insert(Group {
                    name: group_name.to_string(),
                    ..Default::default()
                });
            } else if let Some(ref group) = current_group {
                let host_name = line.split_whitespace().next().unwrap_or(line);

                inventory.hosts.entry(host_name.to_string()).or_insert(Host {
                    name: host_name.to_string(),
                    vars: HashMap::new(),
                });

                if let Some(g) = inventory.groups.get_mut(group) {
                    if !g.hosts.contains(&host_name.to_string()) {
                        g.hosts.push(host_name.to_string());
                    }
                }
            } else {
                let host_name = line.split_whitespace().next().unwrap_or(line);
                inventory.hosts.entry(host_name.to_string()).or_insert(Host {
                    name: host_name.to_string(),
                    vars: HashMap::new(),
                });

                inventory.groups.entry("ungrouped".to_string()).or_insert(Group {
                    name: "ungrouped".to_string(),
                    ..Default::default()
                });

                if let Some(g) = inventory.groups.get_mut("ungrouped") {
                    if !g.hosts.contains(&host_name.to_string()) {
                        g.hosts.push(host_name.to_string());
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
}
