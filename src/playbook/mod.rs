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
    #[serde(default, rename = "vars_files")]
    pub vars_files: Vec<String>,
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
    #[serde(default, deserialize_with = "deserialize_tags")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub with_items: Option<Vec<serde_yaml::Value>>,
    #[serde(default, rename = "loop")]
    pub loop_: Option<Vec<serde_yaml::Value>>,
    #[serde(flatten)]
    pub module: HashMap<String, serde_yaml::Value>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or list of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut tags = Vec::new();
            while let Some(tag) = seq.next_element::<String>()? {
                tags.push(tag);
            }
            Ok(tags)
        }

        fn visit_none<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }

        fn visit_unit<E>(self) -> Result<Vec<String>, E>
        where
            E: de::Error,
        {
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(TagsVisitor)
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

    #[test]
    fn parse_tags_as_list() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Install packages
      command: apt install nginx
      tags:
        - install
        - nginx
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].tasks[0].tags, vec!["install", "nginx"]);
    }

    #[test]
    fn parse_tags_as_string() {
        let yaml = r#"
- hosts: all
  tasks:
    - name: Install packages
      command: apt install nginx
      tags: install
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].tasks[0].tags, vec!["install"]);
    }

    #[test]
    fn parse_no_tags() {
        let yaml = r#"
- hosts: all
  tasks:
    - command: echo hello
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert!(plays[0].tasks[0].tags.is_empty());
    }

    #[test]
    fn parse_vars_files() {
        let yaml = r#"
- hosts: all
  vars_files:
    - vars/common.yml
    - vars/production.yml
  tasks:
    - command: echo hello
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert_eq!(plays[0].vars_files.len(), 2);
        assert_eq!(plays[0].vars_files[0], "vars/common.yml");
        assert_eq!(plays[0].vars_files[1], "vars/production.yml");
    }

    #[test]
    fn parse_vars_files_empty() {
        let yaml = r#"
- hosts: all
  tasks:
    - command: echo hello
"#;
        let plays = parse_playbook(yaml).unwrap();
        assert!(plays[0].vars_files.is_empty());
    }
}
