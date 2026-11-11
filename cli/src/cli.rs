//! Command line definition.

use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

const AFTER_HELP: &str = "\
Configuration (highest precedence first):
  flags            --url, --token-file
  environment      MONARCH_URL, MONARCH_TOKEN, MONARCH_TOKEN_FILE
  user config      $XDG_CONFIG_HOME/monarchctl/config.toml
  system config    /etc/monarchctl/config.toml

Config keys: url, token_file, token_command, tls_skip_verify, timeout.
Without a subcommand, monarchctl opens the TUI on a terminal and prints `status` otherwise.";

#[derive(Debug, Parser)]
#[command(
    name = "monarchctl",
    version,
    about = "Administer Monarch hosts and see their status",
    after_help = AFTER_HELP,
    propagate_version = true
)]
pub struct Cli {
    /// Base URL of the Monarch server (e.g. https://monarch.example.com)
    #[arg(long, global = true, value_name = "URL")]
    pub url: Option<String>,

    /// File holding the API token (an `mnr_...` token)
    #[arg(long, global = true, value_name = "FILE")]
    pub token_file: Option<PathBuf>,

    /// Do not verify the server's TLS certificate
    #[arg(short = 'k', long, global = true)]
    pub insecure: bool,

    /// Print the raw JSON of the API instead of formatted text
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fleet overview: hosts, failing services and recent events
    Status(StatusArgs),
    /// List hosts
    Hosts,
    /// Show one host with its services
    Host {
        /// Host id, hostname or display name
        host: String,
    },
    /// List services of all hosts
    Services(ServicesArgs),
    /// List events
    Events(EventsArgs),
    /// Acknowledge events
    Ack {
        /// Event ids (see `monarchctl events`)
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// Start, stop, restart or (un)monitor services of a host
    #[command(override_usage = "monarchctl action <HOST> <SERVICE>... <ACTION>")]
    Action {
        /// Host id, hostname or display name
        host: String,
        /// One or more service names followed by the action
        /// (start, stop, restart, monitor, unmonitor or remonitor)
        #[arg(required = true, num_args = 2.., value_name = "SERVICE... ACTION")]
        args: Vec<String>,
    },
    /// Silence the notifications of a host
    #[command(group(ArgGroup::new("when").required(true).args(["duration", "forever"])))]
    Mute {
        /// Host id, hostname or display name
        host: String,
        /// Mute for a while, e.g. 90m, 2h or 1d12h
        #[arg(long = "for", value_name = "DURATION")]
        duration: Option<String>,
        /// Mute until it is unmuted manually
        #[arg(long)]
        forever: bool,
    },
    /// Resume the notifications of a host
    Unmute {
        /// Host id, hostname or display name
        host: String,
    },
    /// List pull-mode targets, or poll one
    Targets {
        #[command(subcommand)]
        command: Option<TargetsCommand>,
    },
    /// Print a shell completion script
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Open the terminal UI
    Tui,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Exit with status 1 when a host is offline or degraded or a service is failing
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StateFilter {
    Failed,
    Ok,
    Unmonitored,
    Pending,
    Init,
    Stale,
}

impl StateFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Ok => "ok",
            Self::Unmonitored => "unmonitored",
            Self::Pending => "pending",
            Self::Init => "init",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Args)]
pub struct ServicesArgs {
    /// Only services in this state
    #[arg(long, value_enum)]
    pub state: Option<StateFilter>,
    /// Only services of this host (id, hostname or display name)
    #[arg(long)]
    pub host: Option<String>,
    /// Only service or host names containing this text
    #[arg(short, long, value_name = "TEXT")]
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct EventsArgs {
    /// Only events that nobody acknowledged yet
    #[arg(long)]
    pub unacked: bool,
    /// Only events of this host (id, hostname or display name)
    #[arg(long)]
    pub host: Option<String>,
    /// How many events to show
    #[arg(long, short = 'n', default_value_t = 50)]
    pub limit: usize,
    /// Keep running and print new events as they arrive
    #[arg(long, short = 'f')]
    pub follow: bool,
}

#[derive(Debug, Subcommand)]
pub enum TargetsCommand {
    /// Poll a target now
    Poll {
        /// Target id or name
        target: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn mute_needs_a_duration_or_forever() {
        assert!(Cli::try_parse_from(["monarchctl", "mute", "web"]).is_err());
        assert!(Cli::try_parse_from(["monarchctl", "mute", "web", "--for", "2h"]).is_ok());
        assert!(Cli::try_parse_from(["monarchctl", "mute", "web", "--forever"]).is_ok());
        assert!(
            Cli::try_parse_from(["monarchctl", "mute", "web", "--forever", "--for", "1h"]).is_err()
        );
    }

    #[test]
    fn action_takes_services_then_the_action() {
        let cli = Cli::try_parse_from(["monarchctl", "action", "web", "nginx", "sshd", "restart"])
            .unwrap();
        let Some(Command::Action { host, args }) = cli.command else {
            panic!("not an action")
        };
        assert_eq!(host, "web");
        assert_eq!(args, ["nginx", "sshd", "restart"]);
        assert!(Cli::try_parse_from(["monarchctl", "action", "web", "restart"]).is_err());
    }

    #[test]
    fn global_flags_work_after_the_subcommand() {
        let cli =
            Cli::try_parse_from(["monarchctl", "hosts", "--json", "--url", "http://m"]).unwrap();
        assert!(cli.json);
        assert_eq!(cli.url.as_deref(), Some("http://m"));
    }
}
