use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let dry_run = std::env::args().any(|arg| arg == "--dry-run");
    let dev_dry_run = std::env::args().any(|arg| arg == "--dev-dry-run");
    let commands = [
        vec!["labwc"],
        vec!["seatshell-user-agent"],
        vec!["seatshell-shell"],
    ];

    if dry_run {
        for command in commands {
            println!("{}", command.join(" "));
        }
        return Ok(());
    }

    if dev_dry_run {
        println!("labwc");
        println!("cargo run -p seatshell-user-agent -- --dry-run");
        println!("cargo run -p seatshell-shell");
        return Ok(());
    }

    let mut labwc = spawn(&commands[0]).context("failed to start labwc")?;
    let mut user_agent = spawn(&commands[1]).context("failed to start seatshell-user-agent")?;
    let mut shell = spawn(&commands[2]).context("failed to start seatshell-shell")?;

    tokio::select! {
        status = labwc.wait() => tracing::info!(?status, "labwc exited"),
        status = user_agent.wait() => tracing::info!(?status, "user agent exited"),
        status = shell.wait() => tracing::info!(?status, "shell exited"),
        _ = tokio::signal::ctrl_c() => tracing::info!("received shutdown signal"),
    }

    Ok(())
}

fn spawn(command: &[&str]) -> Result<tokio::process::Child> {
    let (program, args) = command
        .split_first()
        .context("cannot spawn an empty command")?;

    tracing::info!(program, args = ?args, "spawning process");
    Ok(Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .spawn()?)
}
