# Monarch HTTP API

All native endpoints live under `/api` and speak JSON. Authentication uses a
session cookie (`monarch_session`, HttpOnly, SameSite=Lax) obtained through
`POST /api/auth/login`. Unauthenticated requests get `401 {"error": "..."}`;
insufficient privileges get `403`.

Timestamps are **unix seconds** (integers, or floats where sub-second precision
matters). Sizes are documented per field.

Roles: `admin` (everything), `operator` (read + service actions + ack events),
`viewer` (read only), `collector` (may only post to `/collector`).

## Auth

| Method | Path | Body | Response |
| --- | --- | --- | --- |
| GET | `/api/auth/me` | – | `{"user": User \| null, "setup_required": bool, "version": "0.1.0", "oidc": {"name": string} \| null, "password_login": bool}` (never 401) |
| POST | `/api/auth/setup` | `{"username","password"}` | `User` – only allowed while no user exists; logs in |
| POST | `/api/auth/login` | `{"username","password"}` | `User` + cookie |
| POST | `/api/auth/logout` | – | `204` |
| GET | `/api/auth/oidc/login?next=/path` | – | redirect to the identity provider (single sign-on, when `oidc` is set) |
| GET | `/api/auth/oidc/callback` | – | provider redirects back here; on success sets the cookie and redirects to `next`, on failure to `/login?sso_error=<message>` |

```ts
type Role = "admin" | "operator" | "viewer" | "collector";
interface User { id: number; username: string; role: Role; created_at: number; last_login: number | null; auth_source: "local" | "oidc"; sso: boolean }
```

## Overview

`GET /api/overview`

```ts
interface Overview {
  hosts: { total: number; online: number; offline: number; degraded: number };
  services: { total: number; ok: number; failed: number; unmonitored: number; pending: number };
  events_24h: number;               // number of events in the last 24h
  failing: ServiceRef[];            // all currently failed services, worst first
  top_cpu: ProcessRef[];            // top 8 processes by cpu percent (fleet-wide)
  top_mem: ProcessRef[];            // top 8 processes by memory kB
  filesystems: FilesystemRef[];     // top 8 filesystems by space percent
  recent_events: Event[];           // latest 15 events
  activity: { ts: number; ok: number; failed: number }[]; // 24 hourly buckets of event counts (state succeeded vs failed)
}
interface ServiceRef { host_id: number; host: string; service: string; type: ServiceType; status_text: string; since: number | null }
interface ProcessRef { host_id: number; host: string; service: string; cpu: number | null; mem_kb: number | null; mem_percent: number | null }
interface FilesystemRef { host_id: number; host: string; service: string; percent: number; used_mb: number; total_mb: number }
```

## Hosts

`GET /api/hosts` → `HostSummary[]`

```ts
type HostState = "ok" | "degraded" | "offline";
interface HostSummary {
  id: number;
  monit_id: string;
  hostname: string;            // as reported by monit (localhostname)
  display_name: string | null; // user override
  description: string | null;
  state: HostState;
  online: boolean;
  last_seen: number;
  first_seen: number;
  poll: number;                // monit poll interval in seconds
  monit_version: string | null;
  monit_uptime: number | null; // seconds
  os: { name: string | null; release: string | null; version: string | null; machine: string | null };
  cpu_count: number | null;
  mem_total_kb: number | null;
  swap_total_kb: number | null;
  hostgroups: string[];
  services: { total: number; ok: number; failed: number; unmonitored: number; pending: number };
  system: {                    // latest values of the System service, null if not reported
    uptime: number | null;     // seconds
    cpu: number | null;        // total cpu usage percent (user+system+nice+wait+...)
    cpu_user: number | null; cpu_system: number | null; cpu_wait: number | null;
    mem_percent: number | null; mem_kb: number | null;
    swap_percent: number | null; swap_kb: number | null;
    load: [number, number, number] | null;
  };
  sparkline: { cpu: number[]; mem: number[] }; // last ~60 raw samples, oldest first
  failing: string[];           // names of failed services
  muted_until: number | null;
  can_act: boolean;            // true when we know how to reach the monit httpd
}
```

`GET /api/hosts/:id` → `HostDetail`

