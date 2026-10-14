use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS");
    command
}

#[test]
fn help_is_plain_and_available_outside_a_recipe() {
    let directory = tempfile::tempdir().unwrap();
    let result = command(env!("CARGO_BIN_EXE_release"))
        .current_dir(directory.path())
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .arg("--help")
        .output()
        .unwrap();
    assert!(result.status.success());
    let help = String::from_utf8(result.stdout).unwrap();
    assert!(help.contains("Usage:"));
    assert!(help.contains("verify"));
    assert!(!help.contains('\u{1b}'), "help must be plain text");
}

struct Repo {
    dir: TempDir,
    candidate: TempDir,
    sha: String,
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(env!("CARGO_BIN_EXE_release"))
        .current_dir(dir)
        .args(args)
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .output()
        .unwrap()
}
fn git(dir: &Path, args: &[&str]) -> String {
    let out = command("git").current_dir(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn ok(out: Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn fail(out: Output, needle: &str) {
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(needle),
        "expected {needle}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn fixture() -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let candidate = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "2026-10-test"]);
    fs::write(dir.path().join(".gitignore"), "/.release/\n").unwrap();
    let groups = json!({"generated":["generated-sources.tar"],"linux-x86_64":["jvm-linux-x86_64.so","linux.whl"],"linux-aarch64":["arm.whl"],"android":["ldk-node-android-0.7.1.aar"],"darwin":["jvm-darwin-x86_64.dylib","jvm-darwin-aarch64.dylib","mac.whl","LDKNodeFFI.xcframework.zip"],"jvm":["ldk-node-jvm-0.7.1.jar"]});
    write(
        &dir.path().join("release.json"),
        &json!({"schema":1,"bindings_version":"0.7.1","native":{"rev":"edc8916e289d39a4135b6eda66978a9f765396bb"},"groups":groups,"deployment_targets":{"swift_macos":"13.2","swift_ios":"16.1"}}),
    );
    fs::write(dir.path().join("Cargo.lock"), "pinned").unwrap();
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "Fixture",
        ],
    );
    let sha = git(dir.path(), &["rev-parse", "HEAD"]);
    for names in groups.as_object().unwrap().values() {
        for name in names.as_array().unwrap() {
            fs::write(
                candidate.path().join(name.as_str().unwrap()),
                format!("payload {name}"),
            )
            .unwrap();
        }
    }
    Repo {
        dir,
        candidate,
        sha,
    }
}
fn build_candidate(r: &Repo) -> Value {
    let cfg: Value =
        serde_json::from_slice(&fs::read(r.dir.path().join("release.json")).unwrap()).unwrap();
    for (group, names) in cfg["groups"].as_object().unwrap() {
        let staging = tempfile::tempdir().unwrap();
        for name in names.as_array().unwrap() {
            let name = name.as_str().unwrap();
            fs::copy(r.candidate.path().join(name), staging.path().join(name)).unwrap();
        }
        ok(run(
            r.dir.path(),
            &[
                "inventory",
                "--dir",
                staging.path().to_str().unwrap(),
                "--recipe",
                &r.sha,
                "--group",
                group,
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--output",
                r.candidate
                    .path()
                    .join(format!("{group}.inventory.json"))
                    .to_str()
                    .unwrap(),
            ],
        ));
    }
    ok(run(
        r.dir.path(),
        &[
            "candidate",
            "--dir",
            r.candidate.path().to_str().unwrap(),
            "--recipe",
            &r.sha,
            "--run-id",
            "123",
            "--attempt",
            "1",
            "--output",
            r.candidate.path().join("candidate.json").to_str().unwrap(),
        ],
    ));
    serde_json::from_slice(&fs::read(r.candidate.path().join("candidate.json")).unwrap()).unwrap()
}
fn receipts(r: &Repo, c: &Value) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (group, inv) in c["groups"].as_object().unwrap() {
        let host = match group.as_str() {
            "darwin" => "macos-aarch64",
            "linux-aarch64" => "linux-aarch64",
            _ => "linux-x86_64",
        };
        write(
            &dir.path().join(format!("{group}.json")),
            &json!({"schema":1,"candidate_id":c["id"],"recipe":c["recipe"],"inputs_sha256":c["inputs_sha256"],"group":group,"files":inv["files"],"imports":inv["imports"],"method":"independent-rebuild","host":host}),
        );
    }
    assert!(r.candidate.path().is_dir());
    dir
}
#[test]
fn candidate_and_complete_receipts_produce_ledger() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let verification = tempfile::tempdir().unwrap();
    let output = verification.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
    ));
    let ledger: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    fs::remove_file(output).unwrap();
    assert_eq!(ledger["candidate_id"], c["id"]);
    assert_eq!(ledger["receipts"].as_array().unwrap().len(), 6);
}

#[test]
fn collect_rejects_receipt_from_an_incompatible_host() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let path = accepted.path().join("darwin.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt["host"] = json!("linux-x86_64");
    write(&path, &receipt);
    let output = tempfile::tempdir().unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "collect",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--receipts",
                accepted.path().to_str().unwrap(),
                "--output",
                output.path().join("verified.json").to_str().unwrap(),
            ],
        ),
        "receipt host cannot build group",
    );
}
#[test]
fn collector_rejects_missing_receipts_and_changed_bytes() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let out = accepted.path().join("ledger");
    fs::remove_file(accepted.path().join("darwin.json")).unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "collect",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--receipts",
                accepted.path().to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
            ],
        ),
        "missing receipt",
    );
    fs::write(r.candidate.path().join("linux.whl"), "tampered").unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "collect",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--receipts",
                accepted.path().to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
            ],
        ),
        "artifact differs",
    );
}
#[test]
fn collector_rejects_foreign_identity_and_jvm_imports() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let out = accepted.path().join("ledger");
    let file = accepted.path().join("jvm.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    receipt["imports"][0]["sha256"] = json!("0".repeat(64));
    write(&file, &receipt);
    fail(
        run(
            r.dir.path(),
            &[
                "collect",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--receipts",
                accepted.path().to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
            ],
        ),
        "JVM imports",
    );
    receipt["recipe"] = json!("f".repeat(40));
    write(&file, &receipt);
    fail(
        run(
            r.dir.path(),
            &[
                "collect",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--receipts",
                accepted.path().to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
            ],
        ),
        "receipt identity",
    );
}
#[test]
fn inventory_rejects_extra_files_and_symlinks() {
    let r = fixture();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("generated-sources.tar"), "x").unwrap();
    fs::write(dir.path().join("extra"), "x").unwrap();
    let args = [
        "inventory",
        "--dir",
        dir.path().to_str().unwrap(),
        "--recipe",
        &r.sha,
        "--group",
        "generated",
        "--output",
        r.candidate
            .path()
            .join("generated.inventory.json")
            .to_str()
            .unwrap(),
    ]
    .map(String::from);
    fail(
        run(
            r.dir.path(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        ),
        "unexpected file",
    );
    fs::remove_file(dir.path().join("extra")).unwrap();
    fs::remove_file(dir.path().join("generated-sources.tar")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            r.dir.path().join("release.json"),
            dir.path().join("generated-sources.tar"),
        )
        .unwrap();
        fail(
            run(
                r.dir.path(),
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            "regular file",
        );
    }
}
#[test]
fn publish_requires_local_explicit_candidate_confirmation() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    fail(
        run(
            r.dir.path(),
            &[
                "publish",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--verified",
                ledger.to_str().unwrap(),
                "--destination",
                "pypi",
                "--confirm",
                "wrong",
            ],
        ),
        "confirmation",
    );
    let output = command(env!("CARGO_BIN_EXE_release"))
        .current_dir(r.dir.path())
        .args([
            "publish",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--verified",
            ledger.to_str().unwrap(),
            "--destination",
            "pypi",
            "--confirm",
            c["id"].as_str().unwrap(),
        ])
        .env("CI", "true")
        .output()
        .unwrap();
    fail(output, "local machine");
}
#[test]
fn verify_requires_clean_exact_recipe() {
    let r = fixture();
    build_candidate(&r);
    fs::write(r.dir.path().join("Cargo.lock"), "changed").unwrap();
    let output = tempfile::tempdir().unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "verify",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--group",
                "generated",
                "--output",
                output.path().to_str().unwrap(),
            ],
        ),
        "clean",
    );
}

