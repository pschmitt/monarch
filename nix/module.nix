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
      // lib.optionalAttrs (cfg.ensureUsers != [ ]) {
        ensure_users = lib.imap0 (i: u: {
          inherit (u) username role;
          password_file = "/run/credentials/monarch.service/user-${toString i}";
        }) cfg.ensureUsers;
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
              type = lib.types.str;
              description = "Account name.";
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

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the listen port in the firewall.";
    };
  };

  config = lib.mkIf cfg.enable {
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
          ++ lib.imap0 (i: u: "user-${toString i}:${u.passwordFile}") cfg.ensureUsers;
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

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [
      (lib.toInt (lib.last (lib.splitString ":" cfg.settings.listen)))
    ];
  };
}
