<p align="center"><img src="docs/logo.svg" alt="Monarch" width="320"></p>

<p align="center">
  <a href="https://github.com/pschmitt/monarch/actions/workflows/ci.yml"><img src="https://github.com/pschmitt/monarch/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://nixbot.brkn.lol/repos/github/pschmitt/monarch"><img src="https://nixbot.brkn.lol/repos/github/pschmitt/monarch/badge.svg?branch=main" alt="nixbot"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="GPL-3.0"></a>
</p>

<h1 align="center">Monarch</h1>

<p align="center">An open source central dashboard for <a href="https://mmonit.com/monit/">Monit</a> agents.</p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/overview-dark.png">
  <img src="docs/screenshots/overview-light.png" alt="Monarch overview: fleet health, failing services and recent events">
</picture>

> [!NOTE]
> **Don't be a dick!** Monarch is an independent, clean-room Rust
> reimplementation of [M/Monit](https://mmonit.com/), and using Monarch does not
> require an M/Monit license. Please support Tildeslash, the people behind
> M/Monit, by [buying an M/Monit license](https://mmonit.com/shop/) if you find
> their work useful.

Monarch is a GPL-licensed alternative to M/Monit. Point your Monit agents at
it and get one view of the whole fleet, with history charts, events,
notifications and remote service control. There are no license keys and no
host limits.

## Features

- **Native Monit protocol**: agents report with `set mmonit …/collector`, so
  no agent changes are needed. See [Monit agents](docs/agents.md).
- **Pull mode**: Monarch polls an agent's Monit HTTP interface, directly or
  through SSH, so `set httpd` can stay bound to localhost. See
  [pull mode](docs/pull-mode.md).
- **Live dashboard**: hosts, services and events update in real time over
  Server-Sent Events, and the [favicon](docs/favicon.md) shows the fleet state.
- **History**: CPU, memory, load, filesystem, network, process and
  response-time metrics, downsampled from raw to 5 min to 1 h.
- **Remote control**: start, stop, restart, monitor, unmonitor and remonitor
  services through the agent's Monit HTTP interface.
- **Events**: an event log with search, filters and acknowledgements, plus
  offline detection when an agent goes silent.
- **Notifications**: webhook, ntfy, Gotify, Slack, Discord, Telegram and
  e-mail, with per-channel host, service and state filters.
- **Users and roles**: admin, operator, viewer and collector-only accounts,
  and OpenID Connect sign-in with roles mapped from groups. See
  [single sign-on](docs/oidc.md).
- **M/Monit-compatible API subset**: tools written for M/Monit, such as the
  [Home Assistant M/Monit integration](https://github.com/pschmitt/homeassistant-mmonit),
  keep working.
- **Single binary**: Rust (axum, SQLite) with the Svelte UI embedded, plus a
  NixOS module.

## Install

```sh
docker run -p 8080:8080 -v monarch:/data ghcr.io/pschmitt/monarch
```

Static binaries, `nix run` and the NixOS module are in
[docs/install.md](docs/install.md).

## Quick start

Open `http://<host>:8080` and create the admin account. Then add a `collector`
account under *Settings → Users* and point each agent's `monitrc` at Monarch:

```
set mmonit https://monit:PASSWORD@monarch.example.com/collector   # report to Monarch
set httpd port 2812                                                # needed for service actions
  allow monit:changeme
```

Reload Monit (`monit reload`) and the host shows up within one poll cycle.
[docs/agents.md](docs/agents.md) covers agents behind NAT or a VPN.

## Documentation

| Topic | |
| --- | --- |
| [Install](docs/install.md) | Binaries, container image, Nix and the NixOS module |
| [Monit agents](docs/agents.md) | `set mmonit`, collector accounts and service action URLs |
| [Pull mode](docs/pull-mode.md) | Poll agents directly or through SSH |
| [Configuration](docs/configuration.md) | Config file, environment variables and CLI user management |
| [Single sign-on](docs/oidc.md) | OpenID Connect and group-to-role mapping |
| [Reverse proxy](docs/reverse-proxy.md) | nginx settings and the NixOS nginx option |
| [Fleet status favicon](docs/favicon.md) | What the browser tab icon shows |
| [API](docs/API.md) | HTTP API used by the web UI |
| [Development](docs/development.md) | Dev shell, mock UI and tests |

## License

[GPL-3.0-or-later](LICENSE). Monarch is not affiliated with Tildeslash Ltd.,
the authors of Monit and M/Monit.
