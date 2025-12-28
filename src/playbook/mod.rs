use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Play {
    pub hosts: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub handlers: Vec<Task>,
    #[serde(default)]
    pub vars: HashMap<String, serde_yaml::Value>,
    #[serde(default, rename = "become")]
    pub become_: bool,
    #[serde(default)]
    pub become_user: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Task {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub register: Option<String>,
    #[serde(default)]
    pub notify: Option<Vec<String>>,
    #[serde(default)]
    pub with_items: Option<Vec<serde_yaml::Value>>,
    #[serde(default, rename = "loop")]
    pub loop_: Option<Vec<serde_yaml::Value>>,
    #[serde(flatten)]
    pub module: HashMap<String, serde_yaml::Value>,
}

pub fn parse_playbook(content: &str) -> Result<Vec<Play>, serde_yaml::Error> {
    serde_yaml::from_str(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_playbook() {
        let yaml = r#"
- hosts: webservers
  tasks:
    - command: echo hello
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].hosts, "webservers");
        assert_eq!(plays[0].tasks.len(), 1);
    }

    #[test]
    fn parse_task_with_name() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Say hello
      command: echo hello
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].tasks[0].name, Some("Say hello".to_string()));
    }

    #[test]
    fn parse_task_with_module_args() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Install nginx
      apt:
        name: nginx
        state: present
"#;
        let plays = parse_playbook(yaml).unwrap();
        let task = &plays[0].tasks[0];
        assert!(task.module.contains_key("apt"));
    }

    #[test]
    fn parse_task_with_when() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Only on debian
      command: echo debian
      when: ansible_os_family == "Debian"
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(
            plays[0].tasks[0].when,
            Some("ansible_os_family == \"Debian\"".to_string())
        );
    }

    #[test]
    fn parse_task_with_register() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Get date
      command: date
      register: date_output
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].tasks[0].register, Some("date_output".to_string()));
    }

    #[test]
    fn parse_handlers() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Copy config
      copy:
        src: nginx.conf
        dest: /etc/nginx/nginx.conf
      notify:
        - restart nginx
  handlers:
    - name: restart nginx
      service:
        name: nginx
        state: restarted
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].handlers.len(), 1);
        assert_eq!(
            plays[0].handlers[0].name,
            Some("restart nginx".to_string())
        );
    }

    #[test]
    fn parse_become() {
        let yaml = r#"
- hosts: all
  become: true
  become_user: root
  tasks:
    - command: whoami
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert!(plays[0].become_);
        assert_eq!(plays[0].become_user, Some("root".to_string()));
    }

    #[test]
    fn parse_vars() {
        let yaml = r#"
- hosts: all
  vars:
    http_port: 80
    server_name: example.com
  tasks:
    - command: echo done
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert!(plays[0].vars.contains_key("http_port"));
        assert!(plays[0].vars.contains_key("server_name"));
    }
}
