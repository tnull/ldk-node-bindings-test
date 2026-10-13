use crate::{model::*, process::*};
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn inventory(
    root: &Path,
    dir: &Path,
    recipe: &str,
    group: &str,
    import_dir: Option<&Path>,
    output: &Path,
) -> Result<()> {
    hex(recipe, 40)?;
    ensure!(clean(root)? == recipe, "recipe differs from HEAD");
    let inputs = load_inputs(root)?;
    let names = inputs.groups.get(group).context("unknown build group")?;
    exact_directory(dir, names.iter().cloned().collect())?;
    let mut files: Vec<_> = names
        .iter()
        .map(|n| hash_file(&dir.join(n)))
        .collect::<Result<_>>()?;
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let imports = if group == "jvm" {
        let dir = import_dir.context("JVM inventory requires --candidate native assets")?;
        JVM_NATIVE
            .iter()
            .map(|n| hash_file(&dir.join(n)))
            .collect::<Result<_>>()?
    } else {
        vec![]
    };
    write_json(
        output,
        &Inventory {
            schema: 1,
            recipe: recipe.into(),
            inputs_sha256: input_hash(root)?,
            group: group.into(),
            files,
            imports,
        },
    )
}
pub fn candidate(
    root: &Path,
    dir: &Path,
    recipe: &str,
    run_id: &str,
    attempt: &str,
    output: &Path,
) -> Result<()> {
    hex(recipe, 40)?;
    ensure!(clean(root)? == recipe, "recipe differs from HEAD");
    ensure!(
        [run_id, attempt]
            .iter()
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())),
        "run ID and attempt must be numeric"
    );
    let inputs = load_inputs(root)?;
    let mut groups = BTreeMap::new();
    for group in GROUPS {
        groups.insert(
            group.into(),
            read_json(&dir.join(format!("{group}.inventory.json")))?,
        );
    }
    let c = Candidate {
        schema: 1,
        id: format!(
            "candidate-{}-{}-{run_id}-{attempt}",
            inputs.bindings_version,
            &recipe[..12]
        ),
        version: inputs.bindings_version.clone(),
        native_rev: inputs.native.rev.clone(),
        recipe: recipe.into(),
        inputs_sha256: input_hash(root)?,
        groups,
    };
    validate_candidate(&c, &inputs, recipe, &input_hash(root)?)?;
    verify_directory(dir, &c)?;
    ensure!(
        !output.exists(),
        "candidate manifest already exists; never replace a candidate"
    );
    write_json(output, &c)?;
    let mut records: Vec<_> = c.groups.values().flat_map(|g| g.files.clone()).collect();
    records.push(hash_file(output)?);
    records.sort_by(|a, b| a.name.cmp(&b.name));
    fs::write(
        dir.join("checksums.txt"),
        records
            .iter()
            .map(|f| format!("{}  {}\n", f.sha256, f.name))
            .collect::<String>(),
    )?;
    println!("{}", c.id);
    Ok(())
}
fn exact_directory(dir: &Path, allowed: BTreeSet<String>) -> Result<()> {
    ensure!(
        fs::symlink_metadata(dir)?.file_type().is_dir(),
        "artifact directory must be a regular directory"
    );
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .to_str()
            .context("invalid filename")?
            .to_owned();
        ensure!(allowed.contains(&name), "unexpected file: {name}");
        ensure!(
            entry.file_type()?.is_file(),
            "artifact must be a regular file: {name}"
        );
    }
    Ok(())
}
fn verify_directory(dir: &Path, c: &Candidate) -> Result<()> {
    let mut allowed: BTreeSet<_> = c
        .groups
        .values()
        .flat_map(|g| g.files.iter().map(|f| f.name.clone()))
        .collect();
    allowed.extend(["candidate.json".into(), "checksums.txt".into()]);
    for group in c.groups.keys() {
        allowed.insert(format!("{group}.inventory.json"));
    }
    exact_directory(dir, allowed)?;
    for (group, inv) in &c.groups {
        check_files(dir, &inv.files)?;
        let path = dir.join(format!("{group}.inventory.json"));
        if path.exists() {
            ensure!(
                read_json::<Inventory>(&path)? == *inv,
                "inventory sidecar differs from candidate"
            );
        }
    }
    Ok(())
}
pub struct Loaded {
    pub dir: PathBuf,
    pub candidate: Candidate,
    _temporary: Option<tempfile::TempDir>,
}
pub fn github_download_base(source: &str) -> Result<String> {
    let parts: Vec<_> = source.trim_end_matches('/').split('/').collect();
    ensure!(
        parts.len() == 8
            && parts[0] == "https:"
            && parts[1].is_empty()
            && parts[2] == "github.com"
            && parts[5] == "releases"
            && ["tag", "download"].contains(&parts[6]),
        "candidate URL must be a public GitHub release tag or download URL"
    );
    for i in [3, 4, 7] {
        safe_name(parts[i])?;
    }
    Ok(format!(
        "https://github.com/{}/{}/releases/download/{}",
        parts[3], parts[4], parts[7]
    ))
}
pub fn load(root: &Path, source: &str) -> Result<Loaded> {
    let (dir, temporary) = if source.starts_with("https://") {
        let tmp = tempfile::tempdir()?;
        download(
            &format!("{}/candidate.json", github_download_base(source)?),
            &tmp.path().join("candidate.json"),
        )?;
        (tmp.path().to_owned(), Some(tmp))
    } else {
        (fs::canonicalize(source)?, None)
    };
    let c: Candidate = read_json(&dir.join("candidate.json"))?;
    let recipe = git(root, &["rev-parse", "HEAD"])?;
    hex(&recipe, 40)?;
    validate_candidate(&c, &load_inputs(root)?, &recipe, &input_hash(root)?)?;
    if temporary.is_some() {
        let base = github_download_base(source)?;
        ensure!(
            base.ends_with(&format!("/{}", c.id)),
            "candidate URL tag differs from manifest ID"
        );
        download_assets(&base, &dir, &c)?;
    }
    verify_directory(&dir, &c)?;
    Ok(Loaded {
        dir,
        candidate: c,
        _temporary: temporary,
    })
}
pub fn verify(root: &Path, source: &str, group: &str, output: &Path) -> Result<()> {
    clean(root)?;
    let loaded = load(root, source)?;
    ensure!(
        !output.exists() || fs::read_dir(output)?.next().is_none(),
        "verification output must be empty"
    );
    verify_loaded(root, &loaded, group, output, output)?;
    report_groups(&loaded.candidate, &[group]);
    Ok(())
}

