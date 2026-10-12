# LDK Node bindings

Build and independently verify Swift, Kotlin JVM, Kotlin Android, and Python
distributions of [LDK Node](https://github.com/lightningdevkit/ldk-node).

GitHub runners build unsigned candidates. Anyone can download those candidates,
rebuild their supported targets, and compare the exact package bytes without
GitHub credentials. A maintainer publishes the verified files through a separate
local command. Registry credentials, signing keys, and personal access tokens
stay on the maintainer's machine.

`release.json` selects the native revision, bindings version, and toolchain.
Rust source, the authoritative UDL, native tags, and crates.io publication remain
in the native repository. A sibling checkout is not needed.

| Repository | Purpose |
| --- | --- |
| [ldk-node](https://github.com/lightningdevkit/ldk-node) | Native Rust source, authoritative UDL, and native releases |
| [ldk-node-bindings](https://github.com/lightningdevkit/ldk-node-bindings) | Build recipes, CI, public candidates, verification, and local release tooling |
| [ldk-node-swift](https://github.com/lightningdevkit/ldk-node-swift) | SwiftPM distribution source, `Package.swift`, version tags, and verified XCFramework release assets |

The Swift repository is a publication destination. All Swift builds and
candidate verification run from the recipes in this bindings repository.

## Build environment

Install **Nix 2.35.2**, with `nix-command` and `flakes` enabled, and Git. The
checked-in `flake.lock`, `release.json`, and dependency locks select the build
inputs. Do not update them while building or verifying a candidate.

Linux native builds also need a working Docker daemon, or Podman selected with
`CONTAINER_ENGINE=podman`, to run the pinned manylinux images in `release.json`.
On Fedora with packaged Nix, install `nix-daemon`, start it with
`sudo systemctl enable --now nix-daemon`, and set `NIX_REMOTE=daemon` if Nix
tries to write to the system store directly. Run verification as your normal
user. Linux build and wheel-test containers disable SELinux label separation
for their system-store mounts; host-wide SELinux settings and labels stay intact.
Apple builds and verification run on an **Apple Silicon Mac** with macOS 27,
full **Xcode 27.0 (27A266a)**, its macOS and iOS/simulator 27.0 SDKs, and the
Xcode license accepted. CI uses GitHub's `xcode-27` runner, currently in preview.
Select that installation with `xcode-select`, or set:

```bash
export DEVELOPER_DIR=/Applications/Xcode_27.0.app/Contents/Developer
```

Nix supplies the shared tools on both Linux and macOS. Xcode is an additional
local prerequisite; the Darwin group does not need Docker. Intel macOS artifacts
are built on Apple Silicon; Intel Macs are not supported build hosts. Linux and
Apple groups require their respective hosts, so one machine cannot independently
verify the complete release. See [the reproducibility boundary](docs/reproducibility.md).

The resulting Python wheels and Kotlin JVM libraries require macOS 12 or later
on either architecture. Swift supports macOS 12 and iOS 15 or later. These
deployment targets describe artifact compatibility, separately from the macOS 27
build host requirement.

Build one group into a new directory:

```bash
scripts/build.sh --group generated --output /tmp/ldk-node-generated
scripts/build.sh --group linux-x86_64 --output /tmp/ldk-node-linux
# On an Apple Silicon Mac with the selected Xcode:
scripts/build.sh --group darwin --output /tmp/ldk-node-darwin
```

The entrypoint enters the pinned Nix shell. The same command runs in CI. Cargo
output belongs in disposable directories under `/tmp`.

| Group | Host | Candidate outputs |
| --- | --- | --- |
| `generated` | Linux x86_64 | Generated Kotlin, Python, Swift, and FFI sources |
| `linux-x86_64` | Linux x86_64 + Docker | Python wheel and JVM Linux library |
| `linux-aarch64` | Linux aarch64 + Docker | Python wheel |
| `android` | Linux x86_64 | AAR and unsigned Maven publication files |
| `darwin` | Mac arm64 + Xcode | Two Python wheels, two JVM libraries, Swift XCFramework ZIP |
| `jvm` | Linux x86_64 | JAR with all three native libraries and unsigned Maven files |

JVM assembly takes `--candidate DIR` containing the native outputs. Assembly
verification records their hashes; the complete release also needs independent
verification of the Linux and Darwin groups. Python wheels support Python 3.10
and newer on Linux x86_64/aarch64 and macOS x86_64/arm64.

## Prepare a candidate

Use the repository-local [release skill](skills/release/SKILL.md), for example:

> Use skills/release/SKILL.md to prepare a bindings release for LDK Node VERSION
> in lightningdevkit/ldk-node-bindings. Stop before publication.

The skill prepares a reviewed recipe branch for the requested native release,
including updated pins, dependency locks, and compatible bindings/tests. Patch
releases for older series start from that series' previous recipe. See
[release versions and branches](docs/releasing.md#release-versions-and-branches).

Or run the entrypoint directly from a clean, committed preparation branch:

```bash
nix run .#release -- prepare --repo lightningdevkit/ldk-node-bindings
```

Authenticate locally with `gh auth login`, or supply `GH_TOKEN` in the local
environment. The account needs repository contents and Actions write access to
push the branch and dispatch CI. Pushing workflow changes also needs Workflows
write permission (the `workflow` scope for classic PATs). Do not put tokens into
prompts, tracked files, workflow inputs, or repository secrets. The repository
must be public, and `candidate.yml` must already exist on its default branch
for GitHub to accept a workflow dispatch. For a new repository, merge the initial
infrastructure PR before preparing the first hosted candidate.

Preparation pushes the reviewed recipe and starts
[the candidate workflow](.github/workflows/candidate.yml). Linux x86_64,
Linux aarch64, and Mac arm64 runners build the platform groups. A final job
assembles JVM artifacts, validates all inventories, and creates a **public
prerelease** with a unique tag:

```text
candidate-VERSION-RECIPE12-RUN_ID-ATTEMPT
```

Its release assets include every candidate file, `candidate.json`,
`checksums.txt`, and the group inventories. These release assets remain available
after the temporary Actions transport artifacts expire. The prerelease is
public so downloads need neither a PAT nor repository write access. Candidate
tags and assets are never replaced; a new run or attempt creates a new identity.

Build jobs have read-only repository access. Only the candidate upload job has
`contents: write`, using GitHub's automatic temporary `GITHUB_TOKEN`. No workflow
has registry credentials or a final publication step. Pull request checks never
upload public candidates.

## Verify anonymously

Clone this repository and install the prerequisites for your host. Obtain the
candidate ID and the expected **SHA-256 of `candidate.json`** from the maintainer's
release announcement, then run:

```bash
./scripts/verify.sh candidate-VERSION-RECIPE12-RUN_ID-ATTEMPT SHA_SUM
```

The same command works on supported Linux and macOS hosts without a PAT or GitHub
write access. It checks the manifest checksum and requires the recipe to be an
ancestor of `main`, a `MAJOR.MINOR.x` maintenance branch, or a dated preparation
branch in the official bindings repository. It then checks out that recipe in
a temporary directory, leaving your checkout unchanged.
It downloads the candidate, independently rebuilds every group supported by your
host, and reports verified groups, outstanding groups, and the receipt directory.
It continues after individual build failures and exits unsuccessfully if any
group fails. Successful receipts remain available.

Official branch ancestry establishes repository provenance, not human review. Obtain the
expected manifest checksum from a source you trust and review the identified
recipe before running its build scripts. A checksum copied from the same
download is only a consistency check. Keep a recipe's preparation branch until
the original commit is an ancestor of `main` or a maintenance branch. A squash
merge does not preserve that ancestry, and a candidate tag alone is insufficient.

Linux x86_64 verifies `generated`, `linux-x86_64`, `android`, and `jvm`; Linux
aarch64 verifies `linux-aarch64`; Apple Silicon verifies `darwin`. To select one
group and a persistent result directory, use:

```bash
./scripts/verify.sh CANDIDATE_ID SHA_SUM --group darwin \
  --output /tmp/ldk-node-verification-darwin
```

The lower-level CLI also accepts a public candidate URL or a local candidate
directory. It requires a clean checkout of the exact recipe commit:

```bash
candidate_url=https://github.com/lightningdevkit/ldk-node-bindings/releases/tag/CANDIDATE_ID
env -u GH_TOKEN -u GITHUB_TOKEN nix run .#release -- verify \
  --candidate "$candidate_url" --group linux-x86_64 \
  --output /tmp/ldk-node-verification-linux
```

Repeat for the remaining groups on compatible hosts. Verification downloads
public release assets over HTTPS, rebuilds project outputs, compares the bytes,
and writes a receipt identifying the recipe, inputs, group, files, and hashes.
Verifying a downloaded checksum alone is transport validation; it does not
establish that a build reproduced the candidate.

Send the small receipts to the publishing maintainer. After deliberately
selecting the receipts to trust, collect complete coverage:

```bash
nix run .#release -- collect --candidate "$candidate_url" \
  --receipts /tmp/accepted-receipts --output /tmp/verified-bindings.json
```

The collector checks the actual candidate files against those receipts. Missing
targets, incompatible receipt hosts, different recipes, altered files, or unverified JVM dependencies prevent
collection. See [release operation](docs/releasing.md) for the local publication
commands and recovery rules. Explicit local Swift publication sends the
verified generated wrapper, `Package.swift`, licenses, provenance, and unchanged
XCFramework ZIP to `lightningdevkit/ldk-node-swift`; its version tag belongs to
that distribution repository.

## Verification status

Pinned environments and normalized archives are the basis for reproducible
builds; they do not prove equality by themselves. Every platform requires a
successful CI build and independent local comparison before production use.
See [qualification evidence](docs/qualification.md) for checks actually run and
outstanding platform validation. macOS recipes are supported but still require
execution and comparison on the selected Apple host before publication.
Development checks alone do not imply
that a release has been built, independently reproduced, or published.

The [shared interface](docs/interfaces.md) describes the build and release CLI
contracts. Native source migration, native release tags, and removal of files
from the native repository are separate work.
