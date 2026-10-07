{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.monarch;
  settingsFormat = pkgs.formats.toml { };
  configFile = settingsFormat.generate "monarch.toml" (
    lib.filterAttrs (_: v: v != null) (
      cfg.settings
      // lib.optionalAttrs (cfg.initialAdmin.passwordFile != null) {
        initial_admin_user = cfg.initialAdmin.user;
        initial_admin_password_file = "/run/credentials/monarch.service/initial-admin-password";
      }
      // {
        ssh_binary = lib.getExe' cfg.sshPackage "ssh";
      }
      // lib.optionalAttrs (cfg.oidc.clientSecretFile != null) {
        oidc = (cfg.settings.oidc or { }) // {
          client_secret_file = "/run/credentials/monarch.service/oidc-client-secret";
        };
      }
      // lib.optionalAttrs (cfg.ssh.privateKeyFile != null) {
        ssh_identity_file = "/run/credentials/monarch.service/ssh-key";
      }
      // lib.optionalAttrs (cfg.targets != [ ]) {
        targets = lib.imap0 (
          i: t:
          lib.filterAttrs (_: v: v != null) {
            inherit (t)
              name
              url
              username
              interval
              ;
            tls_skip_verify = t.tlsSkipVerify;
            password_file =
              if t.passwordFile != null then "/run/credentials/monarch.service/target-${toString i}" else null;
            ssh_destination = if t.ssh != null then t.ssh.destination else null;
            ssh_port = if t.ssh != null then t.ssh.port else null;
          }
        ) cfg.targets;
      }
      // lib.optionalAttrs (cfg.ensureUsers != [ ]) {
        ensure_users = lib.imap0 (
          i: u:
          lib.filterAttrs (_: v: v != null) {
            inherit (u) username role;
            username_file =
              if u.usernameFile != null then "/run/credentials/monarch.service/user-${toString i}-name" else null;
            password_file = "/run/credentials/monarch.service/user-${toString i}";
          }
        ) cfg.ensureUsers;
      }
    )
  );
