# Local release tool

`release` orchestrates public candidates, independent rebuilds, accepted receipts,
and explicit local publication. Run it from the clean bindings recipe checkout:

```sh
nix run .#release -- --help
```

The Rust modules separate input/receipt validation, filesystem and subprocess
operations, candidate verification, CI preparation, and publication. The
repository-level release guide describes the maintainer workflow.

## Anonymous verification

From any checkout containing the release tools, after installing the host
prerequisites in the root README:

```sh
./scripts/verify.sh candidate-VERSION-RECIPE12-RUN_ID-ATTEMPT SHA256
```

`SHA256` is the expected SHA-256 of the exact `candidate.json` file, obtained
from the maintainer's candidate announcement through a trusted channel. The
manifest identifies the complete recipe commit and checksums of every artifact.
The script runs the pinned Nix release tool, downloads the public manifest, and
checks that hash before checking out or executing candidate build code. It
requires no GitHub account, PAT, or repository write access.

The Rust command is `release verify-candidate ID SHA256`. Both commands default
to `lightningdevkit/ldk-node-bindings`. For a candidate in another public
repository, explicitly select the trusted repository:

```sh
./scripts/verify.sh candidate-VERSION-RECIPE12-RUN_ID-ATTEMPT SHA256 --repo OWNER/REPO
```

`--repo` selects both candidate asset downloads and the repository whose branch
history establishes recipe provenance. It is never inferred from the manifest.
The verifier creates a separate checkout at the manifest's exact recipe commit.
It anonymously fetches complete branch history from the selected repository
into a fresh repository, then requires the recipe to be reachable from `main`,
a numeric `MAJOR.MINOR.x` maintenance branch, or a dated
`YYYY-MM-*` preparation branch. Commits found only in another repository or
candidate tags alone are not accepted. Preserve an accepted branch containing
historical recipe commits, including after squash merges. The separately trusted
manifest hash binds the
exact build code and candidate bytes; branch ancestry establishes the accepted
repository provenance. Your
current branch, tracked files, and untracked files are left unchanged. Candidate
assets are downloaded and checked once, then each group is independently rebuilt
from a fresh checkout using the existing verifier.

| Host | Groups rebuilt by default |
| --- | --- |
| x86_64 Linux | generated, linux-x86_64, android, jvm |
| aarch64 Linux | linux-aarch64, jvm |
| Apple Silicon macOS | darwin |

Use `--group GROUP` to verify one of your host's supported groups. The final
report names verified and outstanding groups: one host does not establish full
release coverage. A zero exit status means all selected groups matched; any
download, build, or checksum failure returns a nonzero status. A failed group
does not prevent the remaining selected groups from running. Linux wheel
groups need a working Docker or Podman daemon; Darwin needs the pinned full
Xcode installation. Native Windows and Intel macOS hosts are not supported.

`--output DIR` selects an empty output directory; otherwise the tool creates and
prints one under `/tmp`. It retains the authenticated manifest, candidate files,
and pinned recipe there, with accepted `*.receipt.json` files in `receipts/`.
Build failures also retain the rebuilt and expected files at the path recorded
in `diagnostics/GROUP/failure.json`, separate from the accepted receipts.
Successful receipts can be copied into the
maintainer's collection directory for the existing `release collect` command.
Retain these files until the candidate is accepted, then remove the printed
output directory and any separate failed build workspace when no longer needed.

## Trust and identity

`release.json` declares every output filename. Input identity is SHA-256 over
sorted tracked release.json, dependency lock files, verification-metadata.xml,
and files under locks/. Each record contributes the little-endian u64 length of
its UTF-8 path, the path, the little-endian u64 content length, and the contents.
The exact recipe commit is recorded separately. Inventories and receipts use
schema 1 and strictly checked fields; unknown or duplicate artifact coverage is
rejected. JVM assembly receipts also record the independently built native files.

Candidate assets are flat files. The downloader accepts public GitHub release
URLs, disables curl user configuration, and sends no GitHub credentials. It never
extracts downloaded archives. Publication reads the single expected Swift source
member to stdout; it does not unpack candidate paths onto the filesystem.