#[cfg(unix)]
fn executable(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[test]
#[cfg(unix)]
fn pypi_requires_identical_testpypi_files_before_upload() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    let bin = tempfile::tempdir().unwrap();
    executable(
        &bin.path().join("curl"),
        "#!/bin/sh\nprintf '{\"urls\":[]}\\n200'\n",
    );
    executable(
        &bin.path().join("uv"),
        "#!/bin/sh\necho 'MUST NOT UPLOAD' >&2\nexit 81\n",
    );
    let out = command(env!("CARGO_BIN_EXE_release"))
        .current_dir(r.dir.path())
        .args([
            "publish",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--verified",
            ledger.to_str().unwrap(),
            "--destination",
            "pypi",
            "--confirm",
            c["id"].as_str().unwrap(),
        ])
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .output()
        .unwrap();
    fail(
        out,
        "TestPyPI does not contain every identical candidate wheel",
    );
}
#[test]
#[cfg(unix)]
fn verify_rebuilds_fresh_without_secrets_and_keeps_host_selectors() {
    let mut r = fixture();
    fs::create_dir(r.dir.path().join("scripts")).unwrap();
    executable(&r.dir.path().join("scripts/build.sh"),"#!/bin/sh\n[ \"$RELEASE_VERIFY\" = 1 ] || exit 91\n[ -z \"$GH_TOKEN$UV_PUBLISH_TOKEN$CENTRAL_TOKEN_PASSWORD\" ] || exit 92\n[ ! -e \"$CARGO_TARGET_DIR\" ] || exit 93\n[ \"$CONTAINER_ENGINE\" = podman ] || exit 94\n[ \"$DEVELOPER_DIR\" = /test-xcode ] || exit 95\n[ \"$DOCKER_HOST\" = unix:///test-socket ] && [ \"$DOCKER_CONTEXT\" = test-context ] && [ \"$XDG_RUNTIME_DIR\" = /test-runtime ] || { echo 'container transport selectors missing' >&2; exit 96; }\nmkdir -p \"$4\"\nprintf 'payload \"generated-sources.tar\"' > \"$4/generated-sources.tar\"\n");
    git(r.dir.path(), &["add", "."]);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "Build fixture",
        ],
    );
    r.sha = git(r.dir.path(), &["rev-parse", "HEAD"]);
    build_candidate(&r);
    let output = tempfile::tempdir().unwrap();
    ok(command(env!("CARGO_BIN_EXE_release"))
        .current_dir(r.dir.path())
        .args([
            "verify",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--group",
            "generated",
            "--output",
            output.path().to_str().unwrap(),
        ])
        .env("CONTAINER_ENGINE", "podman")
        .env("DEVELOPER_DIR", "/test-xcode")
        .env("DOCKER_HOST", "unix:///test-socket")
        .env("DOCKER_CONTEXT", "test-context")
        .env("XDG_RUNTIME_DIR", "/test-runtime")
        .env("GH_TOKEN", "dummy")
        .env("UV_PUBLISH_TOKEN", "dummy")
        .env("CENTRAL_TOKEN_PASSWORD", "dummy")
        .output()
        .unwrap());
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.path().join("generated.receipt.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["method"], "independent-rebuild");
}

#[test]
#[cfg(unix)]
fn maven_publishes_validated_deployment_and_resumes_by_id() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    let bin = tempfile::tempdir().unwrap();
    executable(
        &bin.path().join("curl"),
        r##"#!/bin/sh
for arg do last="$arg"; done
case "$last" in
  https://repo1.maven.org/*) printf '\n404';;
  *'/upload?'*) cat >/dev/null; printf 'upload\n' >> "$MOCK_LOG"; printf 'deployment-123';;
  *'/status?'*) cat >/dev/null; if [ -e "$MOCK_PUBLISHED" ]; then printf '{"deploymentState":"PUBLISHED"}'; else printf '{"deploymentState":"VALIDATED"}'; fi;;
  *'/deployment/'*) cat >/dev/null; printf 'publish\n' >> "$MOCK_LOG"; touch "$MOCK_PUBLISHED";;
  *) exit 99;;
