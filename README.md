# 👑 Monarch

**A modern, open source central dashboard for [Monit](https://mmonit.com/monit/) agents.**

Monarch is a drop-in, GPL-licensed alternative to M/Monit: point your Monit
agents at it and get a fast, beautiful overview of your whole fleet, with
history charts, events, notifications and remote service control. No license
keys, no host limits.

- **Speaks Monit's native protocol**: agents report via `set mmonit …/collector`, so no agent changes are needed
- **Live dashboard**: hosts, services and events update in real time (Server-Sent Events)
- **History**: CPU, memory, load, filesystem, network, process and response-time metrics with automatic
  downsampling (raw → 5 min → 1 h)
- **Remote control**: start, stop, restart, monitor and unmonitor services through the agent's Monit HTTP interface
- **Events**: full event log with search, filters and acknowledgements; offline detection when an agent goes silent
- **Notifications**: webhook, ntfy, Gotify, Slack, Discord, Telegram and e-mail, with per-channel host/service/state filters
- **Users and roles**: admin, operator, viewer and collector-only accounts
- **M/Monit-compatible API subset**: tools written for M/Monit (for example the
  [Home Assistant M/Monit integration](https://github.com/pschmitt/homeassistant-mmonit)) keep working
- **A single binary**: Rust (axum + SQLite) with the Svelte UI embedded, and a NixOS module

## Quick start

### Nix

```sh
nix run github:pschmitt/monarch -- --help
MONARCH_LISTEN=0.0.0.0:8080 nix run github:pschmitt/monarch
```

Open `http://<host>:8080`; on first visit you are asked to create the admin
account.

### NixOS

```nix
{
  inputs.monarch.url = "github:pschmitt/monarch";

  outputs = { nixpkgs, monarch, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        monarch.nixosModules.default
        {
          services.monarch = {
            enable = true;
            settings = {
              listen = "127.0.0.1:8080";
              public_url = "https://monarch.example.com";
            };
            # optional, otherwise the web UI asks for an admin account on first visit
            initialAdmin.passwordFile = "/run/secrets/monarch-admin";
            # declaratively managed accounts, e.g. the one Monit agents report with
            ensureUsers = [
              { username = "monit"; role = "collector"; passwordFile = "/run/secrets/monarch-collector"; }
            ];
          };
        }
      ];
    };
  };
}
```

### Pointing Monit at Monarch

Add this to each agent's `monitrc`:

```
set mmonit https://USER:PASSWORD@monarch.example.com/collector

# Optional, but needed for start/stop/restart from the UI. Monit sends these
# credentials along with each report ("register credentials" is the default).
set httpd port 2812
  allow monit:changeme
```

`USER:PASSWORD` is any Monarch account. A dedicated account with the
`collector` role (it cannot sign in to the UI) is recommended. Monit supports
several `set mmonit` lines, so Monarch can run next to an existing M/Monit
while you try it out.

For service actions, Monarch connects to the address Monit announces for its
HTTP interface (or to the address the report came from when Monit listens on
all interfaces). Override the URL and credentials per host in the host's
settings if the agent is behind NAT.

## Configuration

Monarch reads an optional TOML file (`--config` / `MONARCH_CONFIG`):

```toml
listen = "127.0.0.1:8080"
database = "/var/lib/monarch/monarch.db"
public_url = "https://monarch.example.com"
collector_allow_anonymous = false
session_days = 30
initial_admin_user = "admin"
initial_admin_password_file = "/run/secrets/monarch-admin"

# created on startup; role and password are reset to these values
[[ensure_users]]
username = "monit"
role = "collector"           # default
password_file = "/run/secrets/monarch-collector"
```

Environment overrides: `MONARCH_LISTEN`, `MONARCH_DATABASE`, `MONARCH_PUBLIC_URL`,
`MONARCH_INITIAL_ADMIN_USER`, `MONARCH_INITIAL_ADMIN_PASSWORD_FILE`, `MONARCH_LOG`
(tracing filter, default `info`).

Retention, the offline grace period and the public URL can be changed at
runtime under *Settings*.

User management from the CLI (the password is read from stdin):

```sh
echo 'hunter22hunter22' | monarch user-add alice --role operator
echo 'n3w-passw0rd' | monarch passwd alice
```

### Reverse proxy

Monarch works behind nginx or similar. Forward `X-Forwarded-For` (used to find
agents' addresses) and `X-Forwarded-Proto` (for secure cookies), and disable
buffering for `/api/stream`:

```nginx
location / {
  proxy_pass http://127.0.0.1:8080;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto $scheme;
  proxy_buffering off;
}
```

## API

See [docs/API.md](docs/API.md).

## Development

```sh
nix develop
(cd web && npm ci && npm run dev)      # UI on :5173, proxies /api to :8080
cargo run                              # backend on :8080
(cd web && VITE_MOCK=1 npm run dev)    # UI with mock data, no backend needed
cargo test
nix flake check                        # includes a NixOS VM test with a real Monit agent
```

## License

[GPL-3.0-or-later](LICENSE). Monarch is not affiliated with Tildeslash Ltd.,
the authors of Monit and M/Monit.
