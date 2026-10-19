#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn bash() -> PathBuf {
    let output = Command::new("bash")
        .args(["-c", "command -v bash"])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn mock_nix(directory: &Path) {
    let path = directory.join("nix");
    fs::write(
        &path,
        format!(
            "#!{}\nprintf 'verifier output\\n'\nprintf 'verifier diagnostic\\n' >&2\nexit \"$MOCK_STATUS\"\n",
            bash().display()
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn verify(directory: &Path) -> Command {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut command = Command::new(bash());
    command.arg(root.join("scripts/verify.sh")).env(
        "PATH",
        format!("{}:{}", directory.display(), std::env::var("PATH").unwrap()),
    );
    command
}

fn assert_result(result: Output, status: i32) {
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(result.status.code(), Some(status), "{stdout}\n{stderr}");
    let (summary, other, expected) = if status == 0 {
        (&stdout, &stderr, "VERIFICATION SUCCESSFUL")
    } else {
        (&stderr, &stdout, "VERIFICATION FAILED")
    };
    assert_eq!(summary.lines().last(), Some(expected), "{stdout}\n{stderr}");
    assert_eq!(summary.matches("VERIFICATION ").count(), 1);
    assert!(!other.contains("VERIFICATION "), "{stdout}\n{stderr}");
}

#[test]
fn native_verification_reports_success_and_failure() {
    let bin = tempfile::tempdir().unwrap();
    mock_nix(bin.path());
    for status in [0, 37, 130, 143] {
        let result = verify(bin.path())
            .args(["candidate-test", &"a".repeat(64)])
            .env("MOCK_STATUS", status.to_string())
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&result.stdout).contains("verifier output"));
        assert!(String::from_utf8_lossy(&result.stderr).contains("verifier diagnostic"));
        assert_result(result, status);
    }
}

#[test]
fn native_verification_reports_early_errors() {
    let bin = tempfile::tempdir().unwrap();
    assert_result(verify(bin.path()).output().unwrap(), 2);
    let result = verify(bin.path())
        .args(["candidate-test", &"a".repeat(64)])
        .env("PATH", bin.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&result.stderr).contains("Install Nix"));
    assert_result(result, 1);
}

#[test]
fn native_help_does_not_claim_verification() {
    let bin = tempfile::tempdir().unwrap();
    mock_nix(bin.path());
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["candidate-test", "hash", "--help"],
    ] {
        let result = verify(bin.path())
            .args(args)
            .env("MOCK_STATUS", "0")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("VERIFICATION "));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("VERIFICATION "));
    }
}

#[test]
fn native_verification_reports_termination_and_stops_nix() {
    let bin = tempfile::tempdir().unwrap();
    mock_nix(bin.path());
    fs::write(
        bin.path().join("nix"),
        format!(
            "#!{}\nsleep 30 &\nsleeper=$!\nstop() {{ kill \"$sleeper\"; wait \"$sleeper\" 2>/dev/null; sleep 0.1; echo verifier stopped >&2; exit \"$1\"; }}\ntrap 'stop 130' INT\ntrap 'stop 143' TERM\nprintf ready > \"$MOCK_READY\"\nwait \"$sleeper\"\n",
            bash().display()
        ),
    )
    .unwrap();
    for (signal, status) in [("TERM", 143), ("INT", 130)] {
        let ready = bin.path().join(signal);
        let mut child = verify(bin.path())
            .args(["candidate-test", &"a".repeat(64)])
            .env("MOCK_READY", &ready)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id().to_string();
        let send_signal = |name: &str| {
            assert!(Command::new(bash())
                .args(["-c", "kill -\"$1\" \"$2\"", "signal-verifier", name, &pid])
                .status()
                .unwrap()
                .success());
        };
        let started = Instant::now();
        while !ready.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        send_signal(signal);
        let stopped_at = Instant::now();
        while child.try_wait().unwrap().is_none() && stopped_at.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let stopped = child.try_wait().unwrap().is_some();
        if !stopped {
            send_signal("TERM");
        }
        let result = child.wait_with_output().unwrap();
        assert!(
            ready.exists(),
            "mock verifier must start before termination"
        );
        assert!(
            stopped,
            "{signal} must stop Nix without waiting for its build to finish"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("verifier stopped"));
        assert_result(result, status);
    }
}