esac
"##,
    );
    executable(
        &bin.path().join("gpg"),
        r##"#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then shift; result="$1"; fi
  shift
done
printf 'test signature' > "$result"
"##,
    );
    let invoke = || {
        command(env!("CARGO_BIN_EXE_release"))
            .current_dir(r.dir.path())
            .args([
                "publish",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--verified",
                ledger.to_str().unwrap(),
                "--destination",
                "maven",
                "--confirm",
                c["id"].as_str().unwrap(),
            ])
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("MAVEN_CENTRAL_TOKEN", "dGVzdDp0ZXN0")
            .env("MOCK_LOG", bin.path().join("events"))
            .env("MOCK_PUBLISHED", bin.path().join("published"))
            .output()
            .unwrap()
    };
    ok(invoke());
    ok(invoke());
    assert_eq!(
        fs::read_to_string(bin.path().join("events")).unwrap(),
        "upload\npublish\n"
    );
    let publication: Value = serde_json::from_slice(
        &fs::read(
            accepted
                .path()
                .join(format!("publication-{}.json", c["id"].as_str().unwrap())),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(publication["destinations"]["maven"]["status"], "PUBLISHED");
    assert_eq!(
        publication["destinations"]["maven"]["deployment_id"],
        "deployment-123"
    );
}

#[test]
#[cfg(unix)]
fn prepare_resumes_one_ci_run_without_any_publication() {
    prepare_scenario("lightningdevkit/ldk-node-bindings");
}

#[test]
#[cfg(unix)]
fn prepare_prints_verification_for_the_selected_repository() {
    prepare_scenario("tnull/ldk-node-bindings-test");
}

#[cfg(unix)]
fn prepare_scenario(repo: &str) {
    let r = fixture();
    let candidate = build_candidate(&r);
    let bin = tempfile::tempdir().unwrap();
    let git_binary = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    executable(
        &bin.path().join("git"),
        &format!(
            r##"#!/bin/sh
case "$*" in *' push '*) printf 'push\n' >> "$MOCK_LOG"; exit 0;; esac
exec '{}' "$@"
"##,
            String::from_utf8(git_binary.stdout).unwrap().trim()
        ),
    );
    executable(
        &bin.path().join("gh"),
        r##"#!/bin/sh
case "$1 $2" in
  'run list') if [ -e "$MOCK_DISPATCH" ]; then printf '[{"databaseId":123,"headSha":"%s","displayTitle":"candidate %s"}]' "$MOCK_RECIPE" "$MOCK_RECIPE"; else printf '[]'; fi;;
  'workflow run') touch "$MOCK_DISPATCH"; printf 'dispatch\n' >> "$MOCK_LOG";;
  'run watch') echo 'run watch does not support fine-grained PATs' >&2; exit 98;;
  "api repos/$MOCK_REPO/actions/runs/123")
    if [ -e "$MOCK_RESULT" ]; then cat "$MOCK_RESULT"; else
      printf '{"head_sha":"%s","status":"completed","conclusion":"success","run_attempt":1}' "$MOCK_RECIPE"
    fi;;
  *) echo "unexpected gh call $*" >&2; exit 97;;
esac
"##,
    );
    executable(
        &bin.path().join("curl"),
        r##"#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then shift; result="$1"; fi
  last="$1"; shift
done
cp "$MOCK_CANDIDATE/${last##*/}" "$result"
"##,
    );
    let invoke = || {
        command(env!("CARGO_BIN_EXE_release"))
            .current_dir(r.dir.path())
            .args(["prepare", "--repo", repo])
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("MOCK_LOG", bin.path().join("events"))
            .env("MOCK_DISPATCH", bin.path().join("dispatched"))
            .env("MOCK_RECIPE", &r.sha)
            .env("MOCK_REPO", repo)
            .env("MOCK_RESULT", bin.path().join("result.json"))
            .env("MOCK_CANDIDATE", r.candidate.path())
            .output()
            .unwrap()
    };
    let result = invoke();
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    ok(result);
    let hash = manifest_sha(&r);
    assert!(stdout.contains(&format!("candidate.json SHA-256: {hash}")));
    let repo_option = if repo == "lightningdevkit/ldk-node-bindings" {
        String::new()
    } else {
        format!(" --repo {repo}")
    };
    let verification = format!(
        "./scripts/verify.sh {} {hash}{repo_option}",
        candidate["id"].as_str().unwrap()
    );
    assert!(
        stdout.contains(&verification),
        "missing {verification}: {stdout}"
    );
    let cache = r
        .dir
        .path()
        .join(".release/candidates")
        .join(candidate["id"].as_str().unwrap());
    fs::remove_file(cache.join("linux.whl")).unwrap();
    ok(invoke());
    assert_eq!(
        fs::read_to_string(bin.path().join("events")).unwrap(),
        "push\ndispatch\n"
    );
    let ledger: Value =
        serde_json::from_slice(&fs::read(r.dir.path().join(".release/prepare.json")).unwrap())
            .unwrap();
    assert_eq!(ledger["candidate_id"], candidate["id"]);
    assert_eq!(ledger["phase"], "downloaded");
    for (head, conclusion, expected) in [
        (
            r.sha.as_str(),
            "failure",
            "workflow run 123 concluded failure",
        ),
        (
            r.sha.as_str(),
            "cancelled",
            "workflow run 123 concluded cancelled",
        ),
        (
            "wrong-head",
            "success",
            "workflow head does not match preparation",
        ),
    ] {
        write(
            &bin.path().join("result.json"),
            &json!({"head_sha":head,"status":"completed","conclusion":conclusion,"run_attempt":1}),
        );
        fail(invoke(), expected);
    }
    fs::remove_file(bin.path().join("result.json")).unwrap();
    assert_eq!(
        fs::read_to_string(bin.path().join("events")).unwrap(),
        "push\ndispatch\n"
    );
    fs::write(cache.join("linux.whl"), b"changed cache").unwrap();
    fail(invoke(), "cached candidate differs");
    assert_eq!(fs::read(cache.join("linux.whl")).unwrap(), b"changed cache");
}

#[test]
#[cfg(unix)]
fn testpypi_uploads_only_verified_wheels_and_reconciles() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    let bin = tempfile::tempdir().unwrap();
    let mut urls = vec![];
    for inv in c["groups"].as_object().unwrap().values() {
        for file in inv["files"].as_array().unwrap() {
            if file["name"].as_str().unwrap().ends_with(".whl") {
                urls.push(json!({"filename":file["name"],"digests":{"sha256":file["sha256"]}}));
            }
        }
    }
    executable(
        &bin.path().join("curl"),
        r##"#!/bin/sh
if [ -e "$MOCK_UPLOADED" ]; then printf '%s\n200' "$MOCK_REGISTRY"; else printf '\n404'; fi
"##,
    );
    executable(
        &bin.path().join("uv"),
        r##"#!/bin/sh
[ "$1" = publish ] || exit 93
[ "$3" = https://test.pypi.org/legacy/ ] || exit 94
for arg do last="$arg"; done
printf '%s\n' "${last##*/}" >> "$MOCK_LOG"
touch "$MOCK_UPLOADED"
"##,
    );
    let invoke = || {
        command(env!("CARGO_BIN_EXE_release"))
            .current_dir(r.dir.path())
            .args([
                "publish",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--verified",
                ledger.to_str().unwrap(),
                "--destination",
                "testpypi",
                "--confirm",
                c["id"].as_str().unwrap(),
            ])
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("UV_PUBLISH_TOKEN", "fake-test-token")
            .env("MOCK_LOG", bin.path().join("events"))
            .env("MOCK_UPLOADED", bin.path().join("uploaded"))
            .env("MOCK_REGISTRY", json!({"urls":urls}).to_string())
            .output()
            .unwrap()
    };
    ok(invoke());
    ok(invoke());
    let uploaded = fs::read_to_string(bin.path().join("events")).unwrap();
    let mut actual: Vec<_> = uploaded.lines().collect();
    actual.sort();
    let mut expected: Vec<_> = urls
        .iter()
        .map(|v| v["filename"].as_str().unwrap())
        .collect();
    expected.sort();
    assert_eq!(actual, expected);
}

