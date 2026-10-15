{ pkgs, self }:
let
  lib = pkgs.lib;
  inputs = builtins.fromJSON (builtins.readFile ../release.json);
  rust = pkgs.rust-bin.stable.${inputs.rust_version}.minimal.override {
    targets =
      if pkgs.stdenv.hostPlatform.isDarwin then
        [
          "x86_64-apple-darwin"
          "aarch64-apple-darwin"
          "aarch64-apple-ios"
          "x86_64-apple-ios"
          "aarch64-apple-ios-sim"
        ]
      else
        lib.optionals pkgs.stdenv.hostPlatform.isx86_64 [
          "x86_64-linux-android"
          "aarch64-linux-android"
          "armv7-linux-androideabi"
        ];
    extensions = [
      "rustfmt"
      "clippy"
    ];
  };
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rust;
    rustc = rust;
  };
  nativeArchive = pkgs.fetchurl {
    url = inputs.native.archive_url;
    sha256 = inputs.native.archive_sha256;
    name = "ldk-node-${inputs.native.rev}.tar.gz";
  };
  nativeSource = pkgs.runCommand "ldk-node-source-${inputs.native.version}" { } ''
    mkdir -p $out
    tar -xf ${nativeArchive} --strip-components=1 -C $out
    echo '${inputs.native.udl_sha256}  '$out/bindings/ldk_node.udl | ${pkgs.coreutils}/bin/sha256sum -c -
    cp ${../locks/native.Cargo.lock} $out/Cargo.lock
  '';
  nativeDeps = rustPlatform.importCargoLock { lockFile = ../locks/native.Cargo.lock; };
  bindgen = rustPlatform.buildRustPackage {
    pname = "ldk-node-uniffi-bindgen";
    version = inputs.uniffi_version;
    src = ../bindings/uniffi-bindgen;
    cargoLock.lockFile = ../bindings/uniffi-bindgen/Cargo.lock;
    doCheck = true;
  };
  python = pkgs.python3.withPackages (p: [
    p.build
    p.hatchling
    p.packaging
    p.twine
    p.requests
  ]);
  android = pkgs.androidenv.composeAndroidPackages {
    platformVersions = [ inputs.tools.android_compile_sdk ];
    buildToolsVersions = [ inputs.tools.android_build_tools ];
    includeNDK = true;
    ndkVersions = [ inputs.tools.android_ndk ];
    includeEmulator = false;
    includeSystemImages = false;
  };
  gradleArchive = pkgs.fetchurl {
    url = "https://downloads.gradle.org/distributions/gradle-7.6-bin.zip";
    sha256 = "7ba68c54029790ab444b39d7e293d3236b2632631fb5f2e012bb28b4ff669e4b";
  };
  gradle = pkgs.runCommand "gradle-7.6" { nativeBuildInputs = [ pkgs.unzip ]; } ''
    unzip -q ${gradleArchive}
    mv gradle-7.6 $out
  '';
  mirror = import ./maven-deps.nix { inherit pkgs; };
  generated =
    pkgs.runCommand "ldk-node-generated-sources"
      {
        nativeBuildInputs = [
          bindgen
          pkgs.gnutar
          pkgs.coreutils
        ];
      }
      ''
        cd ${self}
        bash scripts/build-generate.sh ${nativeSource} $TMPDIR/generated
        mkdir -p $out
        tar --sort=name --mtime="@${toString inputs.native.source_date_epoch}" --owner=0 --group=0 --numeric-owner --mode="u+rwX,go+rX,go-w" --format=ustar -cf $out/generated-sources.tar -C $TMPDIR/generated .
      '';
  common = [
    rust
    bindgen
    python
    pkgs.jq
    pkgs.libxml2
    pkgs.curl
    pkgs.git
    pkgs.gnutar
    pkgs.gzip
    pkgs.unzip
    pkgs.zip
    pkgs.coreutils
    pkgs.findutils
    pkgs.diffutils
    pkgs.gnused
    pkgs.gnugrep
    pkgs.bash
    pkgs.gnupg
    pkgs.gh
    pkgs.nix
    pkgs.uv
  ];
  buildShell = pkgs.mkShell {
    packages =
      common ++ [ pkgs.jdk17 ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.patchelf ];
    LDK_BUILD_SHELL = "1";
    LDK_NATIVE_SOURCE = nativeSource;
    LDK_NATIVE_DEPS = nativeDeps;
    LDK_RUST = rust;
    JAVA_HOME = "${pkgs.jdk17}/lib/openjdk";
    shellHook = ''
      export SOURCE_DATE_EPOCH=${toString inputs.native.source_date_epoch}
    '';
  };
in
{
  packages = {
    inherit
      bindgen
      nativeSource
      nativeDeps
      rust
      python
      gradle
      generated
      ;
    maven-deps = mirror;
    container-client = pkgs.docker-client;
    android-sdk = android.androidsdk;
    release = rustPlatform.buildRustPackage {
      pname = "ldk-bindings-release";
      version = "0.1.0";
      src = lib.cleanSource ../.;
      cargoLock.lockFile = ../Cargo.lock;
      nativeBuildInputs = [ pkgs.makeWrapper ];
      nativeCheckInputs = [
        pkgs.git
        pkgs.bash
        pkgs.gnutar
        pkgs.coreutils
        pkgs.zip
      ];
      postInstall = ''
        wrapProgram $out/bin/release --prefix PATH : ${
          lib.makeBinPath [
            pkgs.gh
            pkgs.curl
            pkgs.uv
            pkgs.gnupg
            pkgs.zip
            pkgs.gnutar
            pkgs.coreutils
            pkgs.git
            pkgs.nix
          ]
        }
      '';
      doCheck = true;
    };
    default = bindgen;
  };
  mavenPublication =
    { source, platform }:
    import ./maven.nix {
      inherit
        pkgs
        gradle
        mirror
        source
        platform
        inputs
        ;
      android = android.androidsdk;
    };
  shells = {
    default = buildShell;
    build = buildShell;
    checks = pkgs.mkShell {
      packages = [
        rust
        pkgs.shellcheck
        pkgs.git
        pkgs.bash
        pkgs.coreutils
        pkgs.findutils
      ];
    };
  };
}
