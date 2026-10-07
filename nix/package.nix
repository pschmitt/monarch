{
  lib,
  rustPlatform,
  callPackage,
  # The UI is architecture independent; pass it in to share it between builds.
  web ? callPackage ./web.nix { },
}:
rustPlatform.buildRustPackage {
  pname = "monarch";
  inherit (web) version;

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