```ts
interface HostDetail extends HostSummary {
  controlfile: string | null;
  incarnation: number | null;
  startdelay: number | null;
  httpd: { address: string | null; port: number | null; ssl: boolean; unixsocket: string | null } | null;
  remote_addr: string | null;          // ip the collector request came from
  monit_url: string | null;            // effective URL used for actions
  override_url: string | null;
  override_username: string | null;
  has_override_password: boolean;
  has_reported_credentials: boolean;
  tls_skip_verify: boolean;
  servicegroups: Record<string, string[]>; // group name -> service names
  services: Service[];
}
```

`PATCH /api/hosts/:id` (admin) body (all optional):
`{"display_name","description","override_url","override_username","override_password","tls_skip_verify","muted_until"}`
→ `HostDetail`. Empty string clears a field. `muted_until: null` unmutes.

`DELETE /api/hosts/:id` (admin) → `204`.

`POST /api/hosts/:id/test` (operator) → `{"ok": bool, "message": string, "latency_ms": number | null}` –
tries `GET <monit_url>/_status?format=xml` with the configured credentials.

## Services

```ts
type ServiceType = "filesystem" | "directory" | "file" | "process" | "host" | "system" | "fifo" | "program" | "net";
type ServiceState = "ok" | "failed" | "unmonitored" | "pending" | "init";
interface Service {
  id: number;
  host_id: number;
  name: string;
  type: ServiceType;
  type_id: number;
  state: ServiceState;
  status_text: string;      // e.g. "Running", "Does not exist", "Status failed", "Not monitored"
  status: number;           // raw monit error bitmask
  monitor: number;          // raw monit monitor bitmask
  monitor_mode: "active" | "passive";
  pending_action: string | null; // "restart", ... or null
  every: string | null;     // human readable schedule if not every cycle
  collected_at: number;     // float seconds
  state_since: number | null; // when the current state started
  groups: string[];
  events: number;           // number of stored events for this service
  data: ServiceData;
}
```

`ServiceData` depends on `type` (all fields optional / nullable):

```ts
// system
{ uptime, boottime, load: [l1,l5,l15], cpu: {user,system,nice,wait,hardirq,softirq,steal,guest,guestnice,total},
  memory: {percent, kb}, swap: {percent, kb}, fd: {allocated, unused, maximum} }
// process
{ pid, ppid, uid, euid, gid, uptime, threads, children,
  cpu: {percent, percent_total}, memory: {percent, percent_total, kb, kb_total},
  fd: {open, open_total, soft, hard}, io: {read_bps, write_bps, read_ops, write_ops},
  ports: Port[], unix: UnixSocket[] }
// filesystem
{ fstype, flags, mode, uid, gid, space: {percent, used_mb, total_mb}, inodes: {percent, used, total} | null,
  io: {read_bps, write_bps, read_ops, write_ops}, servicetime: {read, write, wait, run} | null }
// file / directory / fifo
{ mode, uid, gid, size, hardlinks, timestamps: {access, change, modify}, checksum: {type, value} | null }
// net
{ link: {state: number /* 1 up, 0 down, -1 n/a */, speed /* bit/s */, duplex /* 1 full */},
  download: {packets, bytes, errors, packets_total, bytes_total, errors_total},   // per second + totals
  upload:   {packets, bytes, errors, packets_total, bytes_total, errors_total} }
// host
{ icmp: {type, response_ms /* null = failed */}[], ports: Port[], unix: UnixSocket[] }
// program
{ started, exit_status, output }

interface Port { hostname: string; port: number; protocol: string; type: string; request: string | null; response_ms: number | null; cert_valid_days: number | null }
interface UnixSocket { path: string; protocol: string; response_ms: number | null }
```

`GET /api/hosts/:id/services/:name` → `Service & {"host": HostSummary, "recent_events": Event[]}`
(`:name` is URL-encoded service name).

`POST /api/hosts/:id/services/:name/action` (operator) body `{"action": "start"|"stop"|"restart"|"monitor"|"unmonitor"}`
→ `{"ok": true}` or `502 {"error": "..."}`.

`POST /api/hosts/:id/action` (operator) body `{"action": ..., "services": string[]}` → bulk variant.

## Metrics

`GET /api/metrics?host=<id>&service=<name>&metrics=<k1,k2>&from=<ts>&to=<ts>`

`from` defaults to now-6h, `to` to now. Resolution is chosen automatically
(raw < 6h, 5 min buckets < 7d, 1 h buckets otherwise).

