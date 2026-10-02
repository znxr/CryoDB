use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub(crate) const UPDATE_PUBKEY: &str =
    "b7622eb12096f9dbe29f1311f5c0012cf44be88e09647d87db7a053ecbccbd5f";

pub(crate) const MANIFEST_URL: &str = "https://cryodb.znxr.dev/latest.json";

pub(crate) const PROTOCOL: u32 = 1;

const PUBKEY_ENV: &str = "CRYODB_UPDATE_PUBKEY";
const MANIFEST_URL_ENV: &str = "CRYODB_UPDATE_MANIFEST_URL";

fn update_pubkey() -> String {
    std::env::var(PUBKEY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| UPDATE_PUBKEY.to_string())
}

pub(crate) fn manifest_url() -> String {
    std::env::var(MANIFEST_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MANIFEST_URL.to_string())
}

pub(crate) fn update_channel_enabled() -> bool {
    !update_pubkey().is_empty() && current_artifact_key().is_some()
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct Artifact {
    pub(crate) url: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) version: String,
    pub(crate) protocol: u32,
    pub(crate) pub_date: String,
    pub(crate) artifacts: BTreeMap<String, Artifact>,
    pub(crate) signature: String,
}

fn canonical(manifest: &Manifest) -> String {
    let mut out = String::from("cryosql-update-v1\n");
    out.push_str(&format!("version={}\n", manifest.version));
    out.push_str(&format!("protocol={}\n", manifest.protocol));
    out.push_str(&format!("pub_date={}\n", manifest.pub_date));
    for (kind, artifact) in &manifest.artifacts {
        out.push_str(&format!(
            "{kind}\t{}\t{}\t{}\n",
            artifact.url, artifact.sha256, artifact.size
        ));
    }
    out
}

