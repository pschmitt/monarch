{
  lib,
  buildNpmPackage,
  rustPlatform,
}:
let
  version = "0.1.0";

  web = buildNpmPackage {
    pname = "monarch-web";
    inherit version;
    src = lib.fileset.toSource {
      root = ../web;
      fileset = lib.fileset.difference ../web (
        lib.fileset.unions [
          (lib.fileset.maybeMissing ../web/node_modules)
          (lib.fileset.maybeMissing ../web/dist)
        ]
      );
    };
    npmDepsHash = "sha256-6UrzsPaP5AhOziPz5TDs1Hj7/LcoiDukkfWwzjnVsmE=";
    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
  };
in
rustPlatform.buildRustPackage {
  pname = "monarch";
  inherit version;

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

  cargoLock.lockFile = ../Cargo.lock;

  preBuild = ''
    mkdir -p web
    cp -r ${web} web/dist
    chmod -R u+w web/dist
  '';

  passthru = { inherit web; };

  meta = {
    description = "Modern, open source central monitoring server for Monit agents";
    homepage = "https://github.com/pschmitt/monarch";
    license = lib.licenses.gpl3Plus;
    mainProgram = "monarch";
    platforms = lib.platforms.unix;
  };
}
