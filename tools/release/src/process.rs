use anyhow::{ensure, Context, Result};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn output(command: &mut Command) -> Result<String> {
    let program = command.get_program().to_string_lossy().to_string();
    let result = command
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    ensure!(
        result.status.success(),
        "{program} failed ({}): {}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}
pub fn run(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().to_string();
    ensure!(
        command
            .status()
            .with_context(|| format!("cannot run {program}"))?
            .success(),
        "{program} failed"
    );
    Ok(())
}
pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    output(Command::new("git").current_dir(root).args(args))
}
pub fn root() -> Result<std::path::PathBuf> {
    Ok(git(Path::new("."), &["rev-parse", "--show-toplevel"])?.into())
}
pub fn clean(root: &Path) -> Result<String> {
    ensure!(
        git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty(),
        "recipe checkout must be clean and committed"
    );
    git(root, &["rev-parse", "HEAD"])
}
pub fn input_hash(root: &Path) -> Result<String> {
    use bitcoin_hashes::{sha256, Hash, HashEngine};
    let files = git(root, &["ls-files", "-z"])?;
    let mut paths: Vec<_> = files
        .split('\0')
        .filter(|p| {
            !p.is_empty()
                && (*p == "release.json"
                    || p.ends_with(".lock")
                    || p.ends_with("lock.json")
                    || p.ends_with("lockfile")
                    || p.ends_with("verification-metadata.xml")
                    || p.starts_with("locks/"))
        })
        .collect();
    paths.sort();
    ensure!(
        paths.contains(&"release.json"),
        "release.json must be tracked"
    );
    let mut hash = sha256::Hash::engine();
    for path in paths {
        let data = std::fs::read(root.join(path))?;
        hash.input(&(path.len() as u64).to_le_bytes());
        hash.input(path.as_bytes());
        hash.input(&(data.len() as u64).to_le_bytes());
        hash.input(&data);
    }
    Ok(sha256::Hash::from_engine(hash).to_string())
}
pub fn curl() -> Command {
    let mut c = Command::new("curl");
    c.arg("--disable").args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--connect-timeout",
        "30",
        "--max-time",
        "1800",
    ]);
    c.env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
    c
}
pub fn download(url: &str, path: &Path) -> Result<()> {
    ensure!(url.starts_with("https://"), "HTTPS required");
    run(curl().arg("--output").arg(path).arg(url))
}
pub fn authenticated_curl(url: &str, token: &str, args: &[String]) -> Result<String> {
    ensure!(
        token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=_-.".contains(&b)),
        "invalid token encoding"
    );
    let mut command = curl();
    command
        .arg("--config")
        .arg("-")
        .args(args)
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .context("curl stdin")?
        .write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes())?;
    let result = child.wait_with_output()?;
    ensure!(
        result.status.success(),
        "authenticated upload failed ({}): {}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn input_identity_preserves_sorted_length_framing() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        git(root, &["init", "-q"]).unwrap();
        fs::create_dir(root.join("locks")).unwrap();
        for (path, contents) in [
            ("release.json", "{}"),
            ("Cargo.lock", "locked\n"),
            ("locks/source.json", "native\n"),
            ("README.md", "not a release input"),
        ] {
            fs::write(root.join(path), contents).unwrap();
        }
        git(root, &["add", "."]).unwrap();
        fs::write(root.join("untracked.lock"), "not tracked").unwrap();
        // Independently computed from the documented length-prefixed bytes.
        assert_eq!(
            input_hash(root).unwrap(),
            "ddae821a4ed41e7ef49c385fd0fae569f9ebd37c1989710cd8724a74eb1b4e49"
        );
        fs::write(root.join("README.md"), "still not a release input").unwrap();
        assert_eq!(
            input_hash(root).unwrap(),
            "ddae821a4ed41e7ef49c385fd0fae569f9ebd37c1989710cd8724a74eb1b4e49"
        );
    }
}