#[test]
#[cfg(unix)]
fn swift_publishes_distribution_only_and_preserves_recipe() {
    swift_publication_scenario("none");
}

#[test]
#[cfg(unix)]
fn swift_ignores_hooks() {
    swift_publication_scenario("hook");
}

#[test]
#[cfg(unix)]
fn swift_rejects_filter_changes_to_committed_bytes() {
    swift_publication_scenario("filter");
}

#[test]
#[cfg(unix)]
fn swift_resumes_failed_draft_creation_and_missing_asset() {
    swift_publication_scenario("create_failure");
    swift_publication_scenario("upload_failure");
}

#[test]
#[cfg(unix)]
fn swift_does_not_overwrite_differing_draft_asset() {
    swift_publication_scenario("changed_asset");
}

#[cfg(unix)]
fn swift_publication_scenario(mode: &str) {
    let mut r = fixture();
    for name in ["LICENSE-APACHE", "LICENSE-MIT"] {
        fs::write(r.dir.path().join(name), format!("license {name}\n")).unwrap();
    }
    git(r.dir.path(), &["add", "."]);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "License fixture",
        ],
    );
    r.sha = git(r.dir.path(), &["rev-parse", "HEAD"]);
    let sources = tempfile::tempdir().unwrap();
    fs::create_dir_all(sources.path().join("swift/Sources/LDKNode")).unwrap();
    fs::write(
        sources.path().join("swift/Sources/LDKNode/LDKNode.swift"),
        "// generated Swift\n",
    )
    .unwrap();
    assert!(Command::new("tar")
        .current_dir(sources.path())
        .arg("-cf")
        .arg(r.candidate.path().join("generated-sources.tar"))
        .arg(".")
        .status()
        .unwrap()
        .success());
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare"]);
    let bin = tempfile::tempdir().unwrap();
    let git_config = bin.path().join("gitconfig");
    fs::write(&git_config, "").unwrap();
    if mode == "hook" {
        fs::create_dir(bin.path().join("hooks")).unwrap();
        executable(
            &bin.path().join("hooks/pre-commit"),
            "#!/bin/sh\nprintf 'hook corruption' > Package.swift\ngit add Package.swift\n",
        );
        fs::write(
            &git_config,
            format!(
                "[core]\n hooksPath = {}\n",
                bin.path().join("hooks").display()
            ),
        )
        .unwrap();
    }
    if mode == "filter" {
        let attributes = bin.path().join("attributes");
        fs::write(&attributes, "Package.swift filter=corrupt\n").unwrap();
        fs::write(&git_config, format!("[core]\n attributesFile = {}\n[filter \"corrupt\"]\n clean = printf 'filter corruption'\n", attributes.display())).unwrap();
    }
    executable(&bin.path().join("curl"), "#!/bin/sh\nprintf '\\n404'\n");
    executable(
        &bin.path().join("gh"),
        r##"#!/bin/sh
case "$1 $2" in
  'api repos/example/swift/releases')
    if [ ! -e "$MOCK_DRAFT" ]; then printf '[[]]';
    elif [ -e "$MOCK_ASSET" ]; then printf '[[{"tag_name":"v0.7.1","draft":true,"assets":[{"name":"LDKNodeFFI.xcframework.zip"}]}]]';
    else printf '[[{"tag_name":"v0.7.1","draft":true,"assets":[]}]]'; fi;;
  'release create')
    have_notes=no
    for arg do
      case "$arg" in --notes|--notes-file|--generate-notes) have_notes=yes;; esac
    done
    if [ "$have_notes" != yes ]; then echo 'explicit noninteractive release notes required' >&2; exit 78; fi
    if [ "$MOCK_MODE" = create_failure ] && [ ! -e "$MOCK_FAILURE" ]; then touch "$MOCK_FAILURE"; exit 72; fi
    touch "$MOCK_DRAFT"; printf 'draft\n' >> "$MOCK_LOG"
    for arg do last="$arg"; done
    case "$last" in *.zip)
      if [ "$MOCK_MODE" != none ] && [ "$MOCK_MODE" != create_failure ] && [ ! -e "$MOCK_FAILURE" ]; then touch "$MOCK_FAILURE"; exit 73; fi
      cp "$MOCK_CANDIDATE/LDKNodeFFI.xcframework.zip" "$MOCK_ASSET";; esac;;
  'release upload')
    if [ "$MOCK_MODE" != none ] && [ "$MOCK_MODE" != create_failure ] && [ ! -e "$MOCK_FAILURE" ]; then touch "$MOCK_FAILURE"; exit 73; fi
    printf 'upload\n' >> "$MOCK_LOG"; cp "$MOCK_CANDIDATE/LDKNodeFFI.xcframework.zip" "$MOCK_ASSET";;
  'release view')
    [ -e "$MOCK_DRAFT" ] || exit 74
    if [ -e "$MOCK_ASSET" ]; then printf '{"isDraft":true,"assets":[{"name":"LDKNodeFFI.xcframework.zip"}]}'; else printf '{"isDraft":true,"assets":[]}'; fi;;
  'release download') for arg do last="$arg"; done; cp "$MOCK_ASSET" "$last/LDKNodeFFI.xcframework.zip";;
  'release edit') printf 'publish\n' >> "$MOCK_LOG";;
  *) echo 'unexpected gh command' >&2; exit 99;;
