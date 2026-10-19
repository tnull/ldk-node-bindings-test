use crate::{candidates, model::*, process::*};
use anyhow::{bail, ensure, Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
mod swift;

#[derive(clap::Args)]
pub struct SwiftOptions {
    #[arg(long, default_value = "lightningdevkit/ldk-node-swift")]
    pub repo: String,
    #[arg(long)]
    pub swift_git_url: Option<String>,
    #[arg(long)]
    pub swift_asset_base_url: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Destination {
    Testpypi,
    Pypi,
    Maven,
    Swift,
}
impl Destination {
    fn name(self) -> &'static str {
        match self {
            Self::Testpypi => "testpypi",
            Self::Pypi => "pypi",
            Self::Maven => "maven",
            Self::Swift => "swift",
        }
    }
}
#[derive(Deserialize, Serialize)]
struct Publication {
    schema: u32,
    candidate_id: String,
    candidate_sha256: String,
    destinations: BTreeMap<String, Value>,
}
struct Session<'a> {
    candidate: &'a Candidate,
    dir: &'a Path,
    state_path: PathBuf,
    state: Publication,
}
impl Session<'_> {
    fn save(&mut self, destination: &str, value: Value) -> Result<()> {
        self.state.destinations.insert(destination.into(), value);
        write_json(&self.state_path, &self.state)
    }
    fn rehash(&self) -> Result<()> {
        for inv in self.candidate.groups.values() {
            check_files(self.dir, &inv.files)?;
        }
        Ok(())
    }
}
pub fn publish(
    root: &Path,
    verified: &Path,
    source: &str,
    destination: Destination,
    confirm: &str,
    swift_options: &SwiftOptions,
) -> Result<()> {
    ensure!(
        [
            "CI",
            "GITHUB_ACTIONS",
            "GITLAB_CI",
            "JENKINS_URL",
            "BUILD_BUILDID"
        ]
        .iter()
        .all(|v| std::env::var_os(v).is_none()),
        "publication must run on a local machine"
    );
    clean(root)?;
    let loaded = candidates::load(root, source)?;
    ensure!(
        loaded.candidate.id == confirm,
        "candidate-specific confirmation does not match"
    );
    let ledger: Verified = read_json(verified)?;
    let c = &loaded.candidate;
    ensure!(
        ledger.schema == 1
            && ledger.candidate_id == c.id
            && ledger.recipe == c.recipe
            && ledger.inputs_sha256 == c.inputs_sha256
            && ledger.candidate_sha256 == hash_file(&loaded.dir.join("candidate.json"))?.sha256,
        "verified ledger identity differs"
    );
    validate_receipts(c, &ledger.receipts)?;
    let snapshot = tempfile::Builder::new()
        .prefix("bindings-publish-")
        .tempdir_in("/tmp")?;
    for file in c.groups.values().flat_map(|g| &g.files) {
        fs::copy(
            loaded.dir.join(&file.name),
            snapshot.path().join(&file.name),
        )?;
    }
    let state_path = verified
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("publication-{}.json", c.id));
    // Keep the file in place: removing it could let another process lock a
    // different inode. The OS releases the lock even after a crash or SIGKILL.
    let publication_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(state_path.with_extension("lock"))?;
    publication_lock
        .try_lock()
        .context("another publication is using this candidate ledger; retry after it exits")?;
    let state = if state_path.exists() {
        let state: Publication = read_json(&state_path)?;
        ensure!(
            state.schema == 1
                && state.candidate_id == c.id
                && state.candidate_sha256 == ledger.candidate_sha256,
            "publication ledger identity differs"
        );
        state
    } else {
        Publication {
            schema: 1,
            candidate_id: c.id.clone(),
            candidate_sha256: ledger.candidate_sha256,
            destinations: BTreeMap::new(),
        }
    };
    let mut session = Session {
        candidate: c,
        dir: snapshot.path(),
        state_path,
        state,
    };
    session.rehash()?;
    match destination {
        Destination::Testpypi | Destination::Pypi => python(&mut session, destination)?,
        Destination::Maven => maven(&mut session)?,
        Destination::Swift => swift::publish(root, &mut session, swift_options)?,
    }
    println!(
        "{} publication completed for {}. Ledger: {}",
        destination.name(),
        c.id,
        session.state_path.display()
    );
    Ok(())
}
fn remote_json(url: &str) -> Result<Option<Value>> {
    remote_text(url)?
        .map(|body| serde_json::from_str(&body).map_err(Into::into))
        .transpose()
}
fn python_files(c: &Candidate) -> Vec<FileRecord> {
    c.groups
        .values()
        .flat_map(|g| g.files.iter())
        .filter(|f| f.name.ends_with(".whl"))
        .cloned()
        .collect()
}
fn python_remote(index: &str, version: &str) -> Result<BTreeMap<String, String>> {
    let value = remote_json(&format!("https://{index}/pypi/ldk-node/{version}/json"))?;
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let mut files = BTreeMap::new();
    for file in value["urls"]
        .as_array()
        .context("registry response lacks files")?
    {
        let name = file["filename"].as_str().context("registry filename")?;
        let hash = file["digests"]["sha256"]
            .as_str()
            .context("registry digest")?;
        ensure!(
            files.insert(name.into(), hash.into()).is_none(),
            "registry has duplicate filenames"
        );
    }
    Ok(files)
}
fn python_consumer(program: impl AsRef<std::ffi::OsStr>, home: &Path) -> Command {
    let mut command = Command::new(program);
    command.env_clear().current_dir(home).env("HOME", home);
    // Keep transport configuration, but never inherit resolver settings,
    // credentials or Python import paths into the staged-wheel check.
    for key in [
        "PATH",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
}
fn python(session: &mut Session<'_>, destination: Destination) -> Result<()> {
    let files = python_files(session.candidate);
    ensure!(!files.is_empty(), "candidate has no wheels");
    if matches!(destination, Destination::Pypi) {
        let test = python_remote("test.pypi.org", &session.candidate.version)?;
        ensure!(
            files.iter().all(|f| test.get(&f.name) == Some(&f.sha256)),
            "TestPyPI does not contain every identical candidate wheel"
        );
        let env = tempfile::tempdir()?;
        let requirements = env.path().join("requirements.txt");
        let mut requirement = format!("ldk-node=={}", session.candidate.version);
        for file in &files {
            requirement.push_str(&format!(" --hash=sha256:{}", file.sha256));
        }
        requirement.push('\n');
        fs::write(&requirements, requirement)?;
        run(python_consumer("uv", env.path())
            .args(["--no-config", "--no-cache", "venv", "--python", "3.10"])
            .arg(env.path().join("venv")))?;
        let interpreter = env.path().join("venv/bin/python");
        run(python_consumer("uv", env.path())
            .args(["--no-config", "--no-cache", "pip", "install", "--python"])
            .arg(&interpreter)
            .args([
                "--index-url",
                "https://test.pypi.org/simple/",
                "--no-deps",
                "--only-binary",
                ":all:",
                "--require-hashes",
                "--requirements",
            ])
            .arg(&requirements))?;
        run(python_consumer(&interpreter, env.path()).args([
            "-I",
            "-c",
            "import ldk_node; assert callable(ldk_node.Builder)",
        ]))?;
    }
    let (host, upload_host) = if matches!(destination, Destination::Testpypi) {
        ("test.pypi.org", "test.pypi.org")
    } else {
        ("pypi.org", "upload.pypi.org")
    };
    let existing = python_remote(host, &session.candidate.version)?;
    for (name, hash) in &existing {
        let expected = files
            .iter()
            .find(|f| f.name == *name)
            .context("registry version contains an unexpected file; refusing to extend it")?;
        ensure!(
            expected.sha256 == *hash,
            "registry already contains different bytes for {name}"
        );
    }
    let missing: Vec<_> = files
        .iter()
        .filter(|f| !existing.contains_key(&f.name))
        .collect();
    if !missing.is_empty() {
        ensure!(
            std::env::var_os("UV_PUBLISH_TOKEN").is_some(),
            "set local UV_PUBLISH_TOKEN for this destination"
        );
        session.save(
            destination.name(),
            json!({"status":"uploading","existing":existing.keys().collect::<Vec<_>>()}),
        )?;
        for file in missing {
            session.rehash()?;
            run(Command::new("uv")
                .args([
                    "publish",
                    "--publish-url",
                    &format!("https://{upload_host}/legacy/"),
                ])
                .arg(session.dir.join(&file.name)))?;
            session.save(
                destination.name(),
                json!({"status":"uploading","last_uploaded":file.name}),
            )?;
        }
    }
    let final_files = python_remote(host, &session.candidate.version)?;
    ensure!(
        files
            .iter()
            .all(|f| final_files.get(&f.name) == Some(&f.sha256)),
        "registry has not confirmed every uploaded digest; resume to reconcile"
    );
    session.save(
        destination.name(),
        json!({"status":"published","files":files}),
    )
}
fn maven_token() -> Result<String> {
    if let Ok(token) = std::env::var("MAVEN_CENTRAL_TOKEN") {
        return Ok(token);
    }
    let username = std::env::var("CENTRAL_TOKEN_USERNAME")
        .context("set MAVEN_CENTRAL_TOKEN or CENTRAL_TOKEN_USERNAME/PASSWORD locally")?;
    let password =
        std::env::var("CENTRAL_TOKEN_PASSWORD").context("set CENTRAL_TOKEN_PASSWORD locally")?;
    let mut child = Command::new("base64")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("base64 stdin")?
        .write_all(format!("{username}:{password}").as_bytes())?;
    let out = child.wait_with_output()?;
    ensure!(out.status.success(), "cannot encode Maven token");
    Ok(String::from_utf8(out.stdout)?.split_whitespace().collect())
}
fn maven_path(file: &FileRecord, version: &str) -> Result<String> {
    let artifact = if file.name.starts_with("ldk-node-jvm-") {
        "ldk-node-jvm"
    } else if file.name.starts_with("ldk-node-android-") {
        "ldk-node-android"
    } else {
        bail!("unexpected Maven artifact filename")
    };
    Ok(format!(
        "org/lightningdevkit/{artifact}/{version}/{}",
        file.name
    ))
}
fn maven(session: &mut Session<'_>) -> Result<()> {
    let token = maven_token()?;
    let files: Vec<_> = ["android", "jvm"]
        .iter()
        .flat_map(|group| session.candidate.groups[*group].files.clone())
        .collect();
    let previous = session.state.destinations.get("maven").cloned();
    if previous
        .as_ref()
        .is_some_and(|v| v["status"] == "uploading" && v["deployment_id"].is_null())
    {
        bail!("Maven upload outcome is unknown: find the deployment in Central Portal and record its deployment_id in the local publication ledger before resuming");
    }
    let deployment = if let Some(id) = previous.as_ref().and_then(|v| v["deployment_id"].as_str()) {
        id.to_owned()
    } else {
        for file in &files {
            let url = format!(
                "https://repo1.maven.org/maven2/{}.sha256",
                maven_path(file, &session.candidate.version)?
            );
            ensure!(
                remote_text(&url)?.is_none(),
                "Maven version already exists; reconcile remote publication instead of uploading"
            );
        }
        let bundle = tempfile::Builder::new()
            .prefix("bindings-maven-")
            .tempdir_in("/tmp")?;
        for file in &files {
            let path = bundle
                .path()
                .join(maven_path(file, &session.candidate.version)?);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::copy(session.dir.join(&file.name), &path)?;
            ensure!(
                hash_file(&path)? == *file,
                "artifact differs before Maven signing"
            );
            let mut sign = Command::new("gpg");
            sign.args(["--armor", "--detach-sign"]);
            if let Ok(key) = std::env::var("GPG_KEY_ID") {
                sign.arg("--local-user").arg(key);
            }
            sign.arg("--output")
                .arg(format!("{}.asc", path.display()))
                .arg(&path);
            run(&mut sign)?;
            for suffix in ["", ".asc"] {
                let payload = PathBuf::from(format!("{}{suffix}", path.display()));
                for (command, extension) in [
                    ("md5sum", "md5"),
                    ("sha1sum", "sha1"),
                    ("sha256sum", "sha256"),
                    ("sha512sum", "sha512"),
                ] {
                    let sum = output(Command::new(command).arg(&payload))?
                        .split_whitespace()
                        .next()
                        .context("checksum output")?
                        .to_owned();
                    fs::write(
                        format!("{}.{}", payload.display(), extension),
                        format!("{sum}\n"),
                    )?;
                }
            }
        }
        let zip = bundle.path().join("central-bundle.zip");
        run(Command::new("zip")
            .current_dir(bundle.path())
            .args(["-q", "-r"])
            .arg(&zip)
            .arg("org"))?;
        session.rehash()?;
        session.save("maven", json!({"status":"uploading","deployment_id":null}))?;
        let id = authenticated_curl(
            "https://central.sonatype.com/api/v1/publisher/upload?publishingType=USER_MANAGED",
            &token,
            &[
                "--request".into(),
                "POST".into(),
                "--form".into(),
                format!("bundle=@{}", zip.display()),
            ],
        )?;
        safe_name(&id)?;
        session.save("maven", json!({"status":"uploaded","deployment_id":id}))?;
        id
    };
    safe_name(&deployment)?;
    for _ in 0..120 {
        let status: Value = serde_json::from_str(&authenticated_curl(
            &format!("https://central.sonatype.com/api/v1/publisher/status?id={deployment}"),
            &token,
            &["--request".into(), "POST".into()],
        )?)?;
        let phase = status["deploymentState"]
            .as_str()
            .context("missing Maven deployment state")?;
        session.save(
            "maven",
            json!({"status":phase,"deployment_id":deployment,"response":status}),
        )?;
        match phase {
            "PUBLISHED" => return Ok(()),
            "VALIDATED" => {
                session.rehash()?;
                authenticated_curl(
                    &format!(
                        "https://central.sonatype.com/api/v1/publisher/deployment/{deployment}"
                    ),
                    &token,
                    &["--request".into(), "POST".into()],
                )?;
            }
            "PENDING" | "VALIDATING" | "PUBLISHING" => {}
            "FAILED" => bail!(
                "Maven validation failed; details preserved in {}",
                session.state_path.display()
            ),
            _ => bail!("unknown Maven deployment state"),
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    bail!("Maven deployment is still processing; repeat the explicit publish command to resume")
}
fn remote_text(url: &str) -> Result<Option<String>> {
    let result = Command::new("curl")
        .arg("--disable")
        .args([
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
            "120",
            "--write-out",
            "\n%{http_code}",
            url,
        ])
        .output()?;
    ensure!(
        result.status.success(),
        "registry query failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response = String::from_utf8(result.stdout)?;
    let (body, status) = response.rsplit_once('\n').context("missing HTTP status")?;
    match status {
        "200" => Ok(Some(body.into())),
        "404" => Ok(None),
        _ => bail!("remote registry query failed: HTTP {status}"),
    }
}
