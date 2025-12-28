use anyhow::Result;
use clap::Parser;
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

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Playbook: {:?}", cli.playbook);
    println!("Inventory: {:?}", cli.inventory);

    if cli.check {
        println!("Check mode enabled");
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
