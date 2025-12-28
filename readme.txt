WAND
====

A fast, Ansible-compatible automation tool written in Rust.

OVERVIEW
--------
Wand aims to be a drop-in replacement for Ansible, offering:
- Full YAML playbook compatibility
- Ansible module compatibility (built-in modules reimplemented in Rust)
- SSH-based remote execution
- Inventory file support (INI and YAML formats)
- Jinja2-style templating
- Parallel execution across hosts
- Significantly faster execution than Python-based Ansible

GOALS
-----
- Parse and execute Ansible playbooks without modification
- Support core modules: command, shell, copy, file, template, apt, yum, service, user, group
- Inventory parsing (static files, dynamic scripts)
- Variable precedence matching Ansible behavior
- Handlers and notifications
- Roles and includes
- Vault encryption compatibility
- Check mode (dry-run)
- Diff mode

NON-GOALS
---------
- Full plugin ecosystem compatibility (focus on core functionality first)
- Windows target support (Linux/Unix focus initially)
- Ansible Tower/AWX compatibility

ARCHITECTURE
------------
src/
  main.rs           - CLI entry point
  cli/              - Command-line argument parsing
  inventory/        - Inventory file parsing
  playbook/         - Playbook YAML parsing
  modules/          - Built-in module implementations
  executor/         - Task execution engine
  ssh/              - SSH connection handling
  template/         - Jinja2-compatible templating
  vars/             - Variable management and precedence
  vault/            - Ansible Vault compatibility

USAGE
-----
wand playbook.yml -i inventory.ini
wand playbook.yml -i inventory.yml --check
wand playbook.yml -i hosts --limit webservers
wand --version

LICENSE
-------
MIT