```ts
interface MetricsResponse {
  resolution: number; // seconds per point, 0 = raw
  series: { metric: string; points: [number, number, number | null, number | null][] }[]; // [ts, avg, min, max]
}
```

Metric keys:

- system: `load1 load5 load15 cpu cpu_user cpu_system cpu_wait cpu_nice cpu_steal mem_percent mem_kb swap_percent swap_kb`
- process: `cpu cpu_total mem_percent mem_kb mem_kb_total threads children fd_open read_bps write_bps port_ms`
- filesystem: `space_percent space_used_mb inode_percent read_bps write_bps`
- net: `rx_bps tx_bps rx_pps tx_pps rx_errors tx_errors`
- host: `icmp_ms port_ms`
- file: `size`
- program: `exit_status`

## Events

`GET /api/events?host=<id>&service=<name>&state=<failed|succeeded|changed|changed_not>&q=<text>&unacked=1&limit=100&before=<event id>`
→ `{"events": Event[], "has_more": bool}` (newest first).

```ts
type EventState = "succeeded" | "failed" | "changed" | "changed_not" | "init";
interface Event {
  id: number;
  host_id: number | null;
  host: string | null;          // display name of the host
  service: string | null;
  service_type: ServiceType | null;
  kind: string;                 // e.g. "nonexist", "connection", "resource", "status", "heartbeat", "instance", "action"
  kind_label: string;           // e.g. "Does not exist"
  state: EventState;
  action: string | null;        // "alert", "restart", ...
  message: string;
  created_at: number;           // float seconds
  source: "monit" | "monarch";
  acked_by: string | null;
  acked_at: number | null;
}
```

`POST /api/events/:id/ack` (operator) → `Event`. `POST /api/events/ack` body `{"ids": number[]}` → `204`.

## Live updates

`GET /api/stream` – Server-Sent Events. Each message is a JSON object on the
default event name:

```ts
type StreamMessage =
  | { type: "host"; host: HostSummary }   // host received a report / changed state
  | { type: "host_removed"; id: number }
  | { type: "event"; event: Event }
  | { type: "ping"; ts: number };
```

## Users (admin)

- `GET /api/users` → `User[]`
- `POST /api/users` `{"username","password","role"}` → `User`
- `PATCH /api/users/:id` `{"username"?, "password"?, "role"?}` → `User` (users may change their own username and password via `PATCH /api/users/me {"username"?, "password"?, "current_password"?}`; `current_password` is required with `password`; usernames of SSO accounts cannot be changed)
- `DELETE /api/users/:id` → `204`
- `GET /api/tokens` → `{id, name, prefix, created_at, last_used, expires_at}[]` (your own API tokens)
- `POST /api/tokens` `{"name", "expires_days"?}` → the token info plus `token` (shown once)
- `DELETE /api/tokens/:id` → `204`

Users also have an optional `email` (`PATCH /api/users/:id` or `/me` with `{"email": ""}` clears it;
SSO accounts get theirs from the identity provider).

API tokens (`mnr_…`) authenticate every endpoint as their owner (same role) via
`Authorization: Bearer <token>`. They can only be managed from a browser session.

## Notification channels (admin)

```ts
type ChannelKind = "webhook" | "ntfy" | "gotify" | "slack" | "discord" | "telegram" | "email" | "apprise" | "webpush" | "exec";
interface Channel {
  id: number; name: string; kind: ChannelKind; enabled: boolean;
  config: Record<string, string>;   // secrets are returned as "********"
  filter: { hosts: string | null; services: string | null; states: EventState[]; include_heartbeat: boolean; events: string[] }; // hosts/services are regexes
  last_status: string | null; last_sent_at: number | null; created_at: number;
}
```

Config keys per kind:

- webhook: `url`, `method` (POST), `headers` (one `Key: value` per line)
- ntfy: `url` (e.g. `https://ntfy.sh/topic`), `token`
- gotify: `url`, `token`
- slack / discord: `url` (incoming webhook)
- telegram: `token`, `chat_id`
- email: `format` (`html`, the default, sends a styled multipart message with graphs; `text` plain only), `smtp_url` (`smtps://user:pass@host:465`), `from`, `to` (comma separated) and/or `to_roles` (comma separated roles; mails every user of those roles that has an email)
- apprise: `apprise_url` (an Apprise API notify URL), optional `tag`
- webpush: optional `users` (comma separated usernames); browsers enrol with `POST /api/push/subscriptions`
- exec: `command` (run with `/bin/sh -c` on the server; needs `allow_exec_channels`). Event in `MONARCH_*` env vars and JSON on stdin

