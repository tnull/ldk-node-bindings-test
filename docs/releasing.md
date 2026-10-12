# Local release operation

The [release skill](../skills/release/SKILL.md) orchestrates the same CLI that a
maintainer can run directly. Preparation and verification do not authorize
production publication. A candidate lives as flat assets on a public GitHub
prerelease in `lightningdevkit/ldk-node-bindings` until the maintainer explicitly
publishes it locally. `lightningdevkit/ldk-node-swift` is the separate SwiftPM
publication destination; it does not run the bindings candidate build pipeline.

## Release versions and branches

Ask the release skill to prepare bindings for a native version. It resolves the
native tag to an exact commit and prepares a dated recipe branch/worktree with
the new `release.json`, native source and UDL hashes, dependency locks, and any
required bindings or consumer-test changes. The low-level `prepare` command
builds an already committed recipe; the skill performs the preceding update.

Normally, a new minor release is reviewed in a PR to `main`. A later patch for
an older series starts from that series' previous released recipe, identified
by its candidate manifest, even if `main` has advanced. Carry over relevant
tooling fixes deliberately; do not roll `main` back or silently adopt the newer
series' native API or dependency set.

When preparing a new minor or major release, the skill creates its stable
`MAJOR.MINOR.x` backport branch from the reviewed, committed recipe, such as
`0.8.x` for `0.8.0`. Before advancing `main`, it also ensures the previous line
has its branch, such as `0.7.x`, based on that line's last published recipe.
Existing branches are preserved; never reset or force-push one to another line.
These stable maintenance names are an explicit exception to the `YYYY-MM-`
convention. Preparation and PR branches still use the current date prefix.
Patch-release PRs target the matching maintenance branch. Creating a branch
records a recipe line; it does not establish that its candidate was published.
Keep each recipe reachable from the official repository's `main`, numeric
`MAJOR.MINOR.x`, or dated `YYYY-MM-*` preparation branch. After a squash merge,
retain the unsquashed preparation branch unless the original recipe remains
an ancestor of a retained main or maintenance branch. Candidate tags alone do
not establish the official branch ancestry required by anonymous verification.
Each candidate records its full recipe commit and immutable input
identity; verification does not depend on the current contents of `main`.

Use a separate worktree for each preparation, retaining its local `.release`
ledger. The candidate workflow must exist on the default branch, but dispatch
uses the selected dated recipe branch. Publication always needs a separate
explicit local request for the verified candidate and its destinations.

## Preparation state

```bash
nix run .#release -- prepare --repo lightningdevkit/ldk-node-bindings
```

Run from a clean, committed recipe branch with a current `YYYY-MM-` prefix.
Use local `gh` authentication or `GH_TOKEN` with contents and Actions write
access. The command records its resumable state in `.release/prepare.json` and
downloads candidates into `.release/candidates/CANDIDATE_ID`. Retain this local
state when continuing a release; do not commit it or its downloaded artifacts.

The dispatch input is the full recipe commit SHA and must equal the workflow's
checked-out identity. The candidate workflow uses GitHub's temporary repository
token to create the public prerelease. It never receives the maintainer's PAT,
package registry tokens, or signing keys.

Candidate release assets are the durable public handoff. GitHub Actions
artifacts are intermediate job transport with seven-day retention; they are not
the verifier's download source. Rerunning a job may replace that run's transport
artifact, but each published candidate tag includes the run attempt and is never
reused to replace already-public candidate files. A partially uploaded candidate
is not publishable; create a new attempt after diagnosing the failure.

## Complete verification

Use the README's `verify` commands on the appropriate hosts and accept receipts
only from rebuilds you intend to trust. Collect all six groups:

```bash
nix run .#release -- collect --candidate "$candidate_url" \
  --receipts /tmp/accepted-receipts --output /tmp/verified-bindings.json
```

The verified ledger binds the candidate ID, recipe, input identity, files, and
receipt coverage. JVM dependencies also need their Linux and Darwin receipts.
The complete Swift ZIP is a comparison target because SwiftPM checksums archive
bytes, including container metadata.

## Publication commands

The publisher requires a specific candidate confirmation and rejects CI
environments. It rechecks all candidate files against the verified ledger
immediately before upload. Supply credentials locally through the indicated
environment or credential store; do not place credential values in commands or
logs. All destinations use existing verified candidate bytes.

| Destination | Local authentication | Result |
| --- | --- | --- |
| `testpypi` | `UV_PUBLISH_TOKEN` for TestPyPI | Stage the four verified wheels |
| `pypi` | `UV_PUBLISH_TOKEN` for PyPI | Publish wheels after the TestPyPI gate |
| `maven` | `MAVEN_CENTRAL_TOKEN`, local GPG key | Sign, submit, validate, and publish the Maven bundle through the Central Portal API |
| `swift` | `gh` authentication or `GH_TOKEN` for the Swift destination | Publish source, metadata, a version tag, and the verified ZIP to `ldk-node-swift` |