esac
"##,
    );
    let invoke = || {
        command(env!("CARGO_BIN_EXE_release"))
            .current_dir(r.dir.path())
            .args([
                "publish",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--verified",
                ledger.to_str().unwrap(),
                "--destination",
                "swift",
                "--confirm",
                c["id"].as_str().unwrap(),
                "--repo",
                "example/swift",
                "--swift-git-url",
                remote.path().to_str().unwrap(),
                "--swift-asset-base-url",
                "https://github.com/example/swift/releases/download",
            ])
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("MOCK_LOG", bin.path().join("events"))
            .env("MOCK_CANDIDATE", r.candidate.path())
            .env(
                "MOCK_MODE",
                if ["hook", "filter"].contains(&mode) {
                    "none"
                } else {
                    mode
                },
            )
            .env("GIT_CONFIG_GLOBAL", &git_config)
            .env("MOCK_DRAFT", bin.path().join("draft"))
            .env("MOCK_ASSET", bin.path().join("asset"))
            .env("MOCK_FAILURE", bin.path().join("failure"))
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.org")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.org")
            .output()
            .unwrap()
    };
    if mode == "filter" {
        fail(invoke(), "Swift distribution content differs");
        assert!(git(remote.path(), &["for-each-ref"]).is_empty());
        assert!(!bin.path().join("events").exists());
        return;
    }
    if !["none", "hook"].contains(&mode) {
        fail(invoke(), "gh failed");
    }
    if mode == "changed_asset" {
        fs::write(bin.path().join("asset"), b"different remote asset").unwrap();
        fail(invoke(), "artifact differs");
        assert_eq!(
            fs::read(bin.path().join("asset")).unwrap(),
            b"different remote asset"
        );
        assert!(git(remote.path(), &["tag", "--list", "v0.7.1"]).is_empty());
        return;
    }
    ok(invoke());
    let files = git(remote.path(), &["ls-tree", "-r", "--name-only", "v0.7.1"]);
    assert_eq!(
        files.lines().collect::<Vec<_>>(),
        [
            "LICENSE-APACHE",
            "LICENSE-MIT",
            "Package.swift",
            "Sources/LDKNode/LDKNode.swift",
            "release-provenance.json"
        ]
    );
    assert!(git(r.dir.path(), &["tag", "--list", "v0.7.1"]).is_empty());
    assert_eq!(git(r.dir.path(), &["rev-parse", "HEAD"]), r.sha);
    let message = git(remote.path(), &["log", "-1", "--format=%B", "v0.7.1"]);
    assert!(
        !message.to_ascii_lowercase().contains("co-authored-by:"),
        "Generated release commits must not invent co-authors: {message}"
    );
    let package = git(remote.path(), &["show", "v0.7.1:Package.swift"]);
    assert!(
        package.contains(".macOS(\"13.2\")") && package.contains(".iOS(\"16.1\")"),
        "Swift metadata must use the recipe deployment targets: {package}"
    );
    let provenance: Value = serde_json::from_str(&git(
        remote.path(),
        &["show", "v0.7.1:release-provenance.json"],
    ))
    .unwrap();
    assert_eq!(provenance["candidate_id"], c["id"]);
    assert_eq!(provenance["recipe"], r.sha);
    assert_eq!(
        fs::read_to_string(bin.path().join("events")).unwrap(),
        "draft\nupload\npublish\n"
    );
}

#[test]
fn inventory_rejects_dirty_recipe_before_claiming_identity() {
    let r = fixture();
    let staging = tempfile::tempdir().unwrap();
    fs::write(staging.path().join("generated-sources.tar"), "x").unwrap();
    fs::write(r.dir.path().join("Cargo.lock"), "uncommitted").unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "inventory",
                "--dir",
                staging.path().to_str().unwrap(),
                "--recipe",
                &r.sha,
                "--group",
                "generated",
                "--output",
                r.candidate
                    .path()
                    .join("generated.inventory.json")
                    .to_str()
                    .unwrap(),
            ],
        ),
        "clean",
    );
}

#[test]
#[cfg(unix)]
fn failed_verification_preserves_rebuilt_and_expected_files() {
    let mut r = fixture();
    fs::create_dir(r.dir.path().join("scripts")).unwrap();
    executable(
        &r.dir.path().join("scripts/build.sh"),
        "#!/bin/sh\nmkdir -p \"$4\"\nprintf 'different build' > \"$4/generated-sources.tar\"\n",
    );
    git(r.dir.path(), &["add", "."]);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "Mismatch fixture",
        ],
    );
    r.sha = git(r.dir.path(), &["rev-parse", "HEAD"]);
    build_candidate(&r);
    let out = tempfile::tempdir().unwrap();
    fail(
        run(
            r.dir.path(),
            &[
                "verify",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--group",
                "generated",
                "--output",
                out.path().to_str().unwrap(),
            ],
        ),
        "preserved work",
    );
    let report: Value =
        serde_json::from_slice(&fs::read(out.path().join("failure.json")).unwrap()).unwrap();
    let work = Path::new(report["work_dir"].as_str().unwrap());
    assert_eq!(
        fs::read(work.join("output/generated-sources.tar")).unwrap(),
        b"different build"
    );
    assert_eq!(
        fs::read(work.join("expected/generated-sources.tar")).unwrap(),
        fs::read(r.candidate.path().join("generated-sources.tar")).unwrap()
    );
    assert!(!out.path().join("generated.receipt.json").exists());
    fs::remove_dir_all(work).unwrap();
}

#[test]
#[cfg(unix)]
fn publication_excludes_concurrent_destinations_and_releases_lock() {
    let r = fixture();
    let c = build_candidate(&r);
    let accepted = receipts(&r, &c);
    let ledger = accepted.path().join("verified.json");
    ok(run(
        r.dir.path(),
        &[
            "collect",
            "--candidate",
            r.candidate.path().to_str().unwrap(),
            "--receipts",
            accepted.path().to_str().unwrap(),
            "--output",
            ledger.to_str().unwrap(),
        ],
    ));
    let bin = tempfile::tempdir().unwrap();
    executable(&bin.path().join("curl"), "#!/bin/sh\nprintf '\\n404'\n");
    executable(&bin.path().join("uv"),"#!/bin/sh\ntouch \"$MOCK_ENTERED\"\nwhile [ ! -e \"$MOCK_RELEASE\" ]; do sleep 0.05; done\nexit 71\n");
    let make = |destination: &str| {
        let mut cmd = command(env!("CARGO_BIN_EXE_release"));
        cmd.current_dir(r.dir.path())
            .args([
                "publish",
                "--candidate",
                r.candidate.path().to_str().unwrap(),
                "--verified",
                ledger.to_str().unwrap(),
                "--destination",
                destination,
                "--confirm",
                c["id"].as_str().unwrap(),
            ])
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.path().display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("UV_PUBLISH_TOKEN", "fake")
            .env_remove("MAVEN_CENTRAL_TOKEN")
            .env_remove("CENTRAL_TOKEN_USERNAME")
            .env_remove("CENTRAL_TOKEN_PASSWORD")
            .env("MOCK_ENTERED", bin.path().join("entered"))
            .env("MOCK_RELEASE", bin.path().join("release"));
        cmd
    };
    let mut first = make("testpypi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if bin.path().join("entered").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(bin.path().join("entered").exists());
    let concurrent = make("maven").output().unwrap();
    fs::write(bin.path().join("release"), b"").unwrap();
    assert!(!first.wait().unwrap().success());
    fail(concurrent, "another publication");
    let retry = make("maven").output().unwrap();
    fail(retry, "set MAVEN_CENTRAL_TOKEN");
}

#[cfg(unix)]
fn entry_fixture() -> Repo {
    let mut r = fixture();
    fs::create_dir(r.dir.path().join("scripts")).unwrap();
    let inputs: Value =
        serde_json::from_slice(&fs::read(r.dir.path().join("release.json")).unwrap()).unwrap();
    let mut script = String::from("#!/bin/sh\nset -eu\nmkdir -p \"$4\"\ncase \"$2\" in\n");
    for (group, names) in inputs["groups"].as_object().unwrap() {
        script.push_str(&format!("{group})\n"));
        for name in names.as_array().unwrap() {
            let name = name.as_str().unwrap();
            script.push_str(&format!("printf 'payload \"{name}\"' > \"$4/{name}\"\n"));
        }
        script.push_str(";;\n");
    }
    script.push_str("*) exit 91;;\nesac\n");
    executable(&r.dir.path().join("scripts/build.sh"), &script);
    git(r.dir.path(), &["add", "."]);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "-m",
            "Build fixture",
        ],
    );
    r.sha = git(r.dir.path(), &["rev-parse", "HEAD"]);
    r
}

