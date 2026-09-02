use std::{collections::BTreeMap, fs, io::Read, path::PathBuf, process::Command};

use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::bufread::GzDecoder;
use serde_json::Value;

fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_omegon-maintain")
        .map(Into::into)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path.join("omegon-maintain")
        })
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let encoded = include_str!("fixtures/release-verifier-v1.tar.gz.b64")
        .split_whitespace()
        .collect::<String>();
    let bytes = STANDARD.decode(encoded).unwrap();
    let mut container = tar::Archive::new(GzDecoder::new(bytes.as_slice()));
    let directory = tempfile::tempdir().unwrap();
    let mut files = BTreeMap::new();
    for entry in container.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_str().unwrap().to_string();
        if name.starts_with("._") {
            continue;
        }
        let path = directory.path().join(&name);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        fs::write(&path, bytes).unwrap();
        files.insert(name, path);
    }
    let prefix = "omegon-0.29.0-dev-fixture.1-x86_64-unknown-linux-gnu.tar.gz";
    let archive = files.remove(prefix).unwrap();
    let manifest = files.remove(&format!("{prefix}.manifest.json")).unwrap();
    let bundle = files
        .remove(&format!("{prefix}.manifest.sigstore.json"))
        .unwrap();
    (directory, archive, manifest, bundle)
}

fn verify(archive: &str, manifest: &str, bundle: &str) -> (i32, Value) {
    verify_with_root(archive, manifest, bundle, None)
}

fn verify_with_root(
    archive: &str,
    manifest: &str,
    bundle: &str,
    extracted_root: Option<&str>,
) -> (i32, Value) {
    let mut command = Command::new(binary());
    command.args([
        "--json",
        "release",
        "verify",
        "--archive",
        archive,
        "--manifest",
        manifest,
        "--bundle",
        bundle,
    ]);
    if let Some(root) = extracted_root {
        command.args(["--extracted-root", root]);
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON ({error}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code().unwrap_or(-1), value)
}

#[test]
fn release_verify_revalidates_an_exact_extracted_root() {
    let (directory, archive, manifest, bundle) = fixture();
    let extracted = directory.path().join("extracted");
    fs::create_dir(&extracted).unwrap();
    let file = fs::File::open(&archive).unwrap();
    let mut package = tar::Archive::new(GzDecoder::new(std::io::BufReader::new(file)));
    package.unpack(&extracted).unwrap();

    let (code, output) = verify_with_root(
        archive.to_str().unwrap(),
        manifest.to_str().unwrap(),
        bundle.to_str().unwrap(),
        Some(extracted.to_str().unwrap()),
    );
    assert_eq!(code, 0, "{output:#}");
    let verified = output["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "release_verified")
        .unwrap();
    let evidence: Value = serde_json::from_str(verified["evidence"].as_str().unwrap()).unwrap();
    assert_eq!(evidence["extracted_root_verified"], true);

    fs::write(extracted.join("omegon"), b"mutated after authentication").unwrap();
    let (code, output) = verify_with_root(
        archive.to_str().unwrap(),
        manifest.to_str().unwrap(),
        bundle.to_str().unwrap(),
        Some(extracted.to_str().unwrap()),
    );
    assert_eq!(code, 1, "{output:#}");
    assert_eq!(
        output["errors"][0]["code"],
        "release_extracted_root_invalid"
    );
}

#[test]
fn release_verify_accepts_production_fixture_without_runtime_roots() {
    let (_directory, archive, manifest, bundle) = fixture();
    let (code, output) = verify(
        archive.to_str().unwrap(),
        manifest.to_str().unwrap(),
        bundle.to_str().unwrap(),
    );
    assert_eq!(code, 0, "{output:#}");
    assert_eq!(output["status"], "success");
    assert!(
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "release_verified")
    );
}

#[test]
fn release_verify_rejects_corruption_and_relative_operands() {
    let (_directory, archive, manifest, bundle) = fixture();
    let mut bytes = fs::read(&archive).unwrap();
    bytes[0] ^= 1;
    fs::write(&archive, bytes).unwrap();
    let (code, output) = verify(
        archive.to_str().unwrap(),
        manifest.to_str().unwrap(),
        bundle.to_str().unwrap(),
    );
    assert_eq!(code, 1, "{output:#}");
    assert_eq!(
        output["errors"][0]["code"],
        "release_archive_digest_mismatch"
    );

    let (code, output) = verify(
        "relative.tar.gz",
        manifest.to_str().unwrap(),
        bundle.to_str().unwrap(),
    );
    assert_eq!(code, 1, "{output:#}");
    assert_eq!(output["errors"][0]["code"], "release_archive_invalid");
}
