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
the workflow grants no repository permissions.

Each group uses a fresh runner and its own empty verification directory:

| Group | Runner | Required architecture |
| --- | --- | --- |
| `generated` | `ubuntu-24.04` | x86_64 |
| `linux-x86_64` | `ubuntu-24.04` | x86_64 |
| `linux-aarch64` | `ubuntu-24.04-arm` | aarch64 |
| `android` | `ubuntu-24.04` | x86_64 |
| `darwin` | `macos-15`, Xcode 16.4 (16F6) | arm64 |
| `jvm` | `ubuntu-24.04` | x86_64 |

JVM verification assembles all four candidate native libraries. Its receipt
records those imports; collecting complete coverage also requires the Linux
x86_64, Linux aarch64, and Darwin receipts. The candidate build workflow
separately checks JVM assembly and native loading on Linux aarch64.

Every verification job attempts to upload `verify-GROUP`, including on failure.
Artifacts retain receipts, the verifier log, candidate metadata, host details,
failure records, generation logs, and hashes of any rebuilt output. Build
workspaces and caches stay off the artifacts. Download evidence within seven
days:

```bash
gh run download VERIFICATION_RUN_ID --repo tnull/ldk-node-bindings-test \
  --pattern 'verify-*' --dir /tmp/bindings-smoke-evidence
```

Review the evidence locally, explicitly select one receipt for each of the six
groups, and run the existing `collect` command against the candidate. The
workflow neither selects trusted receipts nor collects or publishes a release.
