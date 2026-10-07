{
  description = "Monarch – a modern, open source central monitoring server for Monit agents";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          web = pkgs.callPackage ./nix/web.nix { };
          monarch = pkgs.callPackage ./nix/package.nix { inherit web; };
        in
        {
          inherit monarch web;
          default = monarch;
        }
        // nixpkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux rec {
          # Fully static (musl) binary, used for release assets and the image.
          monarch-static = pkgs.pkgsStatic.callPackage ./nix/package.nix { inherit web; };
          image = pkgs.dockerTools.buildLayeredImage {
            name = "ghcr.io/pschmitt/monarch";
            tag = monarch-static.version;
            contents = [
              pkgs.cacert
              pkgs.openssh
            ];
            extraCommands = "mkdir -p data tmp && chmod 1777 tmp";
            config = {
              Entrypoint = [ "${monarch-static}/bin/monarch" ];
              Env = [
                "MONARCH_LISTEN=0.0.0.0:8080"
                "MONARCH_DATABASE=/data/monarch.db"
                "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
              ];
              ExposedPorts."8080/tcp" = { };
              Volumes."/data" = { };
            };
          };
        }
      );

      overlays.default = final: _prev: {
        monarch = final.callPackage ./nix/package.nix { };
      };

      nixosModules.default = {
        imports = [ ./nix/module.nix ];
        nixpkgs.overlays = [ self.overlays.default ];
      };

      checks = forAllSystems (
        pkgs:
        {
          inherit (self.packages.${pkgs.stdenv.hostPlatform.system}) monarch;
        }
        // nixpkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          nixos = pkgs.testers.runNixOSTest (import ./nix/test.nix { inherit self; });
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            clippy
            rust-analyzer
            rustc
            rustfmt
            nodejs
            sqlite
            monit
            nixfmt
            statix
            deadnix
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
