#![cfg(unix)]
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::Path,
    process::Command,
};

fn executable(path: &Path, source: &str) {
    let bash = Command::new("bash")
        .args(["-c", "command -v bash"])
        .output()
        .unwrap();
    assert!(bash.status.success());
    let bash = String::from_utf8(bash.stdout).unwrap();
    fs::write(
        path,
        source.replacen("#!/usr/bin/env bash", &format!("#!{}", bash.trim()), 1),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
}

#[test]
fn container_bootstrap_does_not_need_git_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_release"))
        .current_dir(temp.path())
        .args(["verify-candidate", "not-a-candidate", &"a".repeat(64)])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("expected a candidate release tag"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn container_launcher_preserves_arguments_results_and_failure() {
    let temp = tempfile::tempdir().unwrap();
    // A linked Git directory exercises worktree-style source discovery, even
    // when this test itself runs from a Nix source tree without Git metadata.
    let checkout = temp.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    assert!(Command::new("git")
        .args(["init", "--separate-git-dir"])
        .arg(temp.path().join("git"))
        .arg(&checkout)
        .output()
        .unwrap()
        .status
        .success());
    fs::create_dir(checkout.join("scripts")).unwrap();
    fs::copy(
        root().join("scripts/verify-docker.sh"),
        checkout.join("scripts/verify-docker.sh"),
    )
    .unwrap();
    fs::write(checkout.join("private-untracked-file"), "must stay on host").unwrap();
    assert!(Command::new("git")
        .current_dir(&checkout)
        .args(["add", "scripts"])
        .status()
        .unwrap()
        .success());
    let socket = temp.path().join("engine.sock");
    let _socket = UnixListener::bind(&socket).unwrap();
    let engine = temp.path().join("docker");
    executable(
        &engine,
        r#"#!/usr/bin/env bash
set -eu
case $1 in
    info)
        if [[ $TEST_ROOTLESS = true ]]; then echo 'linux ["name=rootless"]'; else echo 'linux []'; fi ;;
    context) printf 'unix://%s\n' "$TEST_SOCKET" ;;
    run)
        for arg in "$@"; do
            if [[ $arg = chown ]]; then printf '%s\n' "$@" > "$TEST_OWNER"; exit 0; fi
        done
        printf '%s\n' "$@" > "$TEST_ARGS"
        for arg in "$@"; do
            case $arg in *:/state) tar -tf "${arg%:/state}/source.tar" > "$TEST_ARCHIVE" ;; esac
        done
        if [[ ${TEST_CANCEL:-false} = true ]]; then kill -TERM "$PPID"; exec sleep 30; fi
        exit 37 ;;
    ps)
        name=$(sed -n '/^--name$/{n;p;}' "$TEST_ARGS")
        [[ $2 = -aq && $3 = --filter && $4 = "label=ldk.bindings.verify=$name" ]] || exit 92
        echo fixture-sibling ;;
    rm) printf '%s\n' "$@" >> "$TEST_CLEANUP" ;;
    *) echo "unexpected engine operation: $*" >&2; exit 91 ;;
esac
"#,
    );
    let output = temp.path().join("results with spaces");
    let args = temp.path().join("args");
    for (cancel, rootless) in [(false, false), (true, false), (false, true)] {
        fs::write(temp.path().join("cleanup"), "").unwrap();
        fs::write(temp.path().join("owner"), "").unwrap();
        let started = std::time::Instant::now();
        let result = Command::new("bash")
            .arg(checkout.join("scripts/verify-docker.sh"))
            .current_dir(temp.path())
            .args([
                "candidate-test",
                &"a".repeat(64),
                "--repo",
                "owner/repo",
                "--group",
                "jvm",
                "--output",
            ])
            .arg(&output)
            .env("CONTAINER_ENGINE", &engine)
            .env("TEST_SOCKET", &socket)
            .env("TEST_ARGS", &args)
            .env("TEST_ARCHIVE", temp.path().join("archive"))
            .env("TEST_CLEANUP", temp.path().join("cleanup"))
            .env("TEST_CANCEL", cancel.to_string())
            .env("TEST_ROOTLESS", rootless.to_string())
            .env("TEST_OWNER", temp.path().join("owner"))
            .env_remove("DOCKER_HOST")
            .env_remove("DOCKER_CONTEXT")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(if cancel { 143 } else { 37 }),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let arguments = fs::read_to_string(&args).unwrap();
        let arguments: Vec<_> = arguments.lines().collect();
        assert!(
            arguments.contains(&format!("{}:/results", output.display()).as_str()),
            "host results must survive the container: {arguments:?}"
        );
        assert!(arguments
            .windows(4)
            .any(|a| a == ["--repo", "owner/repo", "--group", "jvm"]));
        assert!(!arguments.contains(&"--privileged"));
        assert!(output.is_dir());
        assert_eq!(
            fs::read_to_string(temp.path().join("archive")).unwrap(),
            "scripts/verify-docker.sh\n"
        );
        assert!(
            fs::read_to_string(temp.path().join("cleanup"))
                .unwrap()
                .contains("fixture-sibling"),
            "cleanup must stop this invocation's siblings"
        );
        assert!(arguments.contains(&"--userns=host"));
        let owner = fs::read_to_string(temp.path().join("owner")).unwrap();
        assert!(
            owner.contains("chown\n-Rh\n"),
            "cleanup must restore ownership even for rootless build-user files"
        );
        if rootless {
            assert!(owner.contains("\n0:0\n"));
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "signal handling must not wait for the build to finish"
        );
    }
}

#[test]
fn container_launcher_rejects_darwin_before_starting_engine() {
    let result = Command::new("bash")
        .arg(root().join("scripts/verify-docker.sh"))
        .args(["candidate-test", &"a".repeat(64), "--group", "darwin"])
        .env("CONTAINER_ENGINE", "/nonexistent-engine")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("Darwin verification requires native macOS"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn container_engine_maps_shared_store_and_private_scratch() {
    let temp = tempfile::tempdir().unwrap();
    let engine = temp.path().join("engine");
    fs::copy(root().join("scripts/verify-docker-engine.sh"), &engine)
        .expect("container engine adapter must exist");
    let client = temp.path().join("client");
    let args = temp.path().join("args");
    executable(
        &client,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$TEST_ARGS\"\nexit 23\n",
    );
    fs::write(
        temp.path().join("engine.env"),
        format!(
            "client='{}'\nstore=ldk-test-store\nhost_work='/host/scratch with spaces'\nrun_id=test-run\n",
            client.display()
        ),
    )
    .unwrap();
    let result = Command::new("bash")
        .arg(&engine)
        .args([
            "run",
            "--read-only",
            "-v",
            "/nix/store:/nix/store:ro",
            "-v",
            "/tmp/native/source:/work",
            "-v",
            "/tmp/native/output:/wheels:ro",
            "image",
            "bash",
            "-c",
            "echo untouched",
        ])
        .env("TEST_ARGS", &args)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(&args).unwrap(), "run\n--label\nldk.bindings.verify=test-run\n--userns=host\n--read-only\n-v\nldk-test-store:/nix:ro\n-v\n/host/scratch with spaces/native/source:/work\n-v\n/host/scratch with spaces/native/output:/wheels:ro\nimage\nbash\n-c\necho untouched\n");
    fs::remove_file(&args).unwrap();
    let result = Command::new("bash")
        .arg(engine)
        .args(["run", "-v", "/unexpected:/work", "image"])
        .env("TEST_ARGS", &args)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        !args.exists(),
        "unmapped bind source must fail before engine creates host directories"
    );
}
