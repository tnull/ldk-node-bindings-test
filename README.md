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
| `linux-aarch64` | Linux aarch64 + Docker | Python wheel and JVM Linux library |
| `android` | Linux x86_64 | AAR and unsigned Maven publication files |
| `darwin` | Mac arm64 + Xcode | Two Python wheels, two JVM libraries, Swift XCFramework ZIP |
| `jvm` | Linux x86_64 or aarch64 | JAR with all four native libraries and unsigned Maven files |

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
prerelease** after an ARM runner reproduces the JVM package and passes its
native-loading test. Each candidate has a unique tag:

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

To verify Linux artifacts **without installing Nix on the host**, install Git,
tar, and either Podman on Linux or Docker (Docker Desktop on macOS), then run:

```bash
./scripts/verify-containerized.sh CANDIDATE_ID SHA_SUM --output ./verification
# Select Docker explicitly when both engines are installed:
CONTAINER_ENGINE=docker ./scripts/verify-containerized.sh CANDIDATE_ID SHA_SUM
```

The launcher accepts the same `--repo`, `--group`, and `--output` options.
It selects Podman when installed, otherwise Docker. Your user must already be
able to run the selected engine; the script requires neither sudo nor host Nix.
Podman uses a temporary local API service that stops when verification finishes.
Docker must use a local Unix socket; remote engines and Podman Machine are not
supported. On macOS, Docker verifies Linux aarch64 groups on Apple Silicon,
or Linux x86_64 groups on Intel; `darwin` still requires native Nix and Xcode.

The pinned Nix image runs the verifier and shares its store and scratch directory
with sibling manylinux containers. This grants the verifier access to your
container engine. The containers disable SELinux label separation for these
mounts; no host relabeling, Nix daemon, or system configuration changes are made.
Only tracked checkout files are copied into the bootstrap container; candidate
builds still use their exact committed recipes. No host credentials are mounted.

Results, retained failure scratch, and the Nix cache volume name are printed at
startup. With no `--output`, results live under `.release/container-*/results`.
With `--output`, that directory must be empty; scratch is kept beside it in a
`.ldk-verify-*` directory. Receipts remain after the container exits. Failure
diagnostics refer to `/tmp/...` inside the container, corresponding to
`work/...` in the printed scratch directory. Remove the result and scratch
directories when finished; remove the printed cache with `podman volume rm NAME`
or `docker volume rm NAME` when no verification is running. Allow ample disk
space for the Nix store, manylinux images, and independent native builds.

The verifier needs neither a PAT nor GitHub write access. It checks the manifest
checksum and requires the recipe to be an
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
aarch64 verifies `linux-aarch64` and `jvm`; Apple Silicon verifies `darwin`. To select one
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
./scripts/collect.sh --candidate "$candidate_url" \
  --receipts /tmp/accepted-receipts --output /tmp/verified-bindings.json
```

The collector checks the actual candidate files against those receipts. Missing
targets, incompatible receipt hosts, different recipes, altered files, or unverified JVM dependencies prevent
collection. See [release operation](docs/releasing.md) for the local publication
commands and recovery rules. Explicit local Swift publication sends the
verified generated wrapper, `Package.swift`, licenses, provenance, and unchanged
XCFramework ZIP to `lightningdevkit/ldk-node-swift`; its version tag belongs to
that distribution repository.

## Publish locally

Return to the clean recipe worktree used to prepare the candidate before
collecting receipts or publishing. Its commit must match the full `recipe`
recorded in `candidate.json`. The `collect.sh` and `publish.sh` entrypoints use
the pinned Nix release tool and accept its existing options; they preserve
relative paths and do not select a different recipe checkout. Run either with
`--help` for the full interface.

After collecting complete verification coverage, explicitly publish one
destination with credentials supplied through your local environment or
credential store:

```bash
candidate_id=CANDIDATE_ID
candidate_url="https://github.com/lightningdevkit/ldk-node-bindings/releases/tag/$candidate_id"
./scripts/publish.sh --candidate "$candidate_url" \
  --verified /tmp/verified-bindings.json --destination maven \
  --confirm "$candidate_id"
```

Choose `testpypi`, `pypi`, `maven`, or `swift`. PyPI requires the identical wheels
to pass the TestPyPI gate first. Swift defaults to `lightningdevkit/ldk-node-swift`;
`--repo` changes that publication destination. The candidate URL independently
selects the build repository, including a test repository. Publication requires
the verified ledger and exact candidate confirmation, rejects CI, and reuses the
existing publication record when resumed. See [the release guide](docs/releasing.md)
for each destination's credentials, qualification requirements, and recovery.

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
