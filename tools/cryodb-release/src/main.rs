use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
struct Artifact {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: String,
    protocol: u32,
    pub_date: String,
    artifacts: BTreeMap<String, Artifact>,
    #[serde(default)]
    signature: String,
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

fn sha256_file(path: &Path) -> Result<(String, u64), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok((hex::encode(hasher.finalize()), bytes.len() as u64))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or(&[]);
    let result = match command {
        "keygen" => keygen(),
        "pubkey" => pubkey(rest),
        "sign" => sign(rest),
        "verify" => verify(rest),
        _ => Err(String::from(
            "usage: cryodb-release <keygen|pubkey|sign|verify> …  (see the file header for flags)",
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn keygen() -> Result<(), String> {
    let signing = SigningKey::generate(&mut rand::rngs::OsRng);
    println!("# CryoDB update-signing keypair.");
    println!("# PRIVATE KEY — keep it offline, never commit it, never put it on a server.");
    println!("# Store it as the GitHub secret CRYODB_UPDATE_KEY and back it up.");
    println!("private {}", hex::encode(signing.to_bytes()));
    println!("# PUBLIC KEY — paste into UPDATE_PUBKEY in src/update/mod.rs.");
    println!(
        "public  {}",
        hex::encode(signing.verifying_key().to_bytes())
    );
    Ok(())
}

fn pubkey(args: &[String]) -> Result<(), String> {
    let seed = match args.first().map(String::as_str) {
        Some("--key") => {
            let path = args.get(1).ok_or("--key needs a path")?;
            let raw =
                std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
            extract_hex_key(&raw)
        }
        Some(inline) => extract_hex_key(inline),
        None => None,
    }
    .ok_or("give the private key hex, or --key <file>")?;
    println!(
        "{}",
        hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes())
    );
    Ok(())
}

type ParsedFlags = (BTreeMap<String, String>, Vec<(String, String)>);

fn parse_flags(args: &[String]) -> Result<ParsedFlags, String> {
    let mut flags = BTreeMap::new();
    let mut artifacts = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let key = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument: {arg}"))?;
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("--{key} needs a value"))?;
        if key == "artifact" {
            let (kind, path) = value
                .split_once('=')
                .ok_or_else(|| format!("--artifact expects KIND=PATH, got {value}"))?;
            artifacts.push((kind.to_string(), path.to_string()));
        } else {
            flags.insert(key.to_string(), value.clone());
        }
        index += 2;
    }
    Ok((flags, artifacts))
}

fn sign(args: &[String]) -> Result<(), String> {
    let (flags, artifacts) = parse_flags(args)?;
    let get = |key: &str| {
        flags
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing --{key}"))
    };

    let version = get("version")?;
    let protocol: u32 = get("protocol")?
        .parse()
        .map_err(|_| "protocol must be a number")?;
    let base_url = get("base-url")?.trim_end_matches('/').to_string();
    let key_path = get("key")?;
    let out = flags
        .get("out")
        .cloned()
        .unwrap_or_else(|| String::from("latest.json"));
    let pub_date = flags.get("pub-date").cloned().unwrap_or_else(iso8601_now);

    if artifacts.is_empty() {
        return Err(String::from(
            "at least one --artifact KIND=PATH is required",
        ));
    }

    let key_raw = std::fs::read_to_string(&key_path)
        .map_err(|error| format!("read key {key_path}: {error}"))?;
    let seed = extract_hex_key(&key_raw).ok_or("the key file holds no valid hex key")?;
    let signing = SigningKey::from_bytes(&seed);

    let mut map = BTreeMap::new();
    for (kind, path) in &artifacts {
        let path = Path::new(path);
        let file_name = path
            .file_name()
            .ok_or_else(|| format!("artifact path has no file name: {}", path.display()))?
            .to_string_lossy()
            .to_string();
        let (sha256, size) = sha256_file(path)?;
        map.insert(
            kind.clone(),
            Artifact {
                url: format!("{base_url}/{file_name}"),
                sha256,
                size,
            },
        );
    }

    let mut manifest = Manifest {
        version,
        protocol,
        pub_date,
        artifacts: map,
        signature: String::new(),
    };
    manifest.signature = hex::encode(signing.sign(canonical(&manifest).as_bytes()).to_bytes());

    let json = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    std::fs::write(&out, json.as_bytes()).map_err(|error| format!("write {out}: {error}"))?;
    println!(
        "wrote {out} ({} artifacts, signed)",
        manifest.artifacts.len()
    );
    for (kind, artifact) in &manifest.artifacts {
        println!("  {kind}: {} ({} bytes)", artifact.url, artifact.size);
    }
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let (flags, _) = parse_flags(&args[..args.len().saturating_sub(1)])?;
    let path = args.last().ok_or("verify needs a latest.json path")?;
    let pubkey_hex = flags.get("pubkey").ok_or("missing --pubkey")?;
    let key_bytes: [u8; 32] = hex::decode(pubkey_hex)
        .map_err(|_| "pubkey is not hex")?
        .try_into()
        .map_err(|_| "pubkey must be 32 bytes")?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes).map_err(|e| e.to_string())?;

    let raw = std::fs::read_to_string(path).map_err(|error| format!("read {path}: {error}"))?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let signature_bytes: [u8; 64] = hex::decode(&manifest.signature)
        .map_err(|_| "signature is not hex")?
        .try_into()
        .map_err(|_| "signature must be 64 bytes")?;
    verifying_key
        .verify(
            canonical(&manifest).as_bytes(),
            &ed25519_dalek::Signature::from_bytes(&signature_bytes),
        )
        .map_err(|_| String::from("SIGNATURE INVALID"))?;
    println!("OK: {path} verifies against {pubkey_hex}");
    println!(
        "  version {} · protocol {}",
        manifest.version, manifest.protocol
    );
    Ok(())
}

fn extract_hex_key(raw: &str) -> Option<[u8; 32]> {
    raw.split_whitespace()
        .filter(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .find_map(|token| hex::decode(token).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
}

fn iso8601_now() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("unix:{seconds}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            protocol: 1,
            pub_date: String::from("2026-07-30T00:00:00Z"),
            artifacts,
            signature: String::new(),
        }
    }

    #[test]
    fn canonical_form_is_the_documented_one() {
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
    fn sign_then_verify_round_trips_and_tampering_fails() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let verifying = signing.verifying_key();

        let mut manifest = sample();
        manifest.signature = hex::encode(signing.sign(canonical(&manifest).as_bytes()).to_bytes());
        let signature: [u8; 64] = hex::decode(&manifest.signature)
            .unwrap()
            .try_into()
            .unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&signature);

        verifying
            .verify(canonical(&manifest).as_bytes(), &signature)
            .unwrap();

        manifest
            .artifacts
            .get_mut("linux-x64-appimage")
            .unwrap()
            .sha256 = "ef".repeat(32);
        assert!(verifying
            .verify(canonical(&manifest).as_bytes(), &signature)
            .is_err());
    }

    #[test]
    fn a_key_file_with_comments_still_yields_the_seed() {
        let raw = format!("# a comment\nprivate {}\n", "1f".repeat(32));
        assert_eq!(extract_hex_key(&raw).unwrap(), [0x1fu8; 32]);
        assert!(extract_hex_key("# nothing here").is_none());
    }
}
