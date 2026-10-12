# Build and release interface

This is the implementation contract shared by build recipes, release tooling,
and workflows. The checked-in `release.json` is the release input source of truth.

## Inputs

`release.json` schema 1 contains `native` (`url`, `rev`, `version`, `archive_url`,
`archive_sha256`, `udl_sha256`, `source_date_epoch`), `bindings_version`,
`uniffi_version`, `rust_version`, `features`, `default_features`, and `groups`.
Each group maps to an array of exact, flat release asset filenames. Additional
tool/SDK/image pins live in `tools` and dependency locks. Review selected native
features, default features, and UniFFI compatibility whenever changing releases.

Groups are `generated`, `linux-x86_64`, `linux-aarch64`, `android`, `darwin`,
and `jvm`. `generated` produces `generated-sources.tar` with the Kotlin, Android
Kotlin, Python, Swift, and FFI header sources. Linux groups produce their Python
wheel; x86_64 additionally produces `jvm-linux-x86_64.so`. Darwin produces
`jvm-darwin-x86_64.dylib`, `jvm-darwin-aarch64.dylib`, both Python wheels, and
`LDKNodeFFI.xcframework.zip`. Android and JVM each produce five unsigned primary
Maven files: binary, sources JAR, Javadoc JAR, POM, and module metadata.

Python asset names are `ldk_node-VERSION-py3-none-PLATFORM.whl`, with PLATFORM one of
`manylinux_2_28_x86_64`, `manylinux_2_28_aarch64`, `macosx_12_0_x86_64`,
`macosx_12_0_arm64`. Maven names use `ldk-node-jvm-VERSION` or
`ldk-node-android-VERSION`. Keep exact filenames in the manifest, not inferred globs.

## Build entrypoint

`scripts/build.sh --group GROUP --output DIR [--candidate DIR]` builds a single
group from committed inputs into an empty output directory. It runs the same
Nix derivations or pinned external recipes in CI and locally. No credentials,
signing, publication, source mutation, or dependency updates occur here.
`--candidate` is required only for JVM assembly, supplying the three named JVM
native assets. Acceptance of this assembly for final publication requires their
independent verification; CI can assemble the initial candidate beforehand.
Generated sources and all project intermediate outputs must rebuild locally
during verification, even if already present in a Nix store or build cache.

## Release CLI

Rust workspace exposes `release` via `nix run .#release -- COMMAND ...`.

- `prepare --repo OWNER/REPO`: validate clean committed inputs, push the dated
  preparation branch, dispatch candidate CI, watch the run, download candidates,
  and record a resumable ledger; never publish final packages.
- `inventory --dir DIR --recipe COMMIT --group GROUP [--candidate DIR]
  --output FILE`: record exact
  files, sizes, hashes, group, and hash of release inputs plus dependency locks.
- `candidate --dir DIR --recipe COMMIT --run-id ID --attempt N --output FILE`:
  combine complete group
  inventories into `candidate.json`; require all groups and identical inputs.
- `verify-candidate ID MANIFEST_SHA256 [--group GROUP] [--output DIR]`:
  authenticate the public `candidate.json` bytes against the supplied SHA-256,
  require recipe ancestry from official `main`, `MAJOR.MINOR.x`, or dated
  preparation branches, then check out the exact recipe in a temporary clone
  and independently rebuild the supported host groups. Continue after individual
  group failures. The caller's checkout remains unchanged. Write
  receipts under the result directory and report partial coverage explicitly.
  `scripts/verify.sh` provides the same command through the pinned Nix CLI.
- `verify --candidate URL_OR_DIR --group GROUP --output DIR`: anonymous download,
  independent rebuild, byte comparison, JSON receipt, and explicit partial
  coverage. Require the local clean recipe commit and input identity to match.
- `collect --candidate URL_OR_DIR --receipts DIR --output FILE`: rehash actual
  candidate files, require all groups and accepted independent receipts,
  validate receipt hosts and JVM native dependencies, emit a verified ledger.
- `publish --verified FILE --candidate URL_OR_DIR --destination DEST
  --confirm CANDIDATE_ID`: local-only final publication of verified bytes.
  Destinations: `testpypi`, `pypi`, `maven`, `swift`. Reject CI environments.

Candidate IDs: `candidate-VERSION-RECIPE12-RUN_ID-ATTEMPT`; non-SemVer, unique,
never overwrite. Candidates are PUBLIC GitHub prereleases (not drafts), with
flat files and `candidate.json`. Download with ordinary HTTPS without gh/PAT.
Build jobs are read-only; only candidate upload uses GitHub's temporary token.
All user PATs, registry tokens, and PGP signing keys remain on local machines.

The initial build/candidate repository is
`https://github.com/lightningdevkit/ldk-node-bindings`. SwiftPM distribution is
published separately to `https://github.com/lightningdevkit/ldk-node-swift`.
Its version commits contain the generated Swift wrapper, `Package.swift`,
licenses, and provenance; its release assets contain the unchanged verified
XCFramework ZIP. The package manifest points at that ZIP with its SHA-256.
Keep the distribution Git URL and asset destination configurable for a possible
Forgejo migration. The initial publication adapter supports GitHub; neither
Swift semantic version tags nor distribution metadata belong in the build
recipe repository.

Receipts have schema, candidate/input/recipe identity, group, file hashes/sizes,
and imported JVM native dependency hashes. Receipt collection is a deliberate
maintainer action, not automatic trust of public submissions. Transport hashes
alone never count as independent reproduction. Logs/timestamps are outside the
reproducible payloads. Final uploads rehash files again; publication records
support checking remote state and resuming without overwriting versions.

Maven publication uses the Central Portal Publisher API from the local CLI.
Sign the verified unsigned artifacts locally, assemble the repository-layout
bundle, upload with `publishingType=USER_MANAGED`, persist the deployment ID,
and poll validation. The explicitly confirmed publish invocation may then
publish a validated deployment and poll its final state. Keep the Portal token
and PGP key local; do not invoke legacy Gradle uploads to Central.
