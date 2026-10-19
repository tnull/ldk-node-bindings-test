# Initial qualification

Qualification distinguishes repeatable bytes, working consumers, and hosted
release operation. A passing check in one category does not establish another.
The initial input is native v0.7.1, commit
`edc8916e289d39a4135b6eda66978a9f765396bb`, with UniFFI 0.28.3 and Rust 1.95.0.

## Local evidence

Development checks on 2026-10-12 established the following results:

| Check | Result |
| --- | --- |
| Native locked Cargo check with the UniFFI feature | Passed |
| Release CLI | 28 unit/integration tests passed, including inside its Nix package build |
| Nix configuration | All declared host systems evaluated successfully |
| Generated-source verification | Real Nix build and anonymous verifier entrypoint passed in a local integration fixture |
| Two fresh Linux x86_64 native builds, `release` | Identical complete library bytes |
| Two fresh Linux x86_64 native builds, `release-smaller` | Identical complete library bytes |
| Two fresh Linux x86_64 Python wheel assemblies | Identical complete wheel bytes; Twine metadata checks passed |
| Python 3.10 consumer in manylinux | Offline installation, import, `default_config()`, and `Builder()` passed |
| Linux wheel ABI inspection | Auditwheel confirmed `manylinux_2_28_x86_64` compatibility |
| JVM consumer | Native smoke test passed using the compiled Linux library |
| JVM unsigned publication | Pure Nix build/rebuild matched; this packaging fixture contained only the Linux native payload |
| Android native outputs | Three supported ABIs compiled and passed 16 KiB alignment checks |
| Android unsigned publication | Pure Nix build/rebuild matched all five artifacts, including the final AAR; imported device tests compiled |
| Android JNA dependency | Version 5.17.0 native libraries passed 16 KiB alignment checks for all three supported ABIs |

The Linux experiments used the exact locked manylinux image and Nix-provided
Rust toolchain and vendored dependencies. Each pair used separate source and
target directories with no project build cache. This development host cannot
run nested Docker/Podman containers, so the experiments executed the unpacked
image with Bubblewrap and networking disabled. This is evidence for the native
and wheel recipes; it is not a successful Docker entrypoint or GitHub CI run.

Observed SHA-256 values for the original recipe (before the PR review fixes):

| Artifact | SHA-256 |
| --- | --- |
| Linux JVM library (`release`) | `497270d468e66232fc2fa163dc1ab435dd6b7d105eddfecf318a9f8a90fd6e1d` |
| Linux Python library (`release-smaller`) | `abccd0647ba4998d711b67cd9c9970cb22baf0b705b47d478817f01bcf78d367` |
| `ldk_node-0.7.1-py3-none-manylinux_2_28_x86_64.whl` | `14c9fc436941c87104d7f75972c2e7fd444e70ed903af3acee3910db4f6b130a` |

These development results do not constitute release receipts. Release acceptance
uses the exact candidate recipe, public candidate manifest, and independent
`verify` and `collect` commands.

## External review evidence

