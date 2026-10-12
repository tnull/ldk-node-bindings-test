use anyhow::{bail, ensure, Context, Result};
use bitcoin_hashes::{sha256, Hash, HashEngine};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

pub const GROUPS: [&str; 6] = [
    "android",
    "darwin",
    "generated",
    "jvm",
    "linux-aarch64",
    "linux-x86_64",
];
pub const JVM_NATIVE: [&str; 3] = [
    "jvm-darwin-aarch64.dylib",
    "jvm-darwin-x86_64.dylib",
    "jvm-linux-x86_64.so",
];
pub fn host_groups(os: &str, arch: &str) -> Result<&'static [&'static str]> {
    match (os, arch) {
        ("linux", "x86_64") => Ok(&["generated", "linux-x86_64", "android", "jvm"]),
        ("linux", "aarch64") => Ok(&["linux-aarch64"]),
        ("macos", "aarch64") => Ok(&["darwin"]),
        _ => bail!("unsupported verification host {os}-{arch}; use x86_64 Linux, aarch64 Linux, or Apple Silicon macOS"),
    }
}
pub fn preparation_branch(branch: &str) -> bool {
    branch.len() > 8
        && branch.as_bytes()[..4].iter().all(u8::is_ascii_digit)
        && branch.as_bytes()[4] == b'-'
        && branch.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
        && branch.as_bytes()[7] == b'-'
}