#[cfg(unix)]
fn entry_run(r: &Repo, id: &str, sha: &str, output: &Path, group: Option<&str>) -> Output {
    entry_run_repo(r, id, sha, output, group, None)
}

#[cfg(unix)]
fn entry_run_repo(
    r: &Repo,
    id: &str,
    sha: &str,
    output: &Path,
    group: Option<&str>,
    repo: Option<&str>,
) -> Output {
    let bin = tempfile::tempdir().unwrap();
    let official = tempfile::tempdir().unwrap();
    git(official.path(), &["init", "--bare", "--quiet"]);
    git(
        official.path(),
        &[
            "fetch",
            "--quiet",
            r.dir.path().to_str().unwrap(),
            "+refs/heads/*:refs/fixture/*",
        ],
    );
    let branch = fs::read_to_string(r.candidate.path().join("official-branch"))
        .unwrap_or_else(|_| "main".into());
    let commit = fs::read_to_string(r.candidate.path().join("official-commit"))
        .unwrap_or_else(|_| r.sha.clone());
    if ["fork-only", "tag-only"].contains(&branch.as_str()) {
        git(
            official.path(),
            &[
                "update-ref",
                "refs/heads/main",
                &git(r.dir.path(), &["rev-parse", &format!("{}^", r.sha)]),
            ],
        );
        if branch == "tag-only" {
            git(
                official.path(),
                &["update-ref", &format!("refs/tags/{id}"), &r.sha],
            );
        }
    } else {
        git(
            official.path(),
            &["update-ref", &format!("refs/heads/{branch}"), &commit],
        );
    }
    {
        let git_binary = Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        executable(
            &bin.path().join("git"),
            &format!(
                r#"#!/bin/sh
set -eu
if [ "$1" = -c ]; then
    [ "$2" = core.hooksPath=/dev/null ] || exit 90
    shift 2
fi
case "$1" in
  fetch)
    [ -z "${{GH_TOKEN:-}}${{GITHUB_TOKEN:-}}${{GIT_CONFIG_PARAMETERS:-}}" ] || exit 91
    [ "$GIT_CONFIG_NOSYSTEM" = 1 ] && [ "$GIT_CONFIG_GLOBAL" = /dev/null ] && [ "$GIT_CONFIG_COUNT" = 0 ] && [ "$GIT_TERMINAL_PROMPT" = 0 ] || exit 92
    [ "$*" = "fetch --quiet --no-tags https://github.com/$MOCK_REPO.git +refs/heads/*:refs/remotes/reviewed/*" ] || exit 93
    touch "$MOCK_FETCHED"
    exec '{0}' fetch --quiet --no-tags "$MOCK_OFFICIAL" '+refs/heads/*:refs/remotes/reviewed/*';;
esac
exec '{0}' "$@"
"#,
                String::from_utf8(git_binary.stdout).unwrap().trim()
            ),
        );
    }
    executable(
        &bin.path().join("curl"),
        r#"#!/bin/sh
set -eu
[ -z "${GH_TOKEN:-}${GITHUB_TOKEN:-}" ] || exit 96
while [ "$#" -gt 0 ]; do
  case "$1" in --output) shift; destination=$1;; esac
  url=$1; shift
done
case "$url" in "https://github.com/$MOCK_REPO/releases/download/"candidate-*/*) ;; *) exit 97;; esac
name=${url##*/}
printf '%s\n' "$name" >> "$MOCK_DOWNLOADS"
cp "$MOCK_CANDIDATE/$name" "$destination"
"#,
    );
    let mut cmd = command(env!("CARGO_BIN_EXE_release"));
    cmd.current_dir(r.dir.path())
        .args(["verify-candidate", id, sha, "--output"])
        .arg(output)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("MOCK_CANDIDATE", r.candidate.path())
        .env(
            "MOCK_REPO",
            repo.unwrap_or("lightningdevkit/ldk-node-bindings"),
        )
        .env("MOCK_FETCHED", r.candidate.path().join("fetched"))
        .env("MOCK_OFFICIAL", official.path())
        .env("MOCK_DOWNLOADS", r.candidate.path().join("downloads"))
        .env("GH_TOKEN", "must-not-be-used")
        .env("GITHUB_TOKEN", "must-not-be-used");
    if let Some(group) = group {
        cmd.args(["--group", group]);
    }
    if let Some(repo) = repo {
        cmd.args(["--repo", repo]);
    }
    if r.candidate.path().join("inherit-repository").exists() {
        cmd.env("GIT_DIR", r.dir.path().join(".git"))
            .env("GIT_COMMON_DIR", r.dir.path().join(".git"))
            .env("GIT_WORK_TREE", r.dir.path())
            .env("GIT_INDEX_FILE", r.dir.path().join(".git/index"))
            .env("GIT_OBJECT_DIRECTORY", r.dir.path().join(".git/objects"))
            .env(
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                r.dir.path().join(".git/objects"),
            );
    }
    cmd.output().unwrap()
}

#[test]
#[cfg(unix)]
fn entry_uses_selected_repository_assets_and_branch_history() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "--allow-empty",
            "-m",
            "Descendant",
        ],
    );
    fs::write(
        r.candidate.path().join("official-commit"),
        git(r.dir.path(), &["rev-parse", "HEAD"]),
    )
    .unwrap();
    for branch in ["main", "0.7.x", "2026-10-prepare"] {
        let out = tempfile::tempdir().unwrap();
        fs::write(r.candidate.path().join("official-branch"), branch).unwrap();
        ok(entry_run_repo(
            &r,
            c["id"].as_str().unwrap(),
            &manifest_sha(&r),
            out.path(),
            None,
            Some("tnull/ldk-node-bindings-test"),
        ));
        assert_eq!(
            git(&out.path().join("recipe"), &["rev-parse", "HEAD"]),
            r.sha
        );
        assert!(out.path().join("receipts").is_dir());
    }
    for branch in [
        "fork-only",
        "tag-only",
        "unreviewed-feature",
        "0.bad.x",
        "2026-1-prepare",
    ] {
        let out = tempfile::tempdir().unwrap();
        fs::write(r.candidate.path().join("official-branch"), branch).unwrap();
        fail(
            entry_run_repo(
                &r,
                c["id"].as_str().unwrap(),
                &manifest_sha(&r),
                out.path(),
                None,
                Some("tnull/ldk-node-bindings-test"),
            ),
            "recipe is not reachable from an official release branch",
        );
        assert!(!out.path().join("receipts").exists());
    }
}

