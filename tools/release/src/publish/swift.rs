use super::*;

fn git_command() -> Command {
    let mut command = crate::process::git_command();
    command.args(["-c", "core.hooksPath=/dev/null"]);
    command
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    output(git_command().current_dir(root).args(args))
}

pub(super) fn publish(
    root: &Path,
    session: &mut Session<'_>,
    options: &SwiftOptions,
) -> Result<()> {
    let repo = &options.repo;
    crate::prepare::validate_repo(repo)?;
    let git_url = options
        .swift_git_url
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{repo}.git"));
    ensure!(
        !git_url.starts_with('-') && !git_url.contains('\n'),
        "invalid Swift Git endpoint"
    );
    let asset_base = options
        .swift_asset_base_url
        .clone()
        .unwrap_or_else(|| format!("https://github.com/{repo}/releases/download"));
    ensure!(
        asset_base.starts_with("https://")
            && asset_base
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":/._-".contains(&b)),
        "invalid Swift asset endpoint"
    );
    let version = &session.candidate.version;
    let tag = format!("v{version}");
    let zip = session.candidate.groups["darwin"]
        .files
        .iter()
        .find(|f| f.name == "LDKNodeFFI.xcframework.zip")
        .context("missing Swift ZIP")?
        .clone();
    let url = format!("{}/{tag}/{}", asset_base.trim_end_matches('/'), zip.name);
    let prior = session.state.destinations.get("swift").cloned();
    if let Some(prior) = &prior {
        ensure!(
            prior["repo"] == *repo
                && prior["git_url"] == git_url
                && prior["asset_base_url"] == asset_base,
            "Swift publication endpoints differ from the resumable ledger"
        );
    }
    let work = tempfile::Builder::new()
        .prefix("bindings-swift-")
        .tempdir_in("/tmp")?;
    let tree = work.path().join("distribution");
    run(git_command()
        .args([
            "-c",
            "credential.helper=!gh auth git-credential",
            "clone",
            "--quiet",
            &git_url,
        ])
        .arg(&tree))?;
    let desired = distribution_files(root, session, &url, &zip)?;
    let public = remote_json(&format!(
        "https://api.github.com/repos/{repo}/releases/tags/{tag}"
    ))?;
    if let Some(release) = public {
        ensure!(release["draft"] == false, "unexpected public draft state");
        let downloaded = tempfile::NamedTempFile::new()?;
        download(&url, downloaded.path())?;
        ensure!(
            digest(&fs::read(downloaded.path())?) == zip.sha256,
            "existing Swift release differs"
        );
        let recorded = prior
            .as_ref()
            .and_then(|s| s["commit"].as_str())
            .context("existing Swift release has no matching local publication record")?;
        let remote = git(root, &["ls-remote", &git_url, &format!("refs/tags/{tag}")])?;
        ensure!(
            remote.split_whitespace().next() == Some(recorded),
            "Swift tag identity differs"
        );
        git(&tree, &["checkout", "--quiet", "--detach", recorded])?;
        assert_distribution(&tree, &desired)?;
        return session.save("swift",json!({"status":"published","repo":repo,"git_url":git_url,"asset_base_url":asset_base,"commit":recorded}));
    }
    if prior.is_none() {
        ensure!(
            git(root, &["ls-remote", &git_url, &format!("refs/tags/{tag}")])?.is_empty(),
            "Swift tag already exists; refusing to move it"
        );
    }
    let month = output(Command::new("date").args(["-u", "+%Y-%m"]))?;
    let branch = prior
        .as_ref()
        .and_then(|p| p["branch"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{month}-swift-{version}"));
    let commit = if let Some(commit) = prior.as_ref().and_then(|p| p["commit"].as_str()) {
        git(&tree, &["checkout", "--quiet", "--detach", commit])?;
        assert_distribution(&tree, &desired)?;
        commit.to_owned()
    } else {
        let existing = git(
            &tree,
            &["ls-remote", "origin", &format!("refs/heads/{branch}")],
        )?;
        if let Some(commit) = existing.split_whitespace().next() {
            git(&tree, &["checkout", "--quiet", "--detach", commit])?;
            assert_distribution(&tree, &desired)?;
            commit.to_owned()
        } else {
            run(git_command()
                .current_dir(&tree)
                .args(["checkout", "--orphan", &branch]))?;
            // An orphan tag tree contains only distribution inputs; it never imports
            // the bindings recipe repository or unrelated existing source files.
            let tracked = git(&tree, &["ls-files", "-z"])?;
            for path in tracked.split('\0').filter(|p| !p.is_empty()) {
                let file = tree.join(path);
                if fs::symlink_metadata(&file).is_ok() {
                    fs::remove_file(file)?;
                }
            }
            git(&tree, &["read-tree", "--empty"])?;
            for (name, bytes) in &desired {
                let path = tree.join(name);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, bytes)?;
            }
            let mut add = git_command();
            add.current_dir(&tree).arg("add").args(desired.keys());
            run(&mut add)?;
            let message = work.path().join("message");
            fs::write(&message,"Publish verified Swift package metadata\n\nSwift consumers need a version tag containing generated bindings and a\nchecksum for the independently reproduced framework.\n\nPublish the matching source, licenses, and provenance in the Swift\ndistribution repository before creating its immutable version tag.\n\nCo-Authored-By: HAL 9000\n")?;
            run(git_command()
                .current_dir(&tree)
                .args(["commit", "--file"])
                .arg(&message))?;
            let commit = git(&tree, &["rev-parse", "HEAD"])?;
            assert_distribution(&tree, &desired)?;
            run(git_command().current_dir(&tree).args([
                "-c",
                "credential.helper=!gh auth git-credential",
                "push",
                "origin",
                &format!("{commit}:refs/heads/{branch}"),
            ]))?;
            commit
        }
    };
    // Read authenticated remote state, including drafts, on every invocation.
    // A saved intent does not prove that its network request actually succeeded.
    let pages: Value = serde_json::from_str(&output(Command::new("gh").args([
        "api",
        &format!("repos/{repo}/releases"),
        "--paginate",
        "--slurp",
    ]))?)?;
    let mut releases = Vec::new();
    for page in pages.as_array().context("invalid GitHub release pages")? {
        for release in page.as_array().context("invalid GitHub release list")? {
            if release["tag_name"].as_str() == Some(&tag) {
                releases.push(release);
            }
        }
    }
    ensure!(
        releases.len() <= 1,
        "multiple remote releases use the Swift tag"
    );
    if let Some(release) = releases.first() {
        ensure!(
            release["draft"] == true,
            "Swift release already published; reconcile remote state before retrying"
        );
    } else {
        session.save("swift",json!({"status":"creating_draft","repo":repo,"git_url":git_url,"asset_base_url":asset_base,"commit":commit,"branch":branch}))?;
        run(Command::new("gh").args([
            "release",
            "create",
            &tag,
            "--repo",
            repo,
            "--draft",
            "--target",
            &commit,
            "--title",
            &format!("LDK Node {version}"),
                "--notes",
                &format!(
                    "Bindings version: {version}\nNative revision: {}\nBuild recipe: {}\nVerified candidate: {}",
                    session.candidate.native_rev, session.candidate.recipe, session.candidate.id
                ),
        ]))?;
    }
    let draft: Value = serde_json::from_str(&output(Command::new("gh").args([
        "release",
        "view",
        &tag,
        "--repo",
        repo,
        "--json",
        "isDraft,assets",
    ]))?)?;
    ensure!(
        draft["isDraft"] == true,
        "Swift release must remain a draft until its metadata commit is tagged"
    );
    let assets = draft["assets"].as_array().context("Swift draft assets")?;
    ensure!(
        assets.len() <= 1
            && assets
                .iter()
                .all(|asset| asset["name"].as_str() == Some(&zip.name)),
        "Swift draft artifact set differs"
    );
    if assets.is_empty() {
        session.rehash()?;
        run(Command::new("gh")
            .args(["release", "upload", &tag, "--repo", repo])
            .arg(session.dir.join(&zip.name)))?;
    }
    let retrieved = tempfile::tempdir()?;
    run(Command::new("gh")
        .args([
            "release",
            "download",
            &tag,
            "--repo",
            repo,
            "--pattern",
            &zip.name,
            "--dir",
        ])
        .arg(retrieved.path()))?;
    check_files(retrieved.path(), std::slice::from_ref(&zip))?;
    session.save("swift",json!({"status":"draft","repo":repo,"git_url":git_url,"asset_base_url":asset_base,"commit":commit,"branch":branch}))?;
    let refs = git(&tree, &["ls-remote", "origin", &format!("refs/tags/{tag}")])?;
    if refs.is_empty() {
        run(git_command().current_dir(&tree).args([
            "-c",
            "credential.helper=!gh auth git-credential",
            "push",
            "origin",
            &format!("{commit}:refs/tags/{tag}"),
        ]))?;
    } else {
        ensure!(
            refs.split_whitespace().next() == Some(&commit),
            "Swift tag already points elsewhere"
        );
    }
    session.rehash()?;
    run(Command::new("gh").args([
        "release",
        "edit",
        &tag,
        "--repo",
        repo,
        "--target",
        &commit,
        "--draft=false",
    ]))?;
    session.save("swift",json!({"status":"published","repo":repo,"git_url":git_url,"asset_base_url":asset_base,"commit":commit,"branch":branch}))
}
fn distribution_files(
    root: &Path,
    session: &Session<'_>,
    url: &str,
    zip: &FileRecord,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let source = Command::new("tar")
        .arg("-xOf")
        .arg(session.dir.join("generated-sources.tar"))
        .arg("./swift/Sources/LDKNode/LDKNode.swift")
        .output()?;
    ensure!(
        source.status.success() && !source.stdout.is_empty(),
        "cannot read generated Swift wrapper"
    );
    let provenance = json!({"schema":1,"candidate_id":session.candidate.id,"recipe":session.candidate.recipe,"native_rev":session.candidate.native_rev,"inputs_sha256":session.candidate.inputs_sha256,"candidate_sha256":session.state.candidate_sha256,"framework":zip,"swift_source_sha256":digest(&source.stdout)});
    let mut files = BTreeMap::new();
    files.insert("Sources/LDKNode/LDKNode.swift".into(), source.stdout);
    files.insert(
        "Package.swift".into(),
        package(url, &zip.sha256, &load_inputs(root)?.deployment_targets).into_bytes(),
    );
    files.insert(
        "release-provenance.json".into(),
        serde_json::to_vec_pretty(&provenance)?,
    );
    for license in ["LICENSE-APACHE", "LICENSE-MIT"] {
        files.insert(license.into(), fs::read(root.join(license))?);
    }
    Ok(files)
}
fn assert_distribution(tree: &Path, desired: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let tracked = git(
        tree,
        &[
            "ls-tree",
            "-r",
            "--format=%(objectmode) %(objecttype) %(path)",
            "HEAD",
        ],
    )?;
    ensure!(
        tracked
            .lines()
            .eq(desired.keys().map(|name| format!("100644 blob {name}"))),
        "existing Swift distribution branch contains different files"
    );
    for (name, bytes) in desired {
        let blob = git_command()
            .current_dir(tree)
            .args(["cat-file", "blob", &format!("HEAD:{name}")])
            .output()?;
        ensure!(
            blob.status.success() && blob.stdout == *bytes,
            "existing Swift distribution content differs: {name}"
        );
    }
    Ok(())
}
fn package(url: &str, checksum: &str, targets: &DeploymentTargets) -> String {
    let DeploymentTargets {
        swift_macos,
        swift_ios,
    } = targets;
    format!("// swift-tools-version: 5.5\nimport PackageDescription\n\nlet package = Package(\n    name: \"ldk-node\",\n    platforms: [.macOS(\"{swift_macos}\"), .iOS(\"{swift_ios}\")],\n    products: [.library(name: \"LDKNode\", targets: [\"LDKNodeFFI\", \"LDKNode\"])],\n    targets: [\n        .target(name: \"LDKNode\", dependencies: [\"LDKNodeFFI\"]),\n        .binaryTarget(name: \"LDKNodeFFI\", url: \"{url}\", checksum: \"{checksum}\")\n    ]\n)\n")
}
#[cfg(test)]
mod tests {
    #[test]
    fn swift_metadata_preserves_deployment_targets() {
        let manifest = super::package(
            "https://example.org/release.zip",
            "checksum",
            &super::DeploymentTargets {
                swift_macos: "12.0".into(),
                swift_ios: "15.0".into(),
            },
        );
        assert!(
            manifest.contains(".iOS(\"15.0\")"),
            "iOS 15 deployment minimum must be preserved"
        );
        assert!(
            manifest.contains(".macOS(\"12.0\")"),
            "macOS 12 deployment minimum must be preserved"
        );
    }
}