fn verify_signature(pubkey_hex: &str, message: &str, signature_hex: &str) -> Result<(), String> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let key_bytes: [u8; 32] = hex::decode(pubkey_hex)
        .map_err(|_| String::from("update key is not hex"))?
        .try_into()
        .map_err(|_| String::from("update key must be 32 bytes"))?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| String::from("update key is invalid"))?;
    let signature_bytes: [u8; 64] = hex::decode(signature_hex)
        .map_err(|_| String::from("update signature is not hex"))?
        .try_into()
        .map_err(|_| String::from("update signature must be 64 bytes"))?;
    verifying_key
        .verify(message.as_bytes(), &Signature::from_bytes(&signature_bytes))
        .map_err(|_| String::from("update manifest signature is invalid"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Install {
    AppImage(PathBuf),
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Windows,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MacApp(PathBuf),
    Pacman,
    Debian,
    Other,
}

impl Install {
    pub(crate) fn self_updatable(&self) -> bool {
        matches!(
            self,
            Install::AppImage(_) | Install::Windows | Install::MacApp(_)
        )
    }

    pub(crate) fn manual_hint(&self) -> &'static str {
        match self {
            Install::Pacman => "Run `pacman -Syu` to update CryoDB.",
            Install::Debian => "Run `sudo apt update && sudo apt upgrade cryodb`.",
            _ => "Download the new release from https://cryodb.znxr.dev.",
        }
    }
}

pub(crate) fn detect_install() -> Install {
    if let Ok(path) = std::env::var("APPIMAGE")
        && !path.is_empty()
    {
        return Install::AppImage(PathBuf::from(path));
    }

    let exe = std::env::current_exe().ok();

    #[cfg(target_os = "windows")]
    {
        let _ = exe;
        Install::Windows
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(exe) = &exe
            && let Some(bundle) = exe
                .ancestors()
                .find(|path| path.extension().is_some_and(|ext| ext == "app"))
        {
            return Install::MacApp(bundle.to_path_buf());
        }
        Install::Other
    }

    #[cfg(target_os = "linux")]
    {
        match exe.as_deref() {
            Some(exe) if owned_by(exe, "pacman", &["-Qo"]) => Install::Pacman,
            Some(exe) if owned_by(exe, "dpkg", &["-S"]) => Install::Debian,
            _ => Install::Other,
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = exe;
        Install::Other
    }
}

#[cfg(target_os = "linux")]
fn owned_by(exe: &Path, tool: &str, args: &[&str]) -> bool {
    std::process::Command::new(tool)
        .args(args)
        .arg(exe)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn current_artifact_key() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x64-appimage"),
        ("windows", "x86_64") => Some("windows-x64"),
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x64"),
        _ => None,
    }
}

pub(crate) fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub(crate) fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Status {
    Disabled,
    UpToDate,
    Available(Available),
}

#[derive(Debug, Clone)]
pub(crate) struct Available {
    pub(crate) version: String,
    pub(crate) artifact: Artifact,
    pub(crate) install: Install,
}

impl Available {
    pub(crate) fn self_updatable(&self) -> bool {
        self.install.self_updatable()
    }
}

pub(crate) async fn check(http: &reqwest::Client) -> Result<Status, String> {
    let pubkey = update_pubkey();
    if pubkey.is_empty() {
        return Ok(Status::Disabled);
    }
    let Some(key) = current_artifact_key() else {
        return Ok(Status::Disabled);
    };

    let manifest: Manifest = http
        .get(manifest_url())
        .send()
        .await
        .map_err(|error| format!("update check failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("update check failed: {error}"))?
        .json()
        .await
        .map_err(|error| format!("update manifest is unreadable: {error}"))?;

    verify_signature(&pubkey, &canonical(&manifest), &manifest.signature)?;

    if manifest.protocol != PROTOCOL {
        return Err(format!(
            "update manifest speaks protocol {} — this build speaks {PROTOCOL}",
            manifest.protocol
        ));
    }
    if !is_newer(&manifest.version, current_version()) {
        return Ok(Status::UpToDate);
    }
    let Some(artifact) = manifest.artifacts.get(key).cloned() else {
        return Ok(Status::UpToDate);
    };

    Ok(Status::Available(Available {
        version: manifest.version,
        artifact,
        install: detect_install(),
    }))
}

pub(crate) async fn download_verified(
    http: &reqwest::Client,
    artifact: &Artifact,
) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};

    let bytes = http
        .get(&artifact.url)
        .send()
        .await
        .map_err(|error| format!("update download failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("update download failed: {error}"))?
        .bytes()
        .await
        .map_err(|error| format!("update download failed: {error}"))?;

    if hex::encode(Sha256::digest(&bytes)) != artifact.sha256.to_ascii_lowercase() {
        return Err(String::from(
            "the downloaded update failed its checksum and was discarded",
        ));
    }
    Ok(bytes.to_vec())
}

pub(crate) fn apply_and_restart(install: &Install, bytes: Vec<u8>) -> Result<(), String> {
    match install {
        Install::AppImage(path) => apply_appimage(path, bytes),
        Install::MacApp(bundle) => apply_macos(bundle, bytes),
        Install::Windows => apply_windows(bytes),
        other => Err(String::from(other.manual_hint())),
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn apply_appimage(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let staged = dir.join(".cryodb-update.AppImage");
    std::fs::write(&staged, &bytes).map_err(io_error("writing the update"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
            .map_err(io_error("marking the update executable"))?;
    }
    std::fs::rename(&staged, path).map_err(io_error("installing the update"))?;
    std::process::Command::new(path)
        .spawn()
        .map_err(io_error("relaunching"))?;
    std::process::exit(0);
}

#[allow(unused_variables)]
fn apply_macos(bundle: &Path, bytes: Vec<u8>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let parent = bundle.parent().unwrap_or_else(|| Path::new("."));
        let staging = parent.join(".cryodb-update");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(io_error("staging the update"))?;
        let tarball = staging.join("update.tar.gz");
        std::fs::write(&tarball, &bytes).map_err(io_error("writing the update"))?;
        run_ok(
            "tar",
            &[
                "-xzf",
                &tarball.to_string_lossy(),
                "-C",
                &staging.to_string_lossy(),
            ],
        )?;
        let new_bundle = staging.join("CryoDB.app");
        if !new_bundle.is_dir() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(String::from(
                "the downloaded update does not contain CryoDB.app",
            ));
        }
        let _ = std::fs::remove_file(&tarball);
        let _ = run_ok(
            "xattr",
            &["-dr", "com.apple.quarantine", &new_bundle.to_string_lossy()],
        );
        swap_macos_bundle(&new_bundle, bundle, &staging)?;
        let _ = std::fs::remove_dir_all(&staging);
        let _ = install_macos_cli_symlink(bundle);
        std::process::Command::new("open")
            .arg(bundle)
            .spawn()
            .map_err(io_error("relaunching"))?;
        std::process::exit(0);
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(String::from("not macOS"))
    }
}

#[cfg(target_os = "macos")]
fn swap_macos_bundle(new_bundle: &Path, bundle: &Path, staging: &Path) -> Result<(), String> {
    let backup = staging.join("previous.app");
    let had_previous = bundle.exists();
    if had_previous {
        std::fs::rename(bundle, &backup).map_err(swap_error("replacing the installed app"))?;
    }
    match std::fs::rename(new_bundle, bundle) {
        Ok(()) => Ok(()),
        Err(error) => {
            if had_previous {
                let _ = std::fs::rename(&backup, bundle);
            }
            Err(swap_error("installing the new app")(error))
        }
    }
}

#[cfg(target_os = "macos")]
fn swap_error(context: &'static str) -> impl Fn(std::io::Error) -> String {
    move |error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!(
                "{context}: {error}. Install the .pkg from https://cryodb.znxr.dev to update this copy."
            )
        } else {
            format!("{context}: {error}")
        }
    }
}

#[cfg(target_os = "macos")]
fn install_macos_cli_symlink(bundle: &Path) -> Result<(), String> {
    let target = bundle.join("Contents/MacOS/cryodb");
    let link = Path::new("/usr/local/bin/cryodb");
    if !target.exists() {
        return Err(String::from("bundle has no cryodb binary"));
    }
    if let Some(parent) = link.parent()
        && !parent.exists()
    {
        return Err(String::from("/usr/local/bin does not exist"));
    }
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(&target, link).map_err(io_error("linking the cryodb command"))
}

#[allow(unused_variables)]
fn apply_windows(bytes: Vec<u8>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().map_err(io_error("locating the executable"))?;
        let dir = exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let staging = dir.join("cryodb-update");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(io_error("staging the update"))?;
        let archive = staging.join("update.zip");
        std::fs::write(&archive, &bytes).map_err(io_error("writing the update"))?;
        run_ok(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                &format!(
                    "$ErrorActionPreference = 'Stop'; Expand-Archive -Force -LiteralPath '{}' -DestinationPath '{}'",
                    archive.display(),
                    staging.display()
                ),
            ],
        )?;
        let _ = std::fs::remove_file(&archive);
        let staged = staging.join("cryodb.exe");
        let previous = dir.join("cryodb-previous.exe");
        let pid = std::process::id();
        let script = staging.join("swap.bat");
        let body = format!(
            "@echo off\r\n\
             :wait\r\n\
             tasklist /FI \"PID eq {pid}\" | find \"{pid}\" >nul && (timeout /t 1 >nul & goto wait)\r\n\
             del /F /Q \"{previous}\" >nul 2>&1\r\n\
             start \"\" \"{exe}\"\r\n",
            pid = pid,
            previous = previous.display(),
            exe = exe.display(),
        );
        std::fs::write(&script, body).map_err(io_error("writing the update helper"))?;
        let _ = std::fs::remove_file(&previous);
        std::fs::rename(&exe, &previous).map_err(io_error("installing the update"))?;
        if let Err(error) = std::fs::rename(&staged, &exe) {
            let _ = std::fs::rename(&previous, &exe);
            return Err(io_error("installing the update")(error));
        }
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &script.to_string_lossy()])
            .spawn()
            .map_err(io_error("launching the update helper"))?;
        std::process::exit(0);
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(String::from("not Windows"))
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn run_ok(command: &str, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .map_err(io_error("running an update helper"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail: String = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect();
    if detail.is_empty() {
        Err(format!("{command} failed while installing the update"))
    } else {
        Err(format!(
            "{command} failed while installing the update: {detail}"
        ))
    }
}

fn io_error(context: &'static str) -> impl Fn(std::io::Error) -> String {
    move |error| format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn sample() -> Manifest {
        let mut artifacts = BTreeMap::new();
        artifacts.insert(
            String::from("linux-x64-appimage"),
            Artifact {
                url: String::from(
                    "https://cryodb.znxr.dev/releases/1.0.0/cryodb-linux-x64.AppImage",
                ),
                sha256: "ab".repeat(32),
                size: 1234,
            },
        );
        artifacts.insert(
            String::from("windows-x64"),
            Artifact {
                url: String::from("https://cryodb.znxr.dev/releases/1.0.0/cryodb-windows-x64.zip"),
                sha256: "cd".repeat(32),
                size: 5678,
            },
        );
        Manifest {
            version: String::from("1.0.0"),
            protocol: PROTOCOL,
            pub_date: String::from("2026-07-30T00:00:00Z"),
            artifacts,
            signature: String::new(),
        }
    }

    #[test]
    fn canonical_form_matches_the_signer() {
        let expected = "cryosql-update-v1\n\
             version=1.0.0\n\
             protocol=1\n\
             pub_date=2026-07-30T00:00:00Z\n\
             linux-x64-appimage\thttps://cryodb.znxr.dev/releases/1.0.0/cryodb-linux-x64.AppImage\t\
             abababababababababababababababababababababababababababababababab\t1234\n\
             windows-x64\thttps://cryodb.znxr.dev/releases/1.0.0/cryodb-windows-x64.zip\t\
             cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd\t5678\n";
        assert_eq!(canonical(&sample()), expected);
    }

    #[test]
    fn a_signed_manifest_verifies_and_tampering_is_caught() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let pubkey = hex::encode(signing.verifying_key().to_bytes());

        let mut manifest = sample();
        manifest.signature = hex::encode(signing.sign(canonical(&manifest).as_bytes()).to_bytes());
        verify_signature(&pubkey, &canonical(&manifest), &manifest.signature).unwrap();

        let mut tampered = manifest.clone();
        tampered.artifacts.get_mut("windows-x64").unwrap().sha256 = "ee".repeat(32);
        assert!(
            verify_signature(&pubkey, &canonical(&tampered), &manifest.signature).is_err(),
            "a swapped hash must break the signature"
        );
    }

    #[test]
    fn version_comparison_is_prerelease_aware() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.0-beta.10", "0.1.0-beta.2"));
        assert!(is_newer("0.1.0", "0.1.0-beta.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-beta.1", "0.1.0-beta.2"));
        assert!(!is_newer("not-a-version", "0.1.0"));
    }

    #[test]
    fn the_shipped_artifact_key_is_one_the_release_job_publishes() {
        let published = [
            "linux-x64-appimage",
            "windows-x64",
            "macos-arm64",
            "macos-x64",
        ];
        if let Some(key) = current_artifact_key() {
            assert!(published.contains(&key));
        }
    }

    #[test]
    fn package_manager_installs_are_notify_only() {
        assert!(!Install::Pacman.self_updatable());
        assert!(!Install::Debian.self_updatable());
        assert!(!Install::Other.self_updatable());
        assert!(Install::Windows.self_updatable());
        assert!(Install::MacApp(PathBuf::from("/Applications/CryoDB.app")).self_updatable());
    }
}
