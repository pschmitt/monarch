# monarchctl

`monarchctl` is a command line client and terminal UI for Monarch. It talks to
the [HTTP API](API.md) with an API token, so it works from any machine that can
reach the server. It is a separate package from the server.

## Install

```sh
nix run github:pschmitt/monarch#monarchctl -- status
nix profile install github:pschmitt/monarch#monarchctl
```

Or build it with Cargo from the `cli/` directory (it is its own crate with its
own `Cargo.lock`, not part of the server's package):

```sh
cargo install --path cli
```

On NixOS use the [module](#nixos-module). Shell completions for bash, zsh and
fish are installed by the Nix package; elsewhere run
`monarchctl completions <shell>`.

## Get a token

Create a token in the web UI under your profile, or with the API from a browser
session (`POST /api/tokens`). A token has the role of its owner: `viewer`
tokens can read, `operator` tokens can also run service actions, mute hosts and
acknowledge events. Store it in a file only you can read:

```sh
install -m 600 /dev/null ~/.config/monarchctl/token
$EDITOR ~/.config/monarchctl/token      # paste the mnr_... token
```

## Configuration

Settings are merged from, highest precedence first:

1. flags: `--url`, `--token-file`, `-k`/`--insecure`
2. environment: `MONARCH_URL`, `MONARCH_TOKEN`, `MONARCH_TOKEN_FILE`
3. `$XDG_CONFIG_HOME/monarchctl/config.toml` (`~/.config/...` by default)
4. `/etc/monarchctl/config.toml`

```toml
url = "https://monarch.example.com"
token_file = "/home/me/.config/monarchctl/token"   # recommended
# token_command = "rbw get monarch-token"          # first line of stdout, run with sh -c
# token = "mnr_..."                                # works, but prefer token_file
tls_skip_verify = false
timeout = 15                                       # seconds
```

| Key | Meaning |
| --- | --- |
| `url` | Base URL of the server (a sub-path behind a reverse proxy is fine) |
| `token_file` | File with the API token; the first line is used |
| `token_command` | Shell command printing the token on its first line |
| `token` | The token itself. Prefer `token_file`: a literal token lives in a config file |
| `tls_skip_verify` | Do not verify the server certificate |
| `timeout` | Request timeout in seconds (default 15) |

Within one source the token comes from `token_file`, then `token_command`, then
`token`. A user config that sets any token key replaces the system config's
token source as a whole. `MONARCH_TOKEN` beats `MONARCH_TOKEN_FILE`. The token
is never printed, not even in errors.

## Commands

Every command accepts `--json` to print the API's raw JSON instead of text.
Colors follow `NO_COLOR` and are off when stdout is not a terminal. Errors go to
stderr and give a non-zero exit status.

Hosts are given by id, hostname or display name, case-insensitive. A short name
(`web-1` for `web-1.example.com`) or a unique prefix works too; an ambiguous
name is an error that lists the candidates.

| Command | |
| --- | --- |
| `monarchctl status [--check]` | Hosts, services, what needs attention and recent events. `--check` exits 1 when a host is offline or degraded or a service fails |
| `monarchctl hosts` | One line per host |
| `monarchctl host <host>` | A host's details and its services |
| `monarchctl services [--state failed\|ok\|unmonitored\|pending\|init\|stale] [--host H] [-q TEXT]` | Services of all hosts, worst first |
| `monarchctl events [--unacked] [--host H] [-n N] [-f]` | Events, oldest first. `-f` keeps running and prints new events from the live stream |
| `monarchctl ack <event-id>...` | Acknowledge events |
| `monarchctl action <host> <service>... <action>` | `start`, `stop`, `restart`, `monitor`, `unmonitor` or `remonitor`. Several services use the bulk endpoint |
| `monarchctl mute <host> --for 2h` / `--forever` | Silence a host's notifications. `--for` takes `90m`, `2h`, `1d12h`, ... (admin) |
| `monarchctl unmute <host>` | Resume notifications (admin) |
| `monarchctl targets` | List pull-mode targets |
| `monarchctl targets poll <target>` | Poll a target now by id or name; exits 1 if the poll failed |
| `monarchctl completions <shell>` | Print a completion script |
| `monarchctl tui` | Open the terminal UI |

Without a subcommand, `monarchctl` opens the TUI on a terminal and prints
`status` otherwise (a pipe, a script, `--json`).

```sh
monarchctl status --check || notify-send "Monarch: something is wrong"
monarchctl action web-1 nginx sshd restart
monarchctl events --unacked --json | jq -r '.events[].message'
monarchctl mute db-1 --for 2h
```

## Terminal UI

Four views: Overview, Hosts, Services and Events. The header shows the host
counts and the connection state: `live` means the event stream is connected,
`reconnecting` that it dropped and is being re-opened, `offline` that the API
cannot be reached. The lists update from the stream and are refreshed in full
every 30 seconds as a fallback. Requests never block the UI.

| Key | |
| --- | --- |
| `Tab`, `Shift-Tab`, `1`-`4`, `h`/`l`, arrows | Switch view |
| `j`/`k`, up/down, `g`/`G`, `Ctrl-d`/`Ctrl-u`, PageUp/PageDown | Move in a list |
| `Enter` | Open the host (from Hosts, Services or Events) |
| `Esc`, `q` | Back from a host page; clear the search; `q` quits |
| `/` | Search the current list (`Enter` keeps it, `Esc` clears it) |
| `f` | Cycle the state filter (Hosts, Services) |
| `u` | Only unacknowledged events (Events) |
| `a` / `A` | Acknowledge the event / all listed unacknowledged events (Events) |
| `r` `s` `S` | Restart, start, stop the selected service |
| `m` | Monitor or unmonitor the selected service, depending on its state |
| `R` | Remonitor the selected service |
| `Ctrl-r`, `F5` | Refresh now |
| `?` | Help |
| `Ctrl-c` | Quit |

Service actions work in the Services view and on a host page. They ask for
confirmation (`y` or `Enter` confirms) and the result shows as a message in the
footer. With a read-only token the header says `read-only` and actions are
refused up front; a `403` from the server is reported the same way.

## NixOS module

```nix
{
  inputs.monarch.url = "github:pschmitt/monarch";

  outputs = { nixpkgs, monarch, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        monarch.nixosModules.monarchctl
        {
          programs.monarchctl = {
            enable = true;
            settings.url = "https://monarch.example.com";
            # not copied to the store: only the path goes into the config file
            tokenFile = "/run/secrets/monarch-token";
          };
        }
      ];
    };
  };
}
```

The module installs the package and writes `/etc/monarchctl/config.toml`:

| Option | |
| --- | --- |
| `programs.monarchctl.enable` | Install `monarchctl` and write the system config |
| `programs.monarchctl.package` | The package, `pkgs.monarchctl` by default (the module adds the flake's overlay) |
| `programs.monarchctl.settings` | Free-form TOML. Documented keys: `url`, `tls_skip_verify`, `timeout` |
| `programs.monarchctl.tokenFile` | `null` or a path to the token file, written as `token_file`. Use a string such as `"/run/secrets/..."`; the file must be readable by the users of `monarchctl` |

`nixosModules.monarchctl` is separate from `nixosModules.default`, which sets up
the server.
