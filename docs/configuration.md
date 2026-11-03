# Configuration

Monarch reads an optional TOML file (`--config` / `MONARCH_CONFIG`). On NixOS
the same keys go into `services.monarch.settings`.

```toml
listen = "127.0.0.1:8080"
database = "/var/lib/monarch/monarch.db"
public_url = "https://monarch.example.com"
collector_allow_anonymous = false
# Optional sender filtering. IPs/CIDRs match the request address; hostnames
# and domains match Monit's reported hostname (domains include subdomains).
# A non-empty allowlist requires a match; denylist matches always take priority.
collector_allowlist = [ "10.20.0.0/16", "example.com" ]
collector_denylist = [ "192.0.2.10", "blocked.example.net" ]
session_days = 30
initial_admin_user = "admin"
initial_admin_password_file = "/run/secrets/monarch-admin"

# created on startup; role and password are reset to these values
[[ensure_users]]
username = "monit"
role = "collector"           # default
password_file = "/run/secrets/monarch-collector"
```

For `action_url_template` see [Monit agents](agents.md); for `[oidc]` see
[single sign-on](oidc.md).

## Environment

`MONARCH_LISTEN`, `MONARCH_DATABASE`, `MONARCH_PUBLIC_URL`,
`MONARCH_ACTION_URL_TEMPLATE`, `MONARCH_INITIAL_ADMIN_USER` and
`MONARCH_INITIAL_ADMIN_PASSWORD_FILE` override the file. `MONARCH_LOG` sets the
tracing filter (default `info`).

## Runtime settings

Retention, the offline grace period and the public URL can be changed at
runtime under *Settings*.

## Users from the CLI

The password is read from stdin:

```sh
echo 'hunter22hunter22' | monarch user-add alice --role operator
echo 'n3w-passw0rd' | monarch passwd alice
```

Roles: `admin`, `operator`, `viewer` and `collector` (report only, no UI
sign-in).