pub fn release_branch(branch: &str) -> bool {
    if branch == "main" || preparation_branch(branch) {
        return true;
    }
    let parts: Vec<_> = branch.split('.').collect();
    matches!(parts.as_slice(), [major, minor, "x"] if [major, minor].iter().all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Inputs {
    pub schema: u32,
    pub bindings_version: String,
    pub native: Native,
    pub groups: BTreeMap<String, Vec<String>>,
    pub deployment_targets: DeploymentTargets,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeploymentTargets {
    pub swift_macos: String,
    pub swift_ios: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Native {
    pub rev: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub schema: u32,
    pub recipe: String,
    pub inputs_sha256: String,
    pub group: String,
    pub files: Vec<FileRecord>,
    pub imports: Vec<FileRecord>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub native_rev: String,
    pub recipe: String,
    pub inputs_sha256: String,
    pub groups: BTreeMap<String, Inventory>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u32,
    pub candidate_id: String,
    pub recipe: String,
    pub inputs_sha256: String,
    pub group: String,
    pub files: Vec<FileRecord>,
    pub imports: Vec<FileRecord>,
    pub method: String,
    pub host: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Verified {
    pub schema: u32,
    pub candidate_id: String,
    pub candidate_sha256: String,
    pub recipe: String,
    pub inputs_sha256: String,
    pub receipts: Vec<Receipt>,
}

pub fn safe_name(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s != "."
            && s != ".."
            && !s.starts_with('-')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "unsafe filename or identifier: {s}"
    );
    Ok(())
}
pub fn hex(s: &str, len: usize) -> Result<()> {
    ensure!(
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid hexadecimal identity"
    );
    Ok(())
}
pub fn digest(bytes: &[u8]) -> String {
    sha256::Hash::hash(bytes).to_string()
}
pub fn hash_file(path: &Path) -> Result<FileRecord> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("missing artifact {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "artifact must be a regular file: {}",
        path.display()
    );
    let name = path
        .file_name()
        .context("missing filename")?
        .to_str()
        .context("non-UTF8 filename")?
        .to_owned();
    safe_name(&name)?;
    let mut file = fs::File::open(path)?;
    let mut hasher = sha256::Hash::engine();
    let mut buffer = [0u8; 65536];
    let mut size = 0;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.input(&buffer[..n]);
        size += n as u64;
    }
    Ok(FileRecord {
        name,
        size,
        sha256: sha256::Hash::from_engine(hasher).to_string(),
    })
}
pub fn check_files(dir: &Path, records: &[FileRecord]) -> Result<()> {
    for record in records {
        safe_name(&record.name)?;
        hex(&record.sha256, 64)?;
        ensure!(
            hash_file(&dir.join(&record.name))? == *record,
            "artifact differs: {}",
            record.name
        );
    }
    Ok(())
}
pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "JSON must be a regular file"
    );
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("invalid JSON: {}", path.display()))
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    use std::io::Write;
    tmp.write_all(&serde_json::to_vec_pretty(value)?)?;
    tmp.write_all(b"\n")?;
    tmp.persist(path)?;
    Ok(())
}
pub fn load_inputs(root: &Path) -> Result<Inputs> {
    let inputs: Inputs = read_json(&root.join("release.json"))?;
    ensure!(inputs.schema == 1, "unsupported input schema");
    safe_name(&inputs.bindings_version)?;
    hex(&inputs.native.rev, 40)?;
    for target in [
        &inputs.deployment_targets.swift_macos,
        &inputs.deployment_targets.swift_ios,
    ] {
        let parts: Vec<_> = target.split('.').collect();
        ensure!(
            (2..=3).contains(&parts.len())
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())),
            "invalid Swift deployment target"
        );
    }
    ensure!(
        inputs.groups.keys().map(String::as_str).eq(GROUPS),
        "release inputs must declare all six groups"
    );
    let mut seen = BTreeSet::new();
    for names in inputs.groups.values() {
        ensure!(!names.is_empty(), "empty artifact group");
        for name in names {
            safe_name(name)?;
            ensure!(seen.insert(name), "duplicate artifact name");
            ensure!(
                name != "candidate.json"
                    && name != "checksums.txt"
                    && !name.ends_with(".inventory.json"),
                "reserved artifact name"
            );
        }
    }
    Ok(inputs)
}
pub fn validate_candidate(
    c: &Candidate,
    inputs: &Inputs,
    recipe: &str,
    input_hash: &str,
) -> Result<()> {
    ensure!(
        c.schema == 1
            && c.recipe == recipe
            && c.inputs_sha256 == input_hash
            && c.native_rev == inputs.native.rev
            && c.version == inputs.bindings_version,
        "candidate identity differs from local recipe/inputs"
    );
    validate_candidate_id(c)?;
    ensure!(
        c.groups.keys().eq(inputs.groups.keys()),
        "candidate group coverage differs"
    );
    for (group, inv) in &c.groups {
        ensure!(
            inv.schema == 1
                && inv.recipe == recipe
                && inv.inputs_sha256 == input_hash
                && inv.group == *group,
            "mixed inventory identity"
        );
        let actual: BTreeSet<_> = inv.files.iter().map(|f| f.name.clone()).collect();
        let expected: BTreeSet<_> = inputs.groups[group].iter().cloned().collect();
        ensure!(
            actual == expected && actual.len() == inv.files.len(),
            "inventory file coverage differs: {group}"
        );
        if group != "jvm" {
            ensure!(inv.imports.is_empty(), "unexpected imported dependencies");
        }
    }
    ensure!(
        c.groups["jvm"].imports == native_records(c)?,
        "JVM imports differ from candidate native assets"
    );
    Ok(())
}
pub fn validate_candidate_id(c: &Candidate) -> Result<()> {
    safe_name(&c.id)?;
    safe_name(&c.version)?;
    hex(&c.recipe, 40)?;
    let prefix = format!("candidate-{}-{}-", c.version, &c.recipe[..12]);
    ensure!(
        c.id.starts_with(&prefix),
        "candidate ID does not match recipe/version"
    );
    let suffix = &c.id[prefix.len()..];
    let parts: Vec<_> = suffix.split('-').collect();
    ensure!(
        parts.len() == 2
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "candidate ID requires numeric run and attempt"
    );
    Ok(())
}
pub fn native_records(c: &Candidate) -> Result<Vec<FileRecord>> {
    JVM_NATIVE
        .iter()
        .map(|name| {
            c.groups
                .values()
                .flat_map(|i| &i.files)
                .find(|f| f.name == *name)
                .cloned()
                .with_context(|| format!("missing native dependency {name}"))
        })
        .collect()
}
pub fn validate_receipts(c: &Candidate, receipts: &[Receipt]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for r in receipts {
        ensure!(
            r.schema == 1
                && r.candidate_id == c.id
                && r.recipe == c.recipe
                && r.inputs_sha256 == c.inputs_sha256
                && r.method == "independent-rebuild",
            "receipt identity or method differs"
        );
        ensure!(seen.insert(r.group.clone()), "duplicate receipt group");
        let inv = c.groups.get(&r.group).context("unknown receipt group")?;
        let (os, arch) = r.host.split_once('-').context("invalid receipt host")?;
        ensure!(
            host_groups(os, arch)?.contains(&r.group.as_str()),
            "receipt host cannot build group {}",
            r.group
        );
        ensure!(r.files == inv.files, "receipt files differ");
        ensure!(r.imports == inv.imports, "JVM imports differ in receipt");
    }
    for group in c.groups.keys() {
        if !seen.contains(group) {
            bail!("missing receipt for {group}");
        }
    }
    for native in native_records(c)? {
        ensure!(
            receipts
                .iter()
                .filter(|r| r.group != "jvm")
                .flat_map(|r| &r.files)
                .any(|f| *f == native),
            "JVM imports require independent native receipts"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_checksums_match_standard_vectors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artifact.bin");
        for (bytes, expected) in [
            (
                Vec::new(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".to_vec(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                vec![b'a'; 1_000_000],
                "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            ),
        ] {
            assert_eq!(digest(&bytes), expected);
            fs::write(&path, &bytes).unwrap();
            let record = hash_file(&path).unwrap();
            assert_eq!(record.name, "artifact.bin");
            assert_eq!(record.size, bytes.len() as u64);
            assert_eq!(record.sha256, expected);
        }
    }
}