Verification makes a fresh checkout of the pinned recipe under `/tmp`, removes
credentials from the build environment, and requests `RELEASE_VERIFY=1`. The
build entrypoint must force every project-generated intermediate to rebuild.
If a build or byte comparison fails, verification preserves its checkout,
rebuilt files, and expected candidate files under `/tmp`. The error identifies
that work directory. The lower-level `verify` command also writes `failure.json`
directly in its output directory; the simple entrypoint's layout is shown above.
Receipts are claims from local independent rebuilds, not signatures or automatic
trust decisions. A maintainer deliberately selects accepted receipts into the
collection directory. Collection rejects host/group combinations that the build
recipes do not support. Keep the resulting verified ledger outside that directory.

## Local state and credentials

Preparation saves `.release/prepare.json` before dispatch and records the exact
run before polling its status through the Actions API every five seconds. This
supports fine-grained PATs with Actions read access for polling; dispatch still
requires Actions write access. It downloads public candidates to `.release/candidates`.
An ambiguous dispatch is reconciled against the recorded pre-dispatch run IDs;
it never silently dispatches a replacement. Preparation cannot publish packages.
Candidate caches are assembled and checked in a sibling temporary directory,
then renamed into place. On resumption, an incomplete older cache is repaired
only when every existing file matches the downloaded candidate.

Publication rejects CI environments and requires `--confirm CANDIDATE_ID`.
It snapshots and rehashes the verified files, then records partial outcomes in
`publication-CANDIDATE_ID.json` alongside the verified ledger. Keep this file for
resuming the same command after interruptions.
An operating-system file lock beside the ledger excludes simultaneous
publication commands using that candidate ledger. It releases automatically
when the process exits, including a crash; the remaining `.lock` file is harmless
and must not be removed while a publisher is running. Use one ledger per candidate.

- GitHub: local `GH_TOKEN` or `gh auth` for prepare and Swift publishing.
- TestPyPI/PyPI: `UV_PUBLISH_TOKEN`, scoped to the selected destination. Production
  requires the identical wheels on TestPyPI and a clean install/import check.
- Maven: `MAVEN_CENTRAL_TOKEN`, base64 of the Portal username and password joined
  by a colon; alternatively `CENTRAL_TOKEN_USERNAME` and `CENTRAL_TOKEN_PASSWORD`.
  `GPG_KEY_ID` optionally selects the local signing key. Tokens enter curl on
  stdin. The Portal flow uploads USER_MANAGED, validates, publishes, then polls.
  An interrupted upload without its returned deployment ID requires recovering
  that ID from the Portal before resuming, preventing a duplicate deployment.
- Swift: `--repo OWNER/REPO` defaults to `lightningdevkit/ldk-node-swift`.
  Prepare a dated branch in that separate distribution repository with only
  generated Swift source, Package.swift, licenses, and release-provenance.json.
  Upload the ZIP to a draft there, create the independent version tag at the
  metadata commit, then publish the draft. Existing tags are never moved. The
  script checks authenticated draft state on every retry, uploads a missing ZIP,
  and rejects an existing ZIP whose bytes differ instead of replacing it. The
  build recipe repository receives no Swift tags or source changes.
  Git hooks are disabled for generated distribution operations. Before pushing
  or resuming, the committed paths, regular-file modes, and exact blob bytes must
  match the verified inputs; a Git filter that changes them causes a failure.
  Local Git authentication, author identity, and signing settings are preserved.
  Swift deployment minima come from the recipe's `deployment_targets`.
  `--swift-git-url` and `--swift-asset-base-url` configure source and asset
  endpoints; the current publication API adapter is GitHub. A future Forgejo
  endpoint requires a Forgejo API adapter before it can replace GitHub uploads.

No credential is included in inventories, receipts, build inputs, or ledgers.
Production publication is not part of the test suite.

## Development checks

```sh
export CARGO_TARGET_DIR=/tmp/cargo-target-bindings-release-dev
cargo fmt --all --check
cargo check --locked
cargo test --locked
```

The integration suite builds temporary Git repositories and calls the real CLI.
It exercises fresh verification, exact coverage, tampering, imported dependencies,
local publication gates, preparation resumption, TestPyPI digest checks, and the
Maven deployment lifecycle. External writes are replaced by subprocess fixtures;
no registry or GitHub upload occurs. Real cross-platform reproduction and live
publication still require the platform environments and explicit maintainer action.
