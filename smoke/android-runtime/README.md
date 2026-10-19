# Android candidate runtime qualification

This disposable test-repository fixture exercises the exact Android publication
from recipe `9a3cd7e5185236b3bb9fdd0f22593f47578d5686`. It belongs only in
`tnull/ldk-node-bindings-test`, not the production recipe. Dispatch the existing
`smoke-verify.yml` workflow on this branch with the candidate tag, reviewed
manifest SHA-256, and `android_only: true`. No registry or GitHub write credential
is used by the tests.

The downloader checks the manifest digest, recipe, version, and each of the five
Android publication file digests and sizes. The app resolves the candidate from
an exclusive local Maven repository, including its published POM and Gradle
module metadata. It does not rebuild the bindings or load them from Maven
Central. The APK check confirms all three packaged `libldk_node.so` files equal
the AAR's exact bytes after extraction. It also rejects platform calls to
`File.toPath` and `Files.createTempDirectory` that would fail on API 21–23
without NIO desugaring.

The minified release application runs a smoke test adapted from
`bindings/kotlin/ldk-node-android/lib/src/androidTest/.../AndroidLibTest.kt`:
create two regtest nodes, start them, read distinct public node IDs, derive
regtest addresses, stop them, and assert that Rust invoked a Kotlin `LogWriter`
callback. `java.nio.file.Files.createTempDirectory` additionally exercises
desugared NIO on the older devices. A small loopback server supplies the empty
regtest fee map required synchronously by `Node.start`; it rejects all chain
endpoints and avoids depending on a public Esplora service. The test runs inside
the minified application; a separate Java-only `com.android.test` module invokes
the kept application entry point through the target application's classloader.
It targets `:app` using the matching release variant, with the debug variant and
core library desugaring disabled in the test module. It has no app or bindings
implementation dependency. This avoids competing Kotlin and desugaring runtimes
inside the instrumentation APK. Its DEX files are checked for bindings, the
application test body, every `j$` class definition, and any class defined by both
APKs. Only the application's smoke entry
point is kept explicitly. All JNA/UniFFI reflection rules must come from the
candidate AAR. APKs, mapping/configuration, test results, logs, image revision,
and observed page size are retained as workflow artifacts.

