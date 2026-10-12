---
name: release
description: Prepare, independently verify, or locally publish LDK Node Swift, Kotlin JVM, Kotlin Android, and Python bindings from this repository's pinned GitHub CI candidates. Native Rust and crates.io releases remain in the native repository.
---

# Release LDK Node bindings

Use this repository-local skill directly by path; global installation is not
required. The shared entrypoint is `nix run .#release -- COMMAND ...`.
Read [the release procedure](references/procedure.md) for the requested stage.
The commands and host requirements are documented in the repository
[README](../../README.md) and [release guide](../../docs/releasing.md).

## Establish identity and intent

- Identify the requested native revision, bindings version, GitHub `OWNER/REPO`,
  and stage: prepare, verify, collect, or publish. `release.json` and committed
  locks are authoritative. Resolve the requested native release tag to its exact
  commit; use its own API and compatible UniFFI version, not native main.
- Inspect the clean recipe worktree, publication ledger, existing candidates,
  and destination versions before resuming. Use current `YYYY-MM-` names for
  preparation/PR branches; stable backport branches use `MAJOR.MINOR.x`.
  Native tags and crates.io releases are outside this repository's release.
- Keep native source identity distinct from the recipe commit and the final
  SwiftPM metadata commit. Do not move published tags to resolve mismatches.

## Preparation and verification

A request such as "prepare a bindings release for LDK Node VERSION" includes
creating or updating the recipe, running checks, and preparing the CI candidate.
Follow the procedure's recipe-update steps first. For a patch to an older series,
start from that series' previous released recipe or maintenance branch. Keep
the newer recipe on `main` intact and prepare changes in a dated worktree.

**Every new minor or major release must establish its backport branch from the
start:** create `MAJOR.MINOR.x` (for example, `0.8.x`) from the reviewed, committed
new recipe, including the first recipe for a series in this repository. Also
ensure the previous release line has its branch (for example,
`0.7.x`) at that line's last published recipe before advancing `main`. Inspect
local and remote branches first and verify their release line; reuse an existing
branch without resetting or force-pushing it. Publish newly created maintenance branches alongside the
preparation branch when authenticated preparation pushes to GitHub. These stable
names explicitly override the date-prefix convention for maintenance branches
only. Patch-release PRs target their matching backport branch. A branch or
candidate is not evidence that a version was published.

`prepare` may push the reviewed recipe and dispatch/watch GitHub candidate CI
using local `gh` authentication or `GH_TOKEN`. Candidate assets live on a public
GitHub prerelease. Never send the maintainer's PAT, registry credentials, or
signing keys to CI; CI upload uses its own temporary repository token.

Anyone may run `scripts/verify.sh CANDIDATE_ID MANIFEST_SHA256` anonymously on a
compatible host. Share the SHA-256 of `candidate.json` with the candidate ID.
The script selects the exact recipe and the host's supported groups. It independently
rebuilds and compares exact candidate bytes, then emits a receipt. Require every
group, including generated sources and JVM assembly. Darwin receipts establish
the imported Mac native payloads; no manual Mac binary copy is required for
publication. Accept receipts deliberately before `collect`; a matching download
checksum is not evidence of reproduction.

Preparation ends with candidate links and identities, verification commands,
coverage status, checks completed, and any concrete outstanding requirements.
Do not describe a candidate build or a partial set of receipts as a verified
release. Preserve the local ledger so work can resume on the same candidate.

## Explicit local publication

**Final publication requires an explicit local publish request or confirmation
identifying the candidate and destinations.** A request to prepare, verify,
collect, or implement release tooling does not authorize final publication.
When that authorization is absent, finish the reviewable preparation first,
then present the exact candidate, verified hashes, destinations, and command.
Name this instruction if it is why publication is waiting.

`publish --confirm CANDIDATE_ID` runs locally, rechecks verified files, and uploads
those same bytes. Add Maven signatures locally after verification. Use TestPyPI
validation before PyPI promotion. Prepare/review Swift source and archive
checksum metadata before creating its independent bindings version tag.

Keep credentials out of chat, logs, arguments, tracked files, and build inputs.
Use the local environment or credential stores required by the publication tool.
After an uncertain upload outcome, inspect destination state before retrying;
never overwrite package versions or replace candidate assets. Report candidate
preparation, independent verification, staging, and final publication separately.