#[test]
#[cfg(unix)]
fn entry_rejects_malformed_repository_before_downloading() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    for (repo, error) in [
        ("owner", "repository must be OWNER/REPO"),
        ("owner/repo/extra", "repository must be OWNER/REPO"),
        (
            "https://github.com/owner/repo",
            "repository must be OWNER/REPO",
        ),
        ("/repo", "unsafe filename or identifier"),
        ("owner/", "unsafe filename or identifier"),
        ("owner/..", "unsafe filename or identifier"),
        ("owner/-repo", "unsafe filename or identifier"),
        ("owner/repo?token=x", "unsafe filename or identifier"),
    ] {
        let out = tempfile::tempdir().unwrap();
        fail(
            entry_run_repo(
                &r,
                c["id"].as_str().unwrap(),
                &manifest_sha(&r),
                out.path(),
                None,
                Some(repo),
            ),
            error,
        );
        assert!(!r.candidate.path().join("downloads").exists());
        assert!(!r.candidate.path().join("fetched").exists());
    }
}

#[test]
#[cfg(unix)]
fn entry_ignores_inherited_repository_locations() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    let refs = git(r.dir.path(), &["show-ref"]);
    let status = git(r.dir.path(), &["status", "--porcelain"]);
    let index = fs::read(r.dir.path().join(".git/index")).unwrap();
    fs::write(r.candidate.path().join("inherit-repository"), "").unwrap();
    let output = tempfile::tempdir().unwrap();
    let result = entry_run(
        &r,
        c["id"].as_str().unwrap(),
        &manifest_sha(&r),
        output.path(),
        None,
    );
    assert_eq!(
        git(r.dir.path(), &["show-ref"]),
        refs,
        "verification must not change the caller's Git refs"
    );
    assert_eq!(git(r.dir.path(), &["status", "--porcelain"]), status);
    assert_eq!(git(r.dir.path(), &["rev-parse", "HEAD"]), r.sha);
    assert_eq!(fs::read(r.dir.path().join(".git/index")).unwrap(), index);
    ok(result);
    assert_eq!(
        git(&output.path().join("recipe"), &["rev-parse", "HEAD"]),
        r.sha
    );
    assert!(output.path().join("receipts").is_dir());
}

#[cfg(unix)]
fn manifest_sha(r: &Repo) -> String {
    use bitcoin_hashes::{sha256, Hash};
    sha256::Hash::hash(&fs::read(r.candidate.path().join("candidate.json")).unwrap()).to_string()
}

#[test]
#[cfg(unix)]
fn entry_rejects_invalid_or_wrong_manifest_checksum_before_checkout() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    let out = tempfile::tempdir().unwrap();
    fail(
        entry_run(&r, c["id"].as_str().unwrap(), "invalid", out.path(), None),
        "invalid hexadecimal identity",
    );
    assert!(!r.candidate.path().join("downloads").exists());
    // The checksum is verified before even attempting to parse the manifest.
    fs::write(r.candidate.path().join("candidate.json"), "invalid JSON").unwrap();
    fail(
        entry_run(
            &r,
            c["id"].as_str().unwrap(),
            &"0".repeat(64),
            out.path(),
            None,
        ),
        "candidate manifest SHA-256 mismatch",
    );
    assert!(!out.path().join("recipe").exists());
    assert!(!out.path().join("receipts").exists());
    assert_eq!(
        fs::read_to_string(r.candidate.path().join("downloads")).unwrap(),
        "candidate.json\n"
    );
}

#[test]
#[cfg(unix)]
fn entry_rejects_fork_tag_only_and_unapproved_branch_recipes() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    for branch in [
        "fork-only",
        "tag-only",
        "unreviewed-feature",
        "0.bad.x",
        "2026-1-prepare",
    ] {
        let out = tempfile::tempdir().unwrap();
        fs::write(r.candidate.path().join("official-branch"), branch).unwrap();
        fail(
            entry_run(
                &r,
                c["id"].as_str().unwrap(),
                &manifest_sha(&r),
                out.path(),
                None,
            ),
            "recipe is not reachable from an official release branch",
        );
        assert!(!out.path().join("receipts").exists());
        assert!(fs::read_to_string(r.candidate.path().join("downloads"))
            .unwrap()
            .lines()
            .all(|name| name == "candidate.json"));
    }
}

#[test]
#[cfg(unix)]
fn entry_accepts_official_release_branches_and_historical_ancestors() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    git(
        r.dir.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.org",
            "commit",
            "--allow-empty",
            "-m",
            "Descendant",
        ],
    );
    fs::write(
        r.candidate.path().join("official-commit"),
        git(r.dir.path(), &["rev-parse", "HEAD"]),
    )
    .unwrap();
    for branch in ["main", "0.7.x", "2026-10-prepare"] {
        let out = tempfile::tempdir().unwrap();
        fs::write(r.candidate.path().join("official-branch"), branch).unwrap();
        ok(entry_run(
            &r,
            c["id"].as_str().unwrap(),
            &manifest_sha(&r),
            out.path(),
            None,
        ));
        assert!(out.path().join("receipts").is_dir());
    }
}

#[test]
#[cfg(unix)]
fn entry_rejects_manifest_tag_and_recipe_mismatches_before_build() {
    let r = entry_fixture();
    let mut c = build_candidate(&r);
    let out = tempfile::tempdir().unwrap();
    fail(
        entry_run(
            &r,
            &format!("{}9", c["id"].as_str().unwrap()),
            &manifest_sha(&r),
            out.path(),
            None,
        ),
        "candidate tag differs from manifest ID",
    );
    assert!(!out.path().join("recipe").exists());
    let out = tempfile::tempdir().unwrap();
    c["recipe"] = json!("not-a-full-commit");
    write(&r.candidate.path().join("candidate.json"), &c);
    fail(
        entry_run(
            &r,
            c["id"].as_str().unwrap(),
            &manifest_sha(&r),
            out.path(),
            None,
        ),
        "invalid hexadecimal identity",
    );
    assert!(!out.path().join("recipe").exists());
    let out = tempfile::tempdir().unwrap();
    c["recipe"] = json!(git(r.dir.path(), &["rev-parse", "HEAD^"]));
    write(&r.candidate.path().join("candidate.json"), &c);
    fail(
        entry_run(
            &r,
            c["id"].as_str().unwrap(),
            &manifest_sha(&r),
            out.path(),
            None,
        ),
        "candidate ID does not match recipe/version",
    );
    assert!(!out.path().join("receipts").exists());
}