`filter.events` limits a channel to event kinds (`GET /api/events/kinds`); empty = all. Kinds in
`settings.disabled_events` (`PATCH /api/settings`) never notify.

A channel with `default: true` (at most one; `PATCH` it onto another to move it) receives the events no
other enabled channel's filter claims. `filter.events: null` means all kinds, `[]` none.
`filter.group_minutes` overrides the global `settings.group_minutes` (default 10; 0 = send at once):
events for a channel are collected for that long and sent as one message.

Per-check overrides (beat the generic settings; admin to change):
`GET /api/checks/alerts` lists them, `GET|PUT|DELETE /api/hosts/:id/services/:name/alerts`
with `{"muted": bool, "events": string[]|null, "channels": number[]|null}`.

Endpoints: `GET /api/channels`, `POST /api/channels`, `PATCH /api/channels/:id`,
`DELETE /api/channels/:id`, `POST /api/channels/:id/test` → `{"ok": bool, "message": string}`.

## Connections (pull mode, admin)

Besides agents pushing to `/collector`, Monarch can **pull** from a Monit
agent's HTTP interface (`GET <url>/_status2?format=xml&level=full`), directly
or tunnelled through SSH (`ssh -W host:port <destination>`, so the agent's
httpd may listen on localhost only). Service actions for pulled hosts use the
same path. Since pulled documents contain no events, Monarch derives events
from state changes between polls.

```ts
interface Target {
  id: number;
  name: string;                 // label, e.g. "rofl-10"
  url: string;                  // monit httpd base URL as seen from the SSH host (or from Monarch when direct), e.g. "http://127.0.0.1:2812"
  username: string | null;      // monit httpd credentials
  has_password: boolean;
  ssh: { destination: string; port: number | null } | null; // e.g. {"destination": "root@rofl-10.example.com", "port": 22}
  interval: number;             // seconds between polls (default 30, min 5)
  tls_skip_verify: boolean;
  enabled: boolean;
  managed: boolean;             // declared in the config file / NixOS module – read only in the UI
  host_id: number | null;       // host created from this connection once polled
  last_status: string | null;   // "ok" or an error message
  last_polled_at: number | null;
  created_at: number;
}
```

- `GET /api/targets` → `Target[]`
- `POST /api/targets` body `{"name","url","username"?,"password"?,"ssh"?: {"destination","port"?} | null,"interval"?,"tls_skip_verify"?,"enabled"?}` → `Target` (polled immediately)
- `PATCH /api/targets/:id` same fields, partial; `"password": "********"` or omitted keeps the stored one → `Target`
- `DELETE /api/targets/:id` → `204` (the host and its history stay; delete the host separately)
- `POST /api/targets/test` body like `POST /api/targets` (or `{"id": n}` to test a stored one) → `{"ok": bool, "message": string, "hostname": string | null, "monit_version": string | null, "services": number | null, "latency_ms": number | null}` – fetches once without storing anything
- `POST /api/targets/:id/poll` → `Target` – poll now

`HostSummary` gains `"source": "push" | "pull"` and `"target_id": number | null`.

## Settings

`GET /api/settings` (admin) →

```ts
interface Settings {
  public_url: string;                     // used for the monitrc snippet + links in notifications
  retention: { raw_hours: number; rollup_5m_days: number; rollup_1h_days: number; events_days: number };
  heartbeat_grace: number;                // multiples of the poll interval before a host is offline
  collector_url: string;                  // e.g. https://monarch.example.com/collector
}
```

`PATCH /api/settings` (admin) with the same shape (partial) → `Settings`.

## M/Monit compatibility layer

So that existing tooling (e.g. the Home Assistant M/Monit integration) works
unchanged, a subset of the M/Monit HTTP API is provided:

- `POST /collector` – Monit agents (`set mmonit http://user:pass@host:port/collector`).
- `GET /index.csp`, `POST /z_security_check` (form: `z_username`, `z_password`).
- `GET /api/2/status/hosts/list`, `GET /api/2/status/hosts/get?id=`, `POST /api/2/action/service`
  (also reachable without the `/api/2` prefix).
