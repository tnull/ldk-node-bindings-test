# Reproducibility boundary

`release.json` is the release input source of truth. It identifies the native
source archive and commit, UDL checksum, package version, UniFFI version, Rust
version, enabled features, epoch, target inventories, and external tool inputs.
The recipe commit identifies the rest of this repository, including its Nix and
dependency locks. A candidate and its receipts must agree on both identities.

## What Nix controls

The flake pins development/build tools and exposes the release CLI and build
recipes. Project compilation, source generation, and archive assembly use
explicit input revisions. Dependency caches may save downloads; independent
verification must rebuild project outputs, including generated wrappers and
archives. A successful binary-cache download is not an independent rebuild.

Builds use a common source epoch, normalized paths, permissions and archive
ordering. Reproduction covers final distributable bytes: Python wheels, primary
unsigned Maven files, generated sources, native payloads, and the complete Swift
ZIP. Maven PGP signatures are added locally later and are outside that comparison.

## External platform inputs

- **Linux distributions:** pinned manylinux containers supply the distributable
  native environment. A Nix-built ELF file is not automatically suitable for a
  user's Linux installation. Check runtime dependencies and reject references
  to `/nix/store` in distributed native libraries.
- **Android:** pin the NDK and API levels and preserve the three supported ABIs
  and 16 KiB page alignment. Host architecture, Android tools and dependency
  locks are inputs, rather than whatever a runner happens to provide.
- **Apple:** use Xcode 16.4, build 16F6, with its macOS 15.5, iOS 18.5, and
  corresponding simulator SDKs. Xcode and licensed SDKs are installed locally;
  Nix does not redistribute them or freeze the complete macOS host. CI selects
  that exact installation and fails if it is absent.

The [GitHub macOS runner inventory](https://github.com/actions/runner-images/blob/main/images/macos/macos-15-arm64-Readme.md)
is a provisioning reference, not a reproducibility lock. Hosted runner labels
move over time. Host architecture is checked explicitly; tool and SDK mismatches
must be resolved by updating reviewed release inputs and creating new candidates.

## Composition and trust

A JVM JAR contains Linux and Darwin native libraries. Linux can reproduce the
JAR assembly using candidate Darwin inputs, but that receipt proves only the
assembly and its recorded dependencies. The collector also requires receipts
for the independently rebuilt Darwin group and Linux native output. Otherwise
an unchanged imported binary could be mistaken for a locally verified binary.

Receipts are assertions by the machines/operators that made them. The maintainer
selects the receipts to accept; the collector checks their consistency and file
hashes. It cannot establish the honesty of a receipt's author. Candidate hashes
and transport checksums likewise do not establish independent reproduction.

## Qualification

Before relying on a new target or toolchain, build it twice in separate clean
directories and compare complete output inventories. Then compare the CI
candidate with an independent local build. Keep consumer checks separate from
byte equality: a reproducible artifact can still contain a functional defect.
Run the retained platform tests and native-loading/import checks before release.

Record environment and test results outside the reproducible payloads. When
bytes differ, retain both outputs and investigate timestamps, embedded paths,
linker identifiers, SDK versions, dependency resolution, and archive metadata.
Do not normalize away meaningful native content merely to make a checksum pass.