#[test]
#[cfg(unix)]
fn entry_rebuilds_host_groups_once_from_pinned_recipe_without_changing_checkout() {
    let r = entry_fixture();
    let c = build_candidate(&r);
    git(r.dir.path(), &["checkout", "--detach", "HEAD^"]);
    fs::write(r.dir.path().join("local-notes"), "keep my untracked notes").unwrap();
    let original = git(r.dir.path(), &["rev-parse", "HEAD"]);
    let status = git(r.dir.path(), &["status", "--porcelain"]);
    let out = tempfile::tempdir().unwrap();
    let result = entry_run(
        &r,
        c["id"].as_str().unwrap(),
        &manifest_sha(&r),
        out.path(),
        None,
    );
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    ok(result);
    let expected: &[&str] = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => &["generated", "linux-x86_64", "android", "jvm"],
        ("linux", "aarch64") => &["linux-aarch64"],
        ("macos", "aarch64") => &["darwin"],
        _ => panic!("test requires a supported verification host"),
    };
    assert!(stdout.contains(&format!("Verified groups: {}", expected.join(", "))));
    assert!(stdout.contains("Outstanding groups:"));
    assert_eq!(
        fs::read_dir(out.path().join("receipts")).unwrap().count(),
        expected.len()
    );
    for group in expected {
        let receipt: Value = serde_json::from_slice(
            &fs::read(out.path().join(format!("receipts/{group}.receipt.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["recipe"], r.sha);
        assert_eq!(receipt["candidate_id"], c["id"]);
    }
    let downloads = fs::read_to_string(r.candidate.path().join("downloads")).unwrap();
    let unique: std::collections::BTreeSet<_> = downloads.lines().collect();
    assert_eq!(
        downloads.lines().count(),
        unique.len(),
        "download each asset only once"
    );
    assert_eq!(git(r.dir.path(), &["rev-parse", "HEAD"]), original);
    assert_eq!(git(r.dir.path(), &["status", "--porcelain"]), status);
}

#[test]
#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn entry_supports_one_group_and_preserves_mismatch_diagnostics() {
    let r = entry_fixture();
    let mut c = build_candidate(&r);
    let out = tempfile::tempdir().unwrap();
    ok(entry_run(
        &r,
        c["id"].as_str().unwrap(),
        &manifest_sha(&r),
        out.path(),
        Some("generated"),
    ));
    assert!(r.candidate.path().join("fetched").exists());
    assert_eq!(
        fs::read_dir(out.path().join("receipts")).unwrap().count(),
        1
    );
    fs::write(
        r.candidate.path().join("generated-sources.tar"),
        "wrong build",
    )
    .unwrap();
    use bitcoin_hashes::{sha256, Hash};
    c["groups"]["generated"]["files"][0]["sha256"] =
        json!(sha256::Hash::hash(b"wrong build").to_string());
    c["groups"]["generated"]["files"][0]["size"] = json!(11);
    write(&r.candidate.path().join("candidate.json"), &c);
    let out = tempfile::tempdir().unwrap();
    fail(
        entry_run(
            &r,
            c["id"].as_str().unwrap(),
            &manifest_sha(&r),
            out.path(),
            Some("generated"),
        ),
        "artifact differs",
    );
    assert!(
        !out.path().join("receipts/failure.json").exists(),
        "aggregate receipts must not contain failure diagnostics"
    );
    let failure: Value = serde_json::from_slice(
        &fs::read(out.path().join("diagnostics/generated/failure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fs::read_dir(out.path().join("receipts")).unwrap().count(),
        0
    );
    let work = Path::new(failure["work_dir"].as_str().unwrap());
    assert!(work.join("expected/generated-sources.tar").is_file());
    assert!(!out.path().join("receipts/generated.receipt.json").exists());
    fs::remove_dir_all(work).unwrap();
}

#[test]
#[cfg(unix)]
fn verification_script_passes_arguments_and_preserves_exit_status() {
    let bin = tempfile::tempdir().unwrap();
    executable(
        &bin.path().join("nix"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$MOCK_ARGUMENTS\"\nexit 37\n",
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let args_file = bin.path().join("arguments");
    let result = Command::new("bash")
        .arg(root.join("scripts/verify.sh"))
        .args([
            "candidate-version-recipe-run-attempt",
            &"a".repeat(64),
            "--group",
            "generated",
            "--output",
            "/tmp/output with spaces",
            "--repo",
            "tnull/ldk-node-bindings-test",
        ])
        .env("MOCK_ARGUMENTS", &args_file)
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(37));
    let args = fs::read_to_string(args_file).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "--extra-experimental-features",
            "nix-command flakes",
            "run",
            "--no-update-lock-file",
            &format!("{}#release", fs::canonicalize(root).unwrap().display()),
            "--",
            "verify-candidate",
            "candidate-version-recipe-run-attempt",
            &"a".repeat(64),
            "--group",
            "generated",
            "--output",
            "/tmp/output with spaces",
            "--repo",
            "tnull/ldk-node-bindings-test",
        ]
    );
}

#[test]
#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn entry_reports_completed_groups_when_a_later_group_fails() {
    let r = entry_fixture();
    let mut c = build_candidate(&r);
    fs::write(r.candidate.path().join("linux.whl"), "wrong build").unwrap();
    use bitcoin_hashes::{sha256, Hash};
    let wheel = c["groups"]["linux-x86_64"]["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["name"] == "linux.whl")
        .unwrap();
    wheel["sha256"] = json!(sha256::Hash::hash(b"wrong build").to_string());
    wheel["size"] = json!(11);
    write(&r.candidate.path().join("candidate.json"), &c);
    let out = tempfile::tempdir().unwrap();
    let result = entry_run(
        &r,
        c["id"].as_str().unwrap(),
        &manifest_sha(&r),
        out.path(),
        None,
    );
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    fail(result, "artifact differs: linux.whl");
    let diagnostics: Value = serde_json::from_slice(
        &fs::read(out.path().join("diagnostics/linux-x86_64/failure.json")).unwrap(),
    )
    .unwrap();
    let work = Path::new(diagnostics["work_dir"].as_str().unwrap());
    assert!(work.join("expected/linux.whl").is_file());
    assert!(work.join("output/linux.whl").is_file());
    fs::remove_dir_all(work).unwrap();
    assert!(
        stdout.contains("Verified groups: generated, android, jvm. Outstanding groups: darwin, linux-aarch64, linux-x86_64"),
        "later failures must report completed and outstanding groups: {stdout}"
    );
    assert!(stdout.contains(&format!(
        "Receipts: {}",
        out.path().join("receipts").display()
    )));
    assert!(!stdout.contains("Verified candidate"));
    assert!(out.path().join("receipts/generated.receipt.json").is_file());
    assert_eq!(
        fs::read_dir(out.path().join("receipts")).unwrap().count(),
        3
    );
}
