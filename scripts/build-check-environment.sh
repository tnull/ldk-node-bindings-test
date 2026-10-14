#!/usr/bin/env bash
# Exercise the real Nix entrypoint with hostile caller compiler settings.
set -euo pipefail
root=$PWD
scratch=$(mktemp -d /tmp/ldk-environment-check-XXXXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/scripts"
cp scripts/build.sh "$scratch/scripts/"
nixpkgs=$(nix --extra-experimental-features 'nix-command flakes' eval --impure --raw --expr "(builtins.getFlake \"$root\").inputs.nixpkgs.outPath")
system=$(nix --extra-experimental-features 'nix-command flakes' eval --impure --raw --expr builtins.currentSystem)
cat > "$scratch/flake.nix" <<EOF
{
  inputs.nixpkgs.url = "path:$nixpkgs";
  outputs = { self, nixpkgs }: {
    devShells.$system.build =
      let pkgs = import nixpkgs { system = "$system"; };
      in pkgs.mkShellNoCC { packages = [ pkgs.bash pkgs.coreutils ]; };
  };
}
EOF
cat > "$scratch/scripts/build-inner.sh" <<'PROBE'
set -euo pipefail
for variable in RUSTC_WRAPPER CARGO_ENCODED_RUSTFLAGS CARGO_PROFILE_RELEASE_OPT_LEVEL CARGO_BUILD_RUSTC CFLAGS_aarch64_linux_android; do
    [[ ! -v $variable ]] || { echo "Caller compiler setting leaked: $variable" >&2; exit 1; }
done
[[ ${LDK_XCODE_DEVELOPER_DIR:-} = /Applications/ChosenXcode.app/Contents/Developer ]] || {
    echo 'Caller Xcode selection was not preserved' >&2; exit 1;
}
[[ ${RELEASE_VERIFY:-} = 1 && ${CARGO_TARGET_DIR:-} = /tmp/verification/cargo-target ]]
[[ ${DOCKER_HOST:-} = unix:///tmp/test-docker.sock && ${DOCKER_CONTEXT:-} = test-context && ${XDG_RUNTIME_DIR:-} = /tmp/test-runtime ]]
[[ ${NIX_REMOTE:-} = auto && -n ${NIX_SSL_CERT_FILE:-} && -n ${SSL_CERT_FILE:-} ]] || {
    echo 'Nix store or TLS selectors were dropped' >&2; exit 1;
}
PROBE
nix --extra-experimental-features 'nix-command flakes' flake lock "$scratch" --offline
RUSTC_WRAPPER=/unwanted/cache CARGO_ENCODED_RUSTFLAGS=unwanted \
    CARGO_PROFILE_RELEASE_OPT_LEVEL=0 CARGO_BUILD_RUSTC=/unwanted/compiler \
    CFLAGS_aarch64_linux_android=unwanted RELEASE_VERIFY=1 \
    CARGO_TARGET_DIR=/tmp/verification/cargo-target \
    DOCKER_HOST=unix:///tmp/test-docker.sock DOCKER_CONTEXT=test-context XDG_RUNTIME_DIR=/tmp/test-runtime \
    NIX_REMOTE=auto NIX_SSL_CERT_FILE="${NIX_SSL_CERT_FILE:-/etc/ssl/certs/ca-certificates.crt}" \
    SSL_CERT_FILE="${SSL_CERT_FILE:-/etc/ssl/certs/ca-certificates.crt}" \
    DEVELOPER_DIR=/Applications/ChosenXcode.app/Contents/Developer \
    bash "$scratch/scripts/build.sh"