in
{
  options.services.monarch = {
    enable = lib.mkEnableOption "Monarch, a central monitoring server for Monit agents";

    package = lib.mkPackageOption pkgs "monarch" { };

    settings = lib.mkOption {
      description = ''
        Monarch configuration, see `config.rs` for all options.
        Runtime settings (retention, public URL, ...) can also be changed in the web UI.
      '';
      default = { };
      type = lib.types.submodule {
        freeformType = settingsFormat.type;
        options = {
          listen = lib.mkOption {
            type = lib.types.str;
            default = "127.0.0.1:8080";
            description = "Address and port to listen on.";
          };
          database = lib.mkOption {
            type = lib.types.str;
            default = "/var/lib/monarch/monarch.db";
            description = "Path of the SQLite database.";
          };
          public_url = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            example = "https://monarch.example.com";
            description = "Externally reachable URL, used for links and the monitrc snippet.";
          };
          collector_allow_anonymous = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Accept collector posts from Monit agents without valid credentials.";
          };
        };
      };
    };

    initialAdmin = {
      user = lib.mkOption {
        type = lib.types.str;
        default = "admin";
        description = "Name of the admin account created on first start.";
      };
      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = ''
          File containing the password of the initial admin account. Only used
          while no user exists. Without it, the first visitor of the web UI is
          asked to create the admin account.
        '';
      };
    };

    ensureUsers = lib.mkOption {
      description = ''
        Accounts managed declaratively: created on startup, and their role and
        password reset to the configured values. Handy for the `collector`
        account Monit agents use.
      '';
      default = [ ];
      type = lib.types.listOf (
        lib.types.submodule {
          options = {
            username = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Account name.";
            };
            usernameFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "File containing the account name (instead of `username`).";
            };
            role = lib.mkOption {
              type = lib.types.enum [
                "admin"
                "operator"
                "viewer"
                "collector"
              ];
              default = "collector";
              description = "Role of the account.";
            };
            passwordFile = lib.mkOption {
              type = lib.types.path;
              description = "File containing the account's password (at least 8 characters).";
            };
          };
        }
      );
    };

    oidc.clientSecretFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        File containing the OIDC client secret. Configure the provider itself
        under `settings.oidc` (`issuer`, `client_id`, `display_name`,
        `admin_groups`, `operator_groups`, `default_role`, ...). The redirect
        URI to register is `<public_url>/api/auth/oidc/callback`.
      '';
    };

    sshPackage = lib.mkPackageOption pkgs "openssh" { };

    ssh.privateKeyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = "/run/secrets/monarch-ssh-key";
      description = ''
        Private key used for connections that tunnel through SSH. Host keys
        are trusted on first use and stored in the state directory.
      '';
    };

    targets = lib.mkOption {
      description = ''
        Monit agents Monarch polls itself (pull mode), directly or through
        SSH. They show up read-only under Settings → Connections.
      '';
      default = [ ];
      example = lib.literalExpression ''
        [
          {
            name = "router";
            url = "http://127.0.0.1:2812";
            username = "monit";
            passwordFile = "/run/secrets/monit-httpd-password";
            ssh.destination = "root@router.lan";
          }
        ]
      '';
      type = lib.types.listOf (
        lib.types.submodule {
          options = {
            name = lib.mkOption {
              type = lib.types.str;
              description = "Unique name of the connection.";
            };
            url = lib.mkOption {
              type = lib.types.str;
              example = "http://127.0.0.1:2812";
              description = "Monit HTTP interface, as seen from the SSH host when `ssh` is set.";
            };
            username = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Monit httpd user.";
            };
            passwordFile = lib.mkOption {
              type = lib.types.nullOr lib.types.path;
              default = null;
              description = "File containing the Monit httpd password.";
            };
            ssh = lib.mkOption {
              default = null;
              description = "Tunnel the connection through SSH (`ssh -W`).";
              type = lib.types.nullOr (
                lib.types.submodule {
                  options = {
                    destination = lib.mkOption {
                      type = lib.types.str;
                      example = "root@host.example.com";
                      description = "SSH destination.";
                    };
                    port = lib.mkOption {
                      type = lib.types.nullOr lib.types.port;
                      default = null;
                      description = "SSH port.";
                    };
                  };
                }
              );
            };
            interval = lib.mkOption {
              type = lib.types.ints.between 5 86400;
              default = 30;
              description = "Seconds between polls.";
            };
            tlsSkipVerify = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Accept self-signed certificates of the Monit httpd.";
            };
          };
        }
      );
    };

    nginx = {
      enable = lib.mkEnableOption "an nginx virtual host reverse-proxying Monarch";
      domain = lib.mkOption {
        type = lib.types.str;
        example = "monarch.example.com";
        description = ''
          Name of the nginx virtual host. Further virtual host settings (e.g.
          `acmeRoot`) can be set through `services.nginx.virtualHosts.<domain>`.
        '';
      };
      enableACME = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Obtain a certificate from ACME and force HTTPS.";
      };
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the listen port in the firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = lib.imap0 (i: u: {
      assertion = (u.username != null) != (u.usernameFile != null);
      message = "services.monarch.ensureUsers.${toString i}: set exactly one of username or usernameFile";
    }) cfg.ensureUsers;

    systemd.services.monarch = {
      description = "Monarch – central monitoring for Monit agents";
      documentation = [ "https://github.com/pschmitt/monarch" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];
      environment.MONARCH_CONFIG = configFile;
      serviceConfig = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
        RestartSec = 5;
        DynamicUser = true;
        StateDirectory = "monarch";
        StateDirectoryMode = "0700";
        LoadCredential =
          lib.optional (
            cfg.initialAdmin.passwordFile != null
          ) "initial-admin-password:${cfg.initialAdmin.passwordFile}"
          ++ lib.imap0 (i: u: "user-${toString i}:${u.passwordFile}") cfg.ensureUsers
          ++ lib.concatLists (
            lib.imap0 (
              i: u: lib.optional (u.usernameFile != null) "user-${toString i}-name:${u.usernameFile}"
            ) cfg.ensureUsers
          )
          ++ lib.optional (cfg.ssh.privateKeyFile != null) "ssh-key:${cfg.ssh.privateKeyFile}"
          ++ lib.optional (
            cfg.oidc.clientSecretFile != null
          ) "oidc-client-secret:${cfg.oidc.clientSecretFile}"
          ++ lib.concatLists (
            lib.imap0 (
              i: t: lib.optional (t.passwordFile != null) "target-${toString i}:${t.passwordFile}"
            ) cfg.targets
          );
        # Hardening
        CapabilityBoundingSet = "";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
        ];
        UMask = "0077";
      };
    };

    services.nginx.virtualHosts = lib.mkIf cfg.nginx.enable {
      ${cfg.nginx.domain} = {
        enableACME = lib.mkDefault cfg.nginx.enableACME;
        forceSSL = lib.mkDefault cfg.nginx.enableACME;
        locations."/" = {
          proxyPass = "http://${cfg.settings.listen}";
          recommendedProxySettings = true;
          extraConfig = ''
            # live updates (server-sent events)
            proxy_buffering off;
            proxy_read_timeout 1h;
            # large monit status documents (program output)
            client_max_body_size 32m;
          '';
        };
      };
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [
      (lib.toInt (lib.last (lib.splitString ":" cfg.settings.listen)))
    ];
  };
}
