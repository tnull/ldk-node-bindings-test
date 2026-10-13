use crate::{candidates, model::*, process::*};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Duration};

#[derive(Deserialize, Serialize)]
struct Preparation {
    schema: u32,
    repo: String,
    recipe: String,
    branch: String,
    previous_runs: Vec<u64>,
    phase: String,
    run_id: Option<u64>,
    candidate_id: Option<String>,
}
fn runs(repo: &str) -> Result<Vec<Value>> {
    Ok(serde_json::from_str(&output(Command::new("gh").args([
        "run",
        "list",
        "--repo",
        repo,
        "--workflow",
        "candidate.yml",
        "--event",
        "workflow_dispatch",
        "--limit",
        "100",
        "--json",
        "databaseId,headSha,displayTitle",
    ]))?)?)
}
pub fn validate_repo(repo: &str) -> Result<()> {
    let parts: Vec<_> = repo.split('/').collect();
    ensure!(parts.len() == 2, "repository must be OWNER/REPO");
    for part in parts {
        safe_name(part)?;
    }
    Ok(())
}
pub fn prepare(root: &Path, repo: &str) -> Result<()> {
    validate_repo(repo)?;
    let recipe = clean(root)?;
    load_inputs(root)?;
    let branch = git(root, &["symbolic-ref", "--short", "HEAD"])?;
    ensure!(
        preparation_branch(&branch),
        "preparation requires a dated YYYY-MM- branch"
    );
    let state = root.join(".release/prepare.json");
    fs::create_dir_all(state.parent().unwrap())?;
    let mut ledger = if state.exists() {
        let existing: Preparation = read_json(&state)?;
        ensure!(
            existing.schema == 1
                && existing.repo == repo
                && existing.recipe == recipe
                && existing.branch == branch,
            "preparation ledger belongs to different inputs; preserve it and use a fresh worktree"
        );
        existing
    } else {
        run(git_command().current_dir(root).args([
            "-c",
            "credential.helper=!gh auth git-credential",
            "push",
            &format!("https://github.com/{repo}.git"),
            &format!("HEAD:refs/heads/{branch}"),
        ]))?;
        let ledger = Preparation {
            schema: 1,
            repo: repo.into(),
            recipe: recipe.clone(),
            branch: branch.clone(),
            previous_runs: runs(repo)?
                .iter()
                .filter_map(|r| r["databaseId"].as_u64())
                .collect(),
            phase: "ready".into(),
            run_id: None,
            candidate_id: None,
        };
        write_json(&state, &ledger)?;
        ledger
    };
    if ledger.phase == "ready" {
        ledger.phase = "dispatching".into();
        write_json(&state, &ledger)?;
        run(Command::new("gh").args([
            "workflow",
            "run",
            "candidate.yml",
            "--repo",
            repo,
            "--ref",
            &branch,
            "--field",
            &format!("recipe_sha={recipe}"),
        ]))?;
    }
    if ledger.run_id.is_none() {
        for _ in 0..12 {
            let found: Vec<_> = runs(repo)?
                .into_iter()
                .filter(|r| {
                    r["headSha"].as_str() == Some(recipe.as_str())
                        && r["displayTitle"].as_str()
                            == Some(format!("candidate {recipe}").as_str())
                        && r["databaseId"]
                            .as_u64()
                            .is_some_and(|id| !ledger.previous_runs.contains(&id))
                })
                .collect();
            ensure!(
                found.len() <= 1,
                "multiple matching dispatches; reconcile the local preparation ledger manually"
            );
            if let Some(run) = found.first() {
                ledger.run_id = run["databaseId"].as_u64();
                ledger.phase = "running".into();
                write_json(&state, &ledger)?;
                break;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    let id = ledger.run_id.context(
        "dispatch outcome unknown; resume prepare to reconcile, do not dispatch another run",
    )?;
    run(Command::new("gh").args([
        "run",
        "watch",
        &id.to_string(),
        "--repo",
        repo,
        "--exit-status",
    ]))?;
    let run: Value = serde_json::from_str(&output(
        Command::new("gh").args(["api", &format!("repos/{repo}/actions/runs/{id}")]),
    )?)?;
    ensure!(
        run["head_sha"].as_str() == Some(&recipe) && run["conclusion"].as_str() == Some("success"),
        "workflow result does not match preparation"
    );
    let attempt = run["run_attempt"].as_u64().context("missing run attempt")?;
    let inputs = load_inputs(root)?;
    let candidate_id = format!(
        "candidate-{}-{}-{id}-{attempt}",
        inputs.bindings_version,
        &recipe[..12]
    );
    let url = format!("https://github.com/{repo}/releases/tag/{candidate_id}");
    let loaded = candidates::load(root, &url)?;
    let destination = root.join(".release/candidates").join(&candidate_id);
    if destination.exists() {
        ensure!(
            fs::symlink_metadata(&destination)?.file_type().is_dir(),
            "candidate cache must be a regular directory"
        );
        // Older/interrupted preparations may leave a subset of the files. Each
        // existing byte must agree before replacing that subset with a full cache.
        for entry in fs::read_dir(&destination)? {
            let entry = entry?;
            let expected = loaded.dir.join(entry.file_name());
            ensure!(expected.exists(), "unexpected cached candidate file");
            ensure!(
                hash_file(&entry.path())? == hash_file(&expected)?,
                "cached candidate differs: {}",
                entry.path().display()
            );
        }
    }
    let parent = destination.parent().context("candidate cache parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".candidate-staging-")
        .tempdir_in(parent)?;
    for entry in fs::read_dir(&loaded.dir)? {
        let entry = entry?;
        fs::copy(entry.path(), staging.path().join(entry.file_name()))?;
    }
    candidates::load(
        root,
        staging.path().to_str().context("candidate staging path")?,
    )?;
    let backup = if destination.exists() {
        let backup = tempfile::Builder::new()
            .prefix(".candidate-previous-")
            .tempdir_in(parent)?;
        fs::rename(&destination, backup.path())?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(staging.path(), &destination) {
        if let Some(backup) = backup {
            let previous = backup.keep();
            fs::rename(&previous, &destination).with_context(|| {
                format!(
                    "failed to restore candidate cache; previous files preserved at {}",
                    previous.display()
                )
            })?;
        }
        return Err(error.into());
    }
    ledger.phase = "downloaded".into();
    ledger.candidate_id = Some(candidate_id.clone());
    write_json(&state, &ledger)?;
    println!(
        "Candidates: {url}\nLocal files: {}\nPublication has not run. Verification coverage: none.",
        destination.display()
    );
    let manifest_sha256 = hash_file(&destination.join("candidate.json"))?.sha256;
    println!("candidate.json SHA-256: {manifest_sha256}");
    if repo == "lightningdevkit/ldk-node-bindings" {
        println!("./scripts/verify.sh {candidate_id} {manifest_sha256}");
    } else {
        for group in GROUPS {
            println!("nix run .#release -- verify --candidate {url} --group {group} --output /tmp/verify-{id}-{group}");
        }
    }
    if ledger.phase != "downloaded" {
        bail!("preparation incomplete");
    }
    Ok(())
}
