# Public throwaway-repository smoke test

`smoke-verify.yml` is test-only infrastructure for the public
`tnull/ldk-node-bindings-test` repository. Keep it off the production PR branch.
It requires the verifier's `--repo OWNER/REPO` option and a completed public
candidate from that same repository.

Dispatch the workflow at the reviewed smoke-test branch with the candidate tag
and the independently recorded SHA-256 of `candidate.json`:

```bash
gh workflow run smoke-verify.yml --repo tnull/ldk-node-bindings-test \
  --ref SMOKE_BRANCH -f candidate_id=CANDIDATE_ID \
  -f manifest_sha256=MANIFEST_SHA256
```

The workflow anonymously fetches its exact commit, then calls
`scripts/verify.sh ID SHA --repo "$GITHUB_REPOSITORY" --group GROUP`.
The verifier authenticates the manifest and fetches its pinned recipe from the
public repository. Downloads and rebuilds receive no GitHub token or PAT, and
the workflow grants no repository permissions. The Nix installer receives a
final empty `access-tokens` setting before starting its daemon; the workflow
also removes the install action's overridden token from the configuration file
and checks that Nix's effective token map is empty without logging its contents.

These builds and rebuilds use the same hosted provider. Their receipts are
smoke-test evidence, not external release qualification or a decision to trust
the candidate for publication.

Each group uses a fresh runner and its own empty verification directory:

| Group | Runner | Required architecture |
| --- | --- | --- |
| `generated` | `ubuntu-24.04` | x86_64 |
| `linux-x86_64` | `ubuntu-24.04` | x86_64 |
| `linux-aarch64` | `ubuntu-24.04-arm` | aarch64 |
| `android` | `ubuntu-24.04` | x86_64 |
| `darwin` | `xcode-27` (macOS 27), Xcode 27.0 (27A266a) | arm64 |
| `jvm` | `ubuntu-24.04` | x86_64 |

JVM verification assembles all four candidate native libraries. Its receipt
records those imports; collecting complete coverage also requires the Linux
x86_64, Linux aarch64, and Darwin receipts. The candidate build workflow
separately checks JVM assembly and native loading on Linux aarch64.

Every verification job attempts to upload `verify-GROUP`, including on failure.
Artifacts retain receipts, the verifier log, candidate metadata, host details,
failure records, generation logs, and final rebuilt output with its hashes.
Build workspaces and caches stay off the artifacts; expected output remains
available from the public candidate. Download evidence within seven days:

```bash
gh run download VERIFICATION_RUN_ID --repo tnull/ldk-node-bindings-test \
  --pattern 'verify-*' --dir /tmp/bindings-smoke-evidence
```

Review the evidence locally, explicitly select one receipt for each of the six
groups, and run the existing `collect` command against the candidate. The
workflow neither selects trusted receipts nor collects or publishes a release.

The throwaway repository also allows manual dispatch of the unchanged Checks
jobs to exercise hosted formatting, Clippy, ShellCheck, and recipe checks
independently of a pull request:

```bash
gh workflow run checks.yml --repo tnull/ldk-node-bindings-test \
  --ref 2026-10-smoke-checks
```

This extra trigger is test infrastructure and stays off the production branch.
