//! monarchctl: command line and terminal UI for Monarch.

mod api;
mod cli;
mod commands;
mod config;
mod duration;
mod fmt;
mod resolve;
mod tui;
mod types;

use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser};

use cli::{Cli, Command, StatusArgs};
use commands::Ctx;

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn connect(cli: &Cli) -> Result<Ctx> {
    let resolved = config::resolve(
        config::Flags {
            url: cli.url.clone(),
            token_file: cli.token_file.clone(),
            insecure: cli.insecure,
        },
        config::Env::from_process(),
        config::load_files()?,
    )?;
    let token = resolved.token.fetch().await?;
    Ok(Ctx {
        client: api::Client::new(&resolved, token)?,
        json: cli.json,
        palette: fmt::Palette::detect(),
    })
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let command = match cli.command {
        Some(Command::Completions { shell }) => {
            let mut command = Cli::command();
            clap_complete::generate(shell, &mut command, "monarchctl", &mut std::io::stdout());
            return Ok(ExitCode::SUCCESS);
        }
        Some(ref command) => command,
        None if interactive && !cli.json => &Command::Tui,
        None => &Command::Status(StatusArgs { check: false }),
    };

    let ctx = connect(&cli).await?;
    match command {
        Command::Tui => {
            tui::run(ctx.client).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Status(args) => commands::status(&ctx, args.check).await,
        Command::Hosts => ok(commands::hosts(&ctx).await),
        Command::Host { host } => ok(commands::host(&ctx, host).await),
        Command::Services(args) => ok(commands::services(&ctx, args).await),
        Command::Events(args) => ok(commands::events(&ctx, args).await),
        Command::Ack { ids } => ok(commands::ack(&ctx, ids).await),
        Command::Action { host, args } => ok(commands::action(&ctx, host, args).await),
        Command::Mute { host, duration, .. } => {
            ok(commands::mute(&ctx, host, duration.as_deref()).await)
        }
        Command::Unmute { host } => ok(commands::unmute(&ctx, host).await),
        Command::Targets { command } => commands::targets(&ctx, command.as_ref()).await,
        Command::Completions { .. } => unreachable!("handled above"),
    }
}

fn ok(result: Result<()>) -> Result<ExitCode> {
    result.map(|()| ExitCode::SUCCESS)
}
