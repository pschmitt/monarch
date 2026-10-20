{
  lib,
  runCommand,
  # crane's library for this nixpkgs (`crane.mkLib pkgs`).
  craneLib,
  callPackage,
  # The UI is architecture independent; pass it in to share it between builds.
  web ? callPackage ./web.nix { },
}:
let
  cargoToml = lib.importTOML ../Cargo.toml;
  inherit (cargoToml.package) version;
  # The package's own version, normalized out of everything the dependency build
  # sees, so a release bump keeps that derivation (and the caches holding it).
  normalized =
    file:
    builtins.toFile (baseNameOf file) (
      builtins.replaceStrings
        [ "name = \"monarch\"\nversion = \"${version}\"" ]
        [ "name = \"monarch\"\nversion = \"0.0.0\"" ]
        (builtins.readFile file)
    );
  depsLock = normalized ../Cargo.lock;
  depsToml = normalized ../Cargo.toml;
  common = {
    pname = "monarch";
    inherit version src;
    strictDeps = true;
    # Vendored crates don't depend on the package's version: shared by both builds.
    cargoVendorDir = craneLib.vendorCargoDeps { cargoLock = depsLock; };
    cargoExtraArgs = "--locked";
  };
  # Only the normalized manifests (eval-time files) and a stub main, unlike crane's
  # mkDummySrc, which also copies the real ones.
  depsSrc = runCommand "monarch-deps-src" { } ''
    mkdir -p $out/src
    cp ${depsToml} $out/Cargo.toml
    cp ${depsLock} $out/Cargo.lock
    echo 'fn main() {}' > $out/src/main.rs
  '';
  # The dependencies, built once: source changes and version bumps reuse them.
  deps = craneLib.buildDepsOnly (
    common
    // {
      pname = "monarch-deps";
      version = "0";
      dummySrc = depsSrc;
    }
  );
  src = lib.fileset.toSource {
    root = ./..;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../build.rs
      ../migrations
      ../src
      ../tests
    ];
  };
in
craneLib.buildPackage (
  common
  // {
    cargoArtifacts = deps;

    preBuild = ''
      mkdir -p web
      cp -r ${web} web/dist
      chmod -R u+w web/dist
    '';

    # The unit tests run here, reusing the built dependencies.
    doCheck = true;

    passthru = { inherit web; };

    meta = {
      description = "Modern, open source central monitoring server for Monit agents";
      homepage = "https://github.com/pschmitt/monarch";
      license = lib.licenses.gpl3Plus;
      mainProgram = "monarch";
      platforms = lib.platforms.unix;
    };
  }
)
