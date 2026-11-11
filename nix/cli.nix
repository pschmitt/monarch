{
  lib,
  stdenv,
  runCommand,
  installShellFiles,
  # crane's library for this nixpkgs (`crane.mkLib pkgs`).
  craneLib,
}:
let
  cargoToml = lib.importTOML ../cli/Cargo.toml;
  inherit (cargoToml.package) version;
  # The package's own version, normalized out of everything the dependency build
  # sees, so a release bump keeps that derivation (and the caches holding it).
  normalized =
    file:
    builtins.toFile (baseNameOf file) (
      builtins.replaceStrings
        [ "name = \"monarchctl\"\nversion = \"${version}\"" ]
        [ "name = \"monarchctl\"\nversion = \"0.0.0\"" ]
        (builtins.readFile file)
    );
  depsLock = normalized ../cli/Cargo.lock;
  depsToml = normalized ../cli/Cargo.toml;
  src = lib.fileset.toSource {
    root = ../cli;
    fileset = lib.fileset.unions [
      ../cli/Cargo.toml
      ../cli/Cargo.lock
      ../cli/src
    ];
  };
  common = {
    pname = "monarchctl";
    inherit version src;
    strictDeps = true;
    cargoVendorDir = craneLib.vendorCargoDeps { cargoLock = depsLock; };
    cargoExtraArgs = "--locked";
  };
  # Only the normalized manifests (eval-time files) and a stub main.
  depsSrc = runCommand "monarchctl-deps-src" { } ''
    mkdir -p $out/src
    cp ${depsToml} $out/Cargo.toml
    cp ${depsLock} $out/Cargo.lock
    echo 'fn main() {}' > $out/src/main.rs
  '';
  # The dependencies, built once: source changes and version bumps reuse them.
  deps = craneLib.buildDepsOnly (
    builtins.removeAttrs common [ "src" ]
    // {
      pname = "monarchctl-deps";
      version = "0";
      dummySrc = depsSrc;
    }
  );
in
craneLib.buildPackage (
  common
  // {
    cargoArtifacts = deps;

    nativeBuildInputs = [ installShellFiles ];

    # The unit tests run here, reusing the built dependencies.
    doCheck = true;

    postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
      installShellCompletion --cmd monarchctl \
        --bash <($out/bin/monarchctl completions bash) \
        --fish <($out/bin/monarchctl completions fish) \
        --zsh <($out/bin/monarchctl completions zsh)
    '';

    meta = {
      description = "Command line and terminal UI to administer Monarch and see the status of your Monit hosts";
      homepage = "https://github.com/pschmitt/monarch";
      license = lib.licenses.gpl3Plus;
      mainProgram = "monarchctl";
      platforms = lib.platforms.unix;
    };
  }
)
