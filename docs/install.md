# Install

Monarch is a single binary with the web UI embedded. Pick one of the
options below, then [point your Monit agents at it](agents.md).

## Binaries and container image

Static Linux binaries (x86_64, aarch64) are attached to each
[release](https://github.com/pschmitt/monarch/releases), and a multi-arch image
is published as `ghcr.io/pschmitt/monarch`:

```sh
docker run -p 8080:8080 -v monarch:/data ghcr.io/pschmitt/monarch
```

## Nix

```sh
nix run github:pschmitt/monarch -- --help
MONARCH_LISTEN=0.0.0.0:8080 nix run github:pschmitt/monarch
```

## NixOS

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

`services.monarch.settings` takes the keys of the
[configuration file](configuration.md). For pulled hosts and single sign-on see
[pull mode](pull-mode.md) and [single sign-on](oidc.md); for nginx see
[reverse proxy](reverse-proxy.md).

## First start

Open `http://<host>:8080`. On the first visit Monarch asks you to create the
admin account, unless one is configured with `initialAdmin` or
`initial_admin_password_file`.