fn download_assets(base: &str, dir: &Path, candidate: &Candidate) -> Result<()> {
    for file in candidate.groups.values().flat_map(|g| &g.files) {
        safe_name(&file.name)?;
        download(&format!("{base}/{}", file.name), &dir.join(&file.name))?;
    }
    Ok(())
}

fn anonymous_git() -> Command {
    let mut command = git_command();
    command
        .args(["-c", "core.hooksPath=/dev/null"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN");
    command
}

pub fn verify_candidate(
    id: &str,
    sha256: &str,
    repo: &str,
    group: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    crate::prepare::validate_repo(repo)?;
    safe_name(id)?;
    ensure!(
        id.starts_with("candidate-"),
        "expected a candidate release tag"
    );
    hex(sha256, 64)?;
    let supported = host_groups(std::env::consts::OS, std::env::consts::ARCH)?;
    let groups = if let Some(group) = group {
        ensure!(
            supported.contains(&group),
            "group {group} is not supported by this host"
        );
        vec![group]
    } else {
        supported.to_vec()
    };
    let output = if let Some(output) = output {
        ensure!(
            !output.exists() || fs::read_dir(output)?.next().is_none(),
            "verification output must be empty"
        );
        fs::create_dir_all(output)?;
        fs::canonicalize(output)?
    } else {
        tempfile::Builder::new()
            .prefix("bindings-candidate-")
            .tempdir_in("/tmp")?
            .keep()
    };
    eprintln!("Verification files and diagnostics: {}", output.display());
    let dir = output.join("candidate");
    fs::create_dir(&dir)?;
    let base = format!("https://github.com/{repo}/releases/download/{id}");
    let manifest = dir.join("candidate.json");
    download(&format!("{base}/candidate.json"), &manifest)?;
    ensure!(
        hash_file(&manifest)?.sha256 == sha256,
        "candidate manifest SHA-256 mismatch"
    );
    let candidate: Candidate = read_json(&manifest)?;
    ensure!(candidate.id == id, "candidate tag differs from manifest ID");
    validate_candidate_id(&candidate)?;

    let checkout = output.join("recipe");
    // Begin with no local refs or shallow boundaries. Fetch branch history from
    // the explicitly trusted repository, never an arbitrary SHA or candidate tag.
    run(anonymous_git()
        .args(["init", "--quiet", "--template="])
        .arg(&checkout))?;
    run(anonymous_git().current_dir(&checkout).args([
        "fetch",
        "--quiet",
        "--no-tags",
        &format!("https://github.com/{repo}.git"),
        "+refs/heads/*:refs/remotes/reviewed/*",
    ]))?;
    let containing = anonymous_git()
        .current_dir(&checkout)
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "--contains",
            &candidate.recipe,
            "refs/remotes/reviewed/",
        ])
        .output()?;
    ensure!(
        containing.status.success() && std::str::from_utf8(&containing.stdout)?.lines()
            .filter_map(|name| name.strip_prefix("refs/remotes/reviewed/"))
            .any(release_branch),
        "recipe is not reachable from an official release branch (main, MAJOR.MINOR.x, or YYYY-MM-* preparation branch)"
    );
    run(anonymous_git().current_dir(&checkout).args([
        "checkout",
        "--quiet",
        "--detach",
        &candidate.recipe,
    ]))?;
    validate_candidate(
        &candidate,
        &load_inputs(&checkout)?,
        &candidate.recipe,
        &input_hash(&checkout)?,
    )?;
    download_assets(&base, &dir, &candidate)?;
    verify_directory(&dir, &candidate)?;
    let loaded = Loaded {
        dir,
        candidate,
        _temporary: None,
    };
    let receipts = output.join("receipts");
    let mut verified = Vec::new();
    let mut failures = Vec::new();
    for group in groups {
        eprintln!("Rebuilding {group}...");
        match verify_loaded(
            &checkout,
            &loaded,
            group,
            &receipts,
            &output.join("diagnostics").join(group),
        ) {
            Ok(()) => verified.push(group),
            Err(error) => failures.push(format!("{group}: {error:#}")),
        }
    }
    report_groups(&loaded.candidate, &verified);
    println!("Receipts: {}", receipts.display());
    ensure!(
        failures.is_empty(),
        "verification failed:\n{}",
        failures.join("\n")
    );
    Ok(())
}

