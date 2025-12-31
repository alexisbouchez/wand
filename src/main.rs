mod executor;
mod inventory;
mod modules;
mod playbook;
mod ssh;
mod template;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use executor::Executor;
use inventory::Inventory;
use ssh::Auth;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "wand")]
#[command(about = "Ansible-compatible automation tool")]
#[command(version)]
struct Cli {
    /// Playbook file to execute
    playbook: PathBuf,

    /// Inventory file or host list
    #[arg(short, long)]
    inventory: PathBuf,

    /// Run in check mode (dry-run)
    #[arg(short = 'C', long)]
    check: bool,

    /// Show diffs for changed files
    #[arg(short = 'D', long)]
    diff: bool,

    /// Limit to specific hosts
    #[arg(short, long)]
    limit: Option<String>,

    /// SSH private key path
    #[arg(long)]
    private_key: Option<PathBuf>,

    /// Remote user
    #[arg(short, long)]
    user: Option<String>,

    /// Number of parallel processes (default: 5)
    #[arg(short, long, default_value = "5")]
    forks: usize,

    /// Extra variables (key=value format)
    #[arg(short = 'e', long = "extra-vars")]
    extra_vars: Vec<String>,

    /// Only run tasks with these tags
    #[arg(short = 't', long)]
    tags: Vec<String>,

    /// Skip tasks with these tags
    #[arg(long)]
    skip_tags: Vec<String>,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load inventory
    let inventory_content = std::fs::read_to_string(&cli.inventory)
        .with_context(|| format!("failed to read inventory: {:?}", cli.inventory))?;
    let inventory = Inventory::from_ini(&inventory_content);

    // Load playbook
    let playbook_content = std::fs::read_to_string(&cli.playbook)
        .with_context(|| format!("failed to read playbook: {:?}", cli.playbook))?;
    let plays = playbook::parse_playbook(&playbook_content)
        .with_context(|| "failed to parse playbook")?;

    // Setup auth
    let auth = if let Some(key_path) = &cli.private_key {
        Auth::key(key_path.to_str().unwrap_or(""))
    } else {
        Auth::agent()
    };

    // Parse extra vars
    let mut extra_vars = std::collections::HashMap::new();
    for var in &cli.extra_vars {
        if let Some((key, value)) = var.split_once('=') {
            extra_vars.insert(key.to_string(), value.to_string());
        }
    }

    // Create executor
    let executor = Executor::new(inventory)
        .with_vars(extra_vars)
        .check_mode(cli.check)
        .diff_mode(cli.diff)
        .forks(cli.forks)
        .tags(cli.tags)
        .skip_tags(cli.skip_tags)
        .limit(cli.limit);

    // Print header
    println!();
    if cli.check {
        println!("{}", "CHECK MODE - no changes will be made".yellow().bold());
        println!();
    }

    let mut total_ok = 0;
    let mut total_changed = 0;
    let mut total_failed = 0;
    let mut total_skipped = 0;

    // Run plays
    for play in &plays {
        println!(
            "{} [{}] {}",
            "PLAY".bold(),
            play.hosts.cyan(),
            "*".repeat(50)
        );
        println!();

        let results = executor.run_play(play, &auth);

        for result in &results {
            for task_result in &result.task_results {
                let is_skipped = task_result.result.msg.contains("skipped");
                let (status, color_status) = if task_result.result.failed {
                    ("FAILED", "FAILED".red().bold())
                } else if is_skipped {
                    ("SKIPPED", "SKIPPED".blue().bold())
                } else if task_result.result.changed {
                    ("CHANGED", "CHANGED".yellow().bold())
                } else {
                    ("OK", "OK".green().bold())
                };

                println!(
                    "{}: [{}] => {}",
                    color_status,
                    result.host.cyan(),
                    task_result.task_name
                );

                if cli.diff && task_result.result.diff.is_some() {
                    println!("--- before");
                    println!("+++ after");
                    for line in task_result.result.diff.as_ref().unwrap().lines() {
                        if line.starts_with('-') {
                            println!("{}", line.red());
                        } else if line.starts_with('+') {
                            println!("{}", line.green());
                        } else if line.starts_with(' ') {
                            println!("{}", line.dimmed());
                        } else {
                            println!("{}", line);
                        }
                    }
                }

                if cli.verbose > 0 && !task_result.result.stdout.is_empty() {
                    println!("  {}: {}", "stdout".dimmed(), task_result.result.stdout.trim());
                }

                if status == "FAILED" || cli.verbose > 0 {
                    if !task_result.result.stderr.is_empty() {
                        println!("  {}: {}", "stderr".red(), task_result.result.stderr.trim());
                    }
                    if !task_result.result.msg.is_empty() {
                        println!("  {}: {}", "msg".dimmed(), task_result.result.msg);
                    }
                }
            }

            total_ok += result.ok;
            total_changed += result.changed;
            total_failed += result.failed;
            total_skipped += result.skipped;
        }

        println!();
    }

    // Print recap
    println!("{} {}", "PLAY RECAP".bold(), "*".repeat(50));
    print!("{}={} ", "ok".green(), total_ok);
    print!("{}={} ", "changed".yellow(), total_changed);
    print!("{}={} ", "skipped".blue(), total_skipped);
    println!("{}={}", "failed".red(), total_failed);

    if total_failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