The [PR 2 review](https://github.com/lightningdevkit/ldk-node-bindings/pull/2#pullrequestreview-5495085839)
reports matching Linux x86_64 bytes through Docker on a separate machine, two
matching Android builds, and two matching complete JVM assemblies. It also
reports matching Darwin builds and passing Swift/Python consumers after local
fixes, using Xcode 26.2 with relaxed pins. These are reviewer-reported results;
they do not qualify the current Xcode 27.0 recipe or the subsequent corrections.

The review corrections change recipe inputs, including generated Android
configuration and native build flags. Historical hashes above describe the
earlier experiments; independently compare fresh candidate bytes for the updated
recipe. Independent reproduction outside GitHub remains required.

## Hosted smoke test and Apple toolchain update

The throwaway repository's [candidate build](https://github.com/tnull/ldk-node-bindings-test/actions/runs/38591238295)
and [anonymous verification run](https://github.com/tnull/ldk-node-bindings-test/actions/runs/38600623779)
passed for all six artifact groups, including Linux aarch64 and Darwin. The
Darwin checks included Swift and Python consumers. Both runs used GitHub runners
and Xcode 16.4; they establish hosted operation, not independent external trust.

The recipe now selects macOS 27's `xcode-27` runner, Xcode 27.0 build `27A266a`,
and SDKs 27.0. Python and JVM native deployment targets move to macOS 12.0 on
both architectures, replacing the earlier Intel 10.12 and ARM 11.0 targets.
The current-toolchain results below supersede the earlier hosted checks for
compilation and byte equality. Execution on the declared minimum macOS version
still needs qualification.

## Current candidate qualification

On 2026-10-19, the throwaway repository built
[`candidate-0.7.1-9a3cd7e51852-39256421271-1`](https://github.com/tnull/ldk-node-bindings-test/releases/tag/candidate-0.7.1-9a3cd7e51852-39256421271-1)
from recipe `9a3cd7e5185236b3bb9fdd0f22593f47578d5686`. Its manifest SHA-256 is
`970fd1011bb177cbe39742700cbaf3cd3448bb4880f23eba926b6ed175bf9fe1`.
Subsequent changes to the production branch only document Android consumer
packaging requirements and these results; executable recipe inputs are unchanged.

| Check | Result |
| --- | --- |
| [Candidate build](https://github.com/tnull/ldk-node-bindings-test/actions/runs/39256421271) | All artifact groups passed |
| [Anonymous reproduction](https://github.com/tnull/ldk-node-bindings-test/actions/runs/39264255547) | All six reproduction jobs passed, including complete unsigned packages and the Swift ZIP |
| Darwin consumers | Swift package build and Python wheel installation/import passed with Xcode 27.0 |
| Apple deployment metadata | Both architectures' JVM and Python libraries report SDK 27.0 and minimum macOS 12.0 |
| [Container verification](https://github.com/tnull/ldk-node-bindings-test/actions/runs/39264259431) | Docker and Podman on Linux x86_64 passed generated, Linux x86_64, Android, and JVM verification; Docker on Linux aarch64 passed Linux aarch64 and JVM verification |
| Local anonymous verification | Generated sources, all five Android publication files, and all five JVM publication files matched |
| Local Linux x86_64 reproduction | Fresh native builds and wheel assembly matched both complete candidate files outside GitHub; Python 3.10 installation/import and Auditwheel passed |
| [Android runtime qualification](https://github.com/tnull/ldk-node-bindings-test/actions/runs/39282458950) | Minified consumer passed on x86_64 emulators for API 21, 22, 23, and API 35 with 16 KiB pages |
| Collection | Missing Darwin coverage was rejected; all six selected receipts produced a verified ledger |
| Release tooling checks | All 54 tests, formatting, Clippy, and ShellCheck passed |

The reproduction run also contains failed Android emulator jobs from an earlier
test harness. Those failures do not invalidate its six successful reproduction
jobs; runtime qualification is a separate check. Hosted reproduction and container
runs establish operation on GitHub, not independent trust outside GitHub.
Collection used local generated/Android/JVM receipts and hosted Linux/Darwin
receipts; it exercises the coverage gate without granting publication approval.
The deployment metadata check does not establish execution on macOS 12.0.

The current Linux x86_64 reproduction used the locked manylinux image's verified
layers, freshly unpacked for network-disabled Bubblewrap execution. Both Rust
profiles compiled in fresh source and target directories, and the current recipe
assembled the wheel. Full-file comparisons matched the candidate JVM library
(`497270d468e66232fc2fa163dc1ab435dd6b7d105eddfecf318a9f8a90fd6e1d`)
and wheel (`5900da4d015262098f630d5f0789913635e2abd6a0e3361a63cf541489f8312b`).
This establishes byte equality outside GitHub; it does not produce a receipt
from the public Docker/Podman verifier entrypoint.

Each Android device report records exactly one successful
`CandidateTest.minifiedCandidateStartsStopsAndCallsBack` execution. The test
starts and stops two regtest nodes, invokes generated APIs, and checks native
logging callbacks through the minified Kotlin consumer. A local fee-estimates
endpoint supplies startup data; full channel integration is separate coverage.
The API 35 device reports 16384-byte pages, with
`bionic.linker.16kb.app_compat.enabled=false` and
`pm.16kb.app_compat.disabled=true`. All five Android publication files match the
candidate manifest, and all three packaged native ABI payloads match the AAR.
Device execution covered x86_64.

The test-only harness uses a separate Java instrumentation module and AGP 7.4's
documented built-in runner fallback. Its earlier failures came from duplicate
instrumentation runtimes and host-side test-launcher setup. The final test
reports and native callback logs establish runtime success after those repairs;
the candidate AAR and its native payloads are unchanged.

## Remaining release qualification

Before production publication, independently reproduce every group on its
required host. The current candidate still needs native Linux x86_64, Linux
aarch64, and Darwin receipts from outside GitHub; its other groups have local
reproduction evidence. Exercise consumer installation separately from checksum
comparison. Live TestPyPI staging and Maven publication have not been exercised;
registry credentials and namespace ownership remain unqualified. TestPyPI
staging must pass before PyPI publication.

Repeat Android consumer qualification for each candidate, including a 16 KiB
device and API 21–23 devices with the documented desugaring configuration.
Compilation and ELF alignment checks do not establish device compatibility; see
[the Android consumer requirements](releasing.md#consumer-qualification).

Service-contract tests use mocks and local Git repositories. They do not prove
registry credentials, namespace ownership, live upload behavior, or GitHub
runner provisioning. Production publication remains an explicit local action.

## Native repository cleanup gate

The cleanup branch in `ldk-node` is deliberately separate from this repository's
implementation. Merge it only after the replacement build and verification
workflow has been qualified across all distributions.

The prepared branch is `2026-10-remove-migrated-bindings` at `c8cffd84`, based on native
`2026-10-release-skill` at `16d6bb51`. It removes the former language projects,
generators, configurations, workflows, and publication scripts, and narrows the
native release skill to Rust and source handoff. It leaves native Rust, the UDL,
UniFFI scaffolding, Cargo configuration, native tests, and remaining CI unchanged.

The native development branch currently uses version `0.8.0+git` and UniFFI
0.29.5, with language tests newer than those imported from v0.7.1. Qualifying
the initial v0.7.1 recipe alone does not replace that development coverage.
Before removing native language CI and its tests, support the current native
API and those tests in this repository, or retain equivalent coverage in the
native repository. Preserve native code, the authoritative UDL, and the UniFFI
scaffolding interface throughout the migration.

Before preparing that cleanup, an isolated snapshot of native commit `16d6bb51`
passed formatting, default and UniFFI Cargo checks, 201 default unit tests,
203 UniFFI unit tests, and a package inventory check. Daemon integration tests
were not run. The ignored native lockfile needed resolution in the disposable
snapshot; the original checkout and lockfile were left unchanged.

The prepared cleanup also passed formatting, default and UniFFI Cargo checks,
201 default unit tests, 203 UniFFI unit tests, and clean-commit package inventory
using that resolved lock.
Independent correctness, architecture, and simplification reviews accepted the
deletion boundary and deferred-merge requirements. These checks validate the
retained native build and tested behavior; they do not waive the migration gates.
