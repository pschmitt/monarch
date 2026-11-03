# Monit agents

Monit agents push their status to Monarch over Monit's native M/Monit
protocol. No agent changes are needed beyond a `set mmonit` line. To have
Monarch poll an agent instead, see [pull mode](pull-mode.md).

## Pointing Monit at Monarch

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

To limit which agents may report, use `collector_allowlist` and
`collector_denylist` in the [configuration file](configuration.md).

## Service actions

For start, stop, restart, monitor, unmonitor and remonitor, Monarch connects to
the address Monit announces for its HTTP interface (or to the address the
report came from when Monit listens on all interfaces).

If that is not reachable (NAT, agents reachable only over a VPN such as
Tailscale), set an *action URL template* under *Settings → Hosts*, for example
`http://{short_hostname}.tailnet.ts.net:{port}`. Placeholders: `{hostname}`,
`{short_hostname}`, `{scheme}`, `{port}`, `{address}`. The template can also be
pinned with `action_url_template` in the config file.

A per-host URL and credential override (*Settings → Hosts*, or the host's
settings) takes precedence over the template.
