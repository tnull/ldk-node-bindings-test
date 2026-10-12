{ pkgs }:
let
  dependencies = builtins.fromJSON (builtins.readFile ../locks/maven.json);
  fetched = map (entry: {
    inherit (entry) path;
    artifact = pkgs.fetchurl { inherit (entry) url sha256; };
  }) dependencies;
in
pkgs.runCommand "ldk-bindings-maven-dependencies" { passAsFile = [ "buildCommand" ]; } (
  pkgs.lib.concatMapStringsSep "\n" (entry: ''
    mkdir -p "$out/$(dirname ${pkgs.lib.escapeShellArg entry.path})"
    ln -s ${entry.artifact} "$out/"${pkgs.lib.escapeShellArg entry.path}
  '') fetched
)
