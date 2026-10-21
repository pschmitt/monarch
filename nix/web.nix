{ lib, buildNpmPackage }:
buildNpmPackage {
  pname = "monarch-web";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../web;
    fileset = lib.fileset.difference ../web (
      lib.fileset.unions [
        (lib.fileset.maybeMissing ../web/node_modules)
        (lib.fileset.maybeMissing ../web/dist)
        (lib.fileset.maybeMissing ../web/dist-mock)
      ]
    );
  };
  npmDepsHash = "sha256-ltBuhNELUO6m0xGuq8XnnfC9ZLnRZBVRVyNO5RWcykM=";
  installPhase = ''
    runHook preInstall
    cp -r dist $out
    runHook postInstall
  '';
}
