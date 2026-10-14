# Darwin UUID diagnostic workflow

Dispatch **Diagnose Darwin UUIDs** (`smoke-verify.yml`) at this branch with
**no inputs**. It runs only in the public `tnull/ldk-node-bindings-test`
repository, on ARM64 `macos-15` with Xcode 16.4 (16F6) and macOS SDK 15.5.

Small C and dependency-free Cargo libraries compare repeated builds, shallow
and nested paths, same-length renamed paths, original and fresh homes,
inherited and clean environments, working directories, output basenames,
and Apple's `-reproducible`, `-S`, and `-oso_prefix` options. Prefix probes
test both canonical `/private/tmp` and lexical `/tmp` spellings separately.
Cargo uses the exact flake `.#rust` compiler shared by the build shell,
without building LDK. Both `release` and `release-smaller` run for ARM64
and x86_64 at their pinned deployment targets, including every linker-option
comparison. The fixture retains
standard-library environment and string handling rather than returning a
constant; its exported function is not executed. Its release profiles
mirror the native package; neither sets `strip`.

The `darwin-uuid-diagnostics` artifact contains original binaries, UUIDs,
load commands, hashes, byte differences, compiler versions, CPU counts,
the linker manual, the Cargo fixture, and `results.tsv`. A small clang
wrapper retains linker arguments, symbols, and libraries before Rust's
post-link debug stripping, allowing comparison with the final libraries.
Inherited environment logging is restricted to `RC_UUID_SALT`, `SOURCE_DATE_EPOCH`,
`ZERO_AR_DATE`, `SDKROOT`, and `DEVELOPER_DIR`.

Diagnostic `.normalized` copies zero the UUID payload and complete code
signature. Their equality does not validate signatures or establish
reproducibility. Original binaries and exact comparisons remain available.
No candidates, publications, or verification receipts are created;
production verification continues to require exact bytes.