Example for an explicitly authorized TestPyPI upload:

```bash
nix run .#release -- publish --verified /tmp/verified-bindings.json \
  --candidate "$candidate_url" --destination testpypi \
  --confirm "$candidate_id"
```

Production PyPI uses the same command with `--destination pypi` and a separate
PyPI token. The publisher checks TestPyPI's recorded digests against every
candidate wheel and performs a fresh installation/import check before production
promotion. That installation uses only TestPyPI, requires a candidate wheel hash,
and ignores local uv configuration, extra indexes, caches, and Python import
paths. Do not rebuild or rename wheels between staging and promotion.

For Maven, `MAVEN_CENTRAL_TOKEN` is the Base64 encoding of the Central Portal
token's `username:password`; keep it in the local environment. `GPG_KEY_ID` can
select the local signing key. Signature bytes are produced only after unsigned
primary artifacts match the ledger. The local script submits a `USER_MANAGED`
deployment, records its deployment ID, polls validation, and invokes the Central
Portal publish API under the explicit candidate confirmation. No browser upload
or separate portal click is required. Verify the final API status before
reporting Maven completion; resume an existing deployment after an interruption
instead of uploading a replacement bundle.

Swift publication uses a separate destination repository. Review the verified
generated Swift wrapper, final archive URL/checksum, licenses, and provenance
before authorizing publication. The local publisher creates their distribution
commit and an independent `vVERSION` tag in `lightningdevkit/ldk-node-swift`, then
publishes the unchanged verified XCFramework ZIP as that release's asset.
The distribution contains `Sources/LDKNode`, `Package.swift`, `LICENSE-APACHE`,
`LICENSE-MIT`, and `release-provenance.json`. Provenance identifies the native
source, bindings recipe, candidate, input identity, and verified archive hash.
Consumers use the Swift repository's SwiftPM URL and version tag; the native
source tag and candidate prerelease tag have distinct roles.

```bash
nix run .#release -- publish --verified /tmp/verified-bindings.json \
  --candidate "$candidate_url" --destination swift \
  --repo lightningdevkit/ldk-node-swift --confirm "$candidate_id"
```

The publisher clones the distribution repository, or initializes its empty Git
remote, and pushes a dated metadata branch and immutable version tag. It uploads
the verified archive to a draft destination release before publishing that
release; inspect its recorded state before retrying an interrupted publication.

`--repo` selects the Swift GitHub destination and defaults to
`lightningdevkit/ldk-node-swift` for Swift publication. Optional `--swift-git-url`
and `--swift-asset-base-url` configure the Git remote and public archive URL
prefix. The current publication adapter uses GitHub's release API; using Forgejo
for Swift publication requires an adapter for its release API as well as changing
the Git destination. The native source URL is independently configurable in the
release inputs.

## Consumer qualification

`NativeSmokeTest` verifies JNI/JNA loading and a generated API call without a
Bitcoin backend. The JVM/Python channel integration tests require bitcoind and
electrs. Android device tests require an emulator or device; compiling them is
not a runtime test. Swift's package is compiled against the built framework on
the Mac build host. Keep these functional checks separate from byte equality.

Android uses JNA 5.17.0 for its 16 KiB page fixes. Both JNA's native payload
and the packaged LDK libraries are checked for compatible ELF alignment.
Applications supporting older Android releases need Java 8 library desugaring.
For example, an application using AGP 7.4 or newer can configure:

```kotlin
android {
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions { jvmTarget = "1.8" }
}
dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.0.3")
}
```

See [Android's Java 8 API support](https://developer.android.com/studio/write/java8-support)
for the desugaring version compatible with the application's AGP version.
[JNA's Android 6 issue](https://github.com/java-native-access/jna/issues/1662)
remains open; enabling desugaring is not proof that every consumer is compatible.
Runtime tests on API 21–23 and a 16 KiB device remain required release
qualification checks. ELF inspection does not replace those tests.

## Resume and inspect

Publication state is recorded next to the verified ledger as
`publication-CANDIDATE_ID.json`. Keep the ledger, accepted receipts, and
publication record together. A failed or interrupted upload may already have
changed the destination; inspect remote files/hashes and recorded status before
retrying. Never replace an existing registry version, move a published tag, or
upload newly built bytes under an old verified ledger.

Report exactly which destinations were staged, submitted, validated, or finally
published. Candidate preparation, a successful hash comparison, and a TestPyPI
upload each establish a different part of the release.
