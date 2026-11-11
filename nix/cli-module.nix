{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.monarchctl;
  settingsFormat = pkgs.formats.toml { };
  configFile = settingsFormat.generate "monarchctl.toml" (
    lib.filterAttrs (_: v: v != null) (
      cfg.settings
      // lib.optionalAttrs (cfg.tokenFile != null) {
        # A plain string: the token itself never reaches the Nix store.
        token_file = toString cfg.tokenFile;
      }
    )
  );
in
{
  options.programs.monarchctl = {
    enable = lib.mkEnableOption "monarchctl, the command line and terminal UI for Monarch";

    package = lib.mkPackageOption pkgs "monarchctl" { };

    settings = lib.mkOption {
      type = lib.types.submodule {
        freeformType = settingsFormat.type;
        options = {
          url = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            example = "https://monarch.example.com";
            description = "Base URL of the Monarch server.";
          };
          tls_skip_verify = lib.mkOption {
            type = lib.types.nullOr lib.types.bool;
            default = null;
            description = "Do not verify the server's TLS certificate.";
          };
          timeout = lib.mkOption {
            type = lib.types.nullOr lib.types.ints.positive;
            default = null;
            example = 30;
            description = "Request timeout in seconds (monarchctl's default is 15).";
          };
        };
      };
      default = { };
      description = ''
        Contents of `/etc/monarchctl/config.toml`, the system-wide
        configuration. Users can override it in
        `$XDG_CONFIG_HOME/monarchctl/config.toml`. Other keys of the file
        (`token_command`, ...) can be set here too; see the monarchctl docs.
      '';
    };

    tokenFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = "/run/secrets/monarch-token";
      description = ''
        File holding the API token (an `mnr_...` token). Only the path goes
        into the configuration, the token is never copied to the Nix store.
        Give a string, not a Nix path literal, and make the file readable by
        the users who should use monarchctl.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
    environment.etc."monarchctl/config.toml".source = configFile;
  };
}