Both modules use one disposable signing key generated under this fixture's
ignored `build` directory. Changing Android's task-local configuration paths
for the emulator cannot change the signer. The APK guard checks both release
output identities, the test manifest's target package, and matching signatures.
AGP requires minification enabled in the test module to apply the app's mapping;
AGP itself preserves the test driver and runner classes. The application remains
minified with its existing consumer rules. See
[Android's separate test modules](https://developer.android.com/studio/test/advanced-test-setup#use-separate-test-modules-for-instrumented-tests).

After instrumentation, a JDK XML parser requires exactly one successful
`CandidateTest.minifiedCandidateStartsStopsAndCallsBack` result. Missing reports,
zero tests, failure/error/skip results, inconsistent suite counts, duplicates,
and other test identities fail qualification. Old results are removed before
the device run. `check-results-test.sh` exercises positive and negative controls
before the hosted build; a successful Gradle exit alone is insufficient.

## Consumer configuration

The consumer pins AGP 7.4.2, Gradle 7.6 (verified distribution SHA-256), Kotlin
1.6.10, JDK 17, compile SDK/build tools 34/34.0.0, and
`desugar_jdk_libs_nio:2.0.3`. The NIO variant is required by the fixture's
`Files.createTempDirectory` and `File.toPath` calls; the default desugaring
variant does not cover them. This extra NIO usage belongs to the smoke test,
not the generated bindings. Its AndroidX runner/JUnit versions match the
original library's device-test generation. AGP 7.4 is needed because the
recipe's AGP 7.1.2 cannot provide the documented Java 11/NIO desugaring support. This consumer
toolchain is separate from the toolchain that built the AAR. Dependencies come
from Google/Maven Central during this functional test; this is not a hermetic
artifact-reproduction receipt.

Connected tests use AGP 7.4's
[documented built-in runner fallback](https://developer.android.com/studio/releases/past-releases/as-bumblebee-release-notes#disable-new-testing-pipeline).
Its Unified Test Platform aborted on the API 35 image before APK installation
because host-side protobuf classes came from incompatible classloaders. The
fixture disables UTP and checks the actual connected release task's runner
configuration. This check establishes runner selection; device execution and
the exact successful JUnit result remain required.

The consumer explicitly uses compressed native libraries
(`jniLibs.useLegacyPackaging = true`). This is the default for its `minSdk=21`,
and it is the supported packaging configuration for 16 KiB devices with older
AGP. Applications using uncompressed native libraries need AGP 8.5.1 or newer;
older AGP users must select compressed packaging. The fixture also disables
uncompressed native libraries in bundles, although it tests APK installation,
not a Play-generated bundle. See
[Android's packaging requirements](https://developer.android.com/guide/practices/page-sizes#update-packaging)
and [Java API desugaring](https://developer.android.com/studio/write/java8-support).
[Android's desugaring variants](https://android-developers.googleblog.com/2023/02/api-desugaring-supporting-android-13-and-java-nio.html)
describe the additional APIs covered by the NIO dependency.

## Runtime matrix and limits

Each job uses GitHub's Ubuntu 24.04 x86_64 runner with KVM. The workflow
installs `libpulse0`: the emulator executable links to this library
even when launched with `-no-audio`. Its required dependencies are resolved by
APT; no PulseAudio daemon is needed.

AVD creation and launch share explicit task-local Android configuration paths.
Large AVD data stays outside uploaded evidence and is deleted during cleanup.
The runner checks that the emulator can discover the created AVD, detects
process exit while waiting for ADB, and bounds each ADB command (including
diagnostic cleanup). Failed ADB commands report their arguments and exit status.
The smoke runs without a UI and does not depend on unlocking the device.
The 16 KiB job restarts ADB as root, waits for reconnection,
and records UID 0 before disabling and reading back both compatibility
properties. A successful root request with a non-root UID fails qualification.
`check-emulator-test.sh` checks these process boundaries with subprocess
controls; those checks are not device qualification.

| API | Image tag | Revision | Required page size |
| --- | --- | --- | --- |
| 21 | google_apis | 32 | 4096 |
| 22 | google_apis | 26 | 4096 |
| 23 | google_apis | 33 | 4096 |
| 35 | google_apis_ps16k | 5 | 16384 |

These revisions were read from Google's SDK repository. The job refuses a
different image revision and records the installed emulator version. It checks
the booted API and page size and disables page-size compatibility mode for the
16 KiB image. This qualifies actual x86_64 Android execution for this candidate
and consumer configuration. It does not qualify physical ARM devices, arbitrary
consumer shrinker rules, every JNA callback, channel integration with Bitcoin,
or Play's bundle conversion. It is separate from independent reproduction.

For local compilation, install Gradle 7.6, JDK 17, SDK 34/build tools 34.0.0, put
the five candidate files under
`MAVEN/org/lightningdevkit/ldk-node-android/0.7.1/`, and run:

```sh
export LDK_ANDROID_CANDIDATE_REPOSITORY=/absolute/path/to/MAVEN
export LDK_ANDROID_CANDIDATE_VERSION=0.7.1
gradle -p smoke/android-runtime --no-daemon --no-build-cache \
    :app:assembleRelease :instrumentation:assembleRelease
bash smoke/android-runtime/check-apk.sh
```

With a working Android SDK, KVM, and accepted SDK licenses, run one matrix row:

```sh
export ANDROID_RUNTIME_EVIDENCE=/absolute/path/to/evidence
bash smoke/android-runtime/emulator.sh 35 google_apis_ps16k 5 16384
```

An emulator passing with this exact candidate is runtime evidence. Merely
compiling the APK or inspecting its ELF alignment is not.