fn docker_endpoint() -> Result<Option<String>> {
    let engine = std::env::var_os("CONTAINER_ENGINE")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "docker".into());
    if output(Command::new(&engine).arg("--version"))?
        .to_ascii_lowercase()
        .contains("podman")
    {
        return Ok(None);
    }
    let context = std::env::var("DOCKER_CONTEXT").unwrap_or_default();
    let explicit_host = std::env::var("DOCKER_HOST").unwrap_or_default();
    let host = if context.is_empty() && !explicit_host.is_empty() {
        explicit_host
    } else {
        output(Command::new(&engine).args([
            "context",
            "inspect",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ]))?
    };
    ensure!(
        host.starts_with("unix:///"),
        "Linux verification requires a local Docker Unix socket; select a local context or use CONTAINER_ENGINE=podman"
    );
    Ok(Some(host))
}

fn verify_loaded(
    root: &Path,
    loaded: &Loaded,
    group: &str,
    output: &Path,
    diagnostics: &Path,
) -> Result<()> {
    let recipe = clean(root)?;
    let c = &loaded.candidate;
    let inv = c.groups.get(group).context("unknown verification group")?;
    ensure!(
        !output.join(format!("{group}.receipt.json")).exists(),
        "receipt already exists"
    );
    fs::create_dir_all(output)?;
    let work = tempfile::Builder::new()
        .prefix("bindings-verify-")
        .tempdir_in("/tmp")?;
    let result = (|| -> Result<()> {
        let checkout = work.path().join("recipe");
        run(anonymous_git()
            .args([
                "clone",
                "--quiet",
                "--no-hardlinks",
                "--no-checkout",
                "--template=",
            ])
            .arg(root)
            .arg(&checkout))?;
        run(anonymous_git()
            .current_dir(&checkout)
            .args(["checkout", "--quiet", "--detach", &recipe]))?;
        let built = work.path().join("output");
        let home = work.path().join("home");
        fs::create_dir(&home)?;
        let mut command = Command::new("bash");
        command
            .current_dir(&checkout)
            .arg("scripts/build.sh")
            .args(["--group", group, "--output"])
            .arg(&built);
        if group == "jvm" {
            command.arg("--candidate").arg(&loaded.dir);
        }
        command.env_clear();
        for key in [
            "PATH",
            "SSL_CERT_FILE",
            "NIX_SSL_CERT_FILE",
            "NIX_REMOTE",
            "TMPDIR",
            "CONTAINER_ENGINE",
            "DOCKER_HOST",
            "DOCKER_CONTEXT",
            "XDG_RUNTIME_DIR",
            "DEVELOPER_DIR",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        if group.starts_with("linux-") {
            if let Some(host) = docker_endpoint()? {
                // Resolve under the caller's HOME, then carry only the socket
                // into isolation, without context files or registry credentials.
                command
                    .env("DOCKER_HOST", host)
                    .env_remove("DOCKER_CONTEXT");
            }
        }
        command
            .env("HOME", &home)
            .env("RELEASE_VERIFY", "1")
            .env("CARGO_TARGET_DIR", work.path().join("cargo-target"))
            .env("CARGO_HOME", work.path().join("cargo-home"));
        run(&mut command)?;
        exact_directory(&built, inv.files.iter().map(|f| f.name.clone()).collect())?;
        check_files(&built, &inv.files)?;
        Ok(())
    })();
    if let Err(error) = result {
        let preserved = work.keep();
        eprintln!(
            "Verification failed; preserved work at {}",
            preserved.display()
        );
        let expected = preserved.join("expected");
        fs::create_dir(&expected)?;
        for file in &inv.files {
            fs::copy(loaded.dir.join(&file.name), expected.join(&file.name))?;
        }
        write_json(
            &diagnostics.join("failure.json"),
            &serde_json::json!({
                "schema": 1, "candidate_id": c.id, "group": group,
                "work_dir": preserved, "error": format!("{error:#}"), "expected_files": inv.files
            }),
        )?;
        return Err(error.context(format!(
            "verification failed; preserved work at {} (see {}/failure.json)",
            preserved.display(),
            diagnostics.display()
        )));
    }
    let receipt = Receipt {
        schema: 1,
        candidate_id: c.id.clone(),
        recipe: c.recipe.clone(),
        inputs_sha256: c.inputs_sha256.clone(),
        group: group.into(),
        files: inv.files.clone(),
        imports: inv.imports.clone(),
        method: "independent-rebuild".into(),
        host: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    };
    write_json(&output.join(format!("{group}.receipt.json")), &receipt)?;
    Ok(())
}
fn report_groups(c: &Candidate, groups: &[&str]) {
    println!(
        "Candidate {}. Verified groups: {}. Outstanding groups: {}",
        c.id,
        if groups.is_empty() {
            "none".into()
        } else {
            groups.join(", ")
        },
        c.groups
            .keys()
            .filter(|g| !groups.contains(&g.as_str()))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
}
pub fn collect(root: &Path, source: &str, receipts: &Path, output: &Path) -> Result<()> {
    clean(root)?;
    let loaded = load(root, source)?;
    let c = &loaded.candidate;
    let mut accepted = vec![];
    for entry in fs::read_dir(receipts)? {
        let path = entry?.path();
        ensure!(
            path.extension().and_then(|s| s.to_str()) == Some("json"),
            "receipt directory contains a non-JSON file"
        );
        accepted.push(read_json::<Receipt>(&path)?);
    }
    accepted.sort_by(|a, b| a.group.cmp(&b.group));
    validate_receipts(c, &accepted)?;
    write_json(
        output,
        &Verified {
            schema: 1,
            candidate_id: c.id.clone(),
            candidate_sha256: hash_file(&loaded.dir.join("candidate.json"))?.sha256,
            recipe: c.recipe.clone(),
            inputs_sha256: c.inputs_sha256.clone(),
            receipts: accepted,
        },
    )?;
    println!("Complete independent coverage accepted for {}", c.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::host_groups;

    #[test]
    fn verification_groups_match_supported_build_hosts() {
        assert_eq!(
            host_groups("linux", "x86_64").unwrap(),
            ["generated", "linux-x86_64", "android", "jvm"]
        );
        assert_eq!(
            host_groups("linux", "aarch64").unwrap(),
            ["linux-aarch64", "jvm"]
        );
        assert_eq!(host_groups("macos", "aarch64").unwrap(), ["darwin"]);
        assert!(host_groups("macos", "x86_64").is_err());
        assert!(host_groups("windows", "x86_64").is_err());
    }
}
