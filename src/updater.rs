//! Offline signed release staging. The control server cannot choose a trust key or execute an update.
use anyhow::{Result, ensure};
use p256::{
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    pkcs8::DecodePublicKey,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: String,
    os: String,
    arch: String,
    sha256: String,
}

#[derive(Deserialize)]
struct LatestResponse {
    update: bool,
    #[serde(rename = "artifactUrl")]
    artifact_url: Option<String>,
    #[serde(rename = "manifestUrl")]
    manifest_url: Option<String>,
    #[serde(rename = "signatureUrl")]
    signature_url: Option<String>,
}

pub enum UpdateOutcome {
    None,
    Staged,
    Applied,
}

pub async fn poll_and_stage(
    client: reqwest::Client,
    server: url::Url,
    state: PathBuf,
    trusted_key: PathBuf,
    apply_helper: Option<PathBuf>,
) {
    loop {
        if let Err(error) = check_and_stage(
            &client,
            &server,
            &state,
            &trusted_key,
            apply_helper.as_deref(),
        )
        .await
        {
            tracing::warn!(%error, "Agent update check failed");
        }
        tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    }
}

pub async fn check_and_stage(
    client: &reqwest::Client,
    server: &url::Url,
    state: &Path,
    trusted_key: &Path,
    apply_helper: Option<&Path>,
) -> Result<UpdateOutcome> {
    let latest = server.join(&format!(
        "api/v1/agent/update/latest?platform={}&architecture={}&version={}&channel=stable",
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION")
    ))?;
    let response: LatestResponse = client
        .get(latest)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if !response.update {
        return Ok(UpdateOutcome::None);
    }
    let directory = state.join("updates").join("download");
    std::fs::create_dir_all(&directory)?;
    let artifact = directory.join("artifact.tar.gz");
    let manifest = directory.join("manifest.json");
    let signature = directory.join("manifest.sig");
    download(client, server, response.artifact_url.as_deref(), &artifact).await?;
    download(client, server, response.manifest_url.as_deref(), &manifest).await?;
    download(
        client,
        server,
        response.signature_url.as_deref(),
        &signature,
    )
    .await?;
    let staged = stage(state, &manifest, &signature, &artifact, trusted_key)?;
    tracing::info!(path = %staged.display(), "Verified agent update staged");
    if let Some(helper) = apply_helper {
        apply(helper, &manifest, &signature, &artifact, trusted_key)?;
        return Ok(UpdateOutcome::Applied);
    }
    Ok(UpdateOutcome::Staged)
}

fn apply(
    helper: &Path,
    manifest: &Path,
    signature: &Path,
    artifact: &Path,
    trusted_key: &Path,
) -> Result<()> {
    ensure!(helper.is_absolute(), "Update apply helper must be absolute");
    let status = std::process::Command::new("/usr/bin/sudo")
        .arg("-n")
        .arg(helper)
        .arg(manifest)
        .arg(signature)
        .arg(artifact)
        .arg(trusted_key)
        .status()?;
    ensure!(status.success(), "Update apply helper failed");
    Ok(())
}

async fn download(
    client: &reqwest::Client,
    server: &url::Url,
    path: Option<&str>,
    destination: &Path,
) -> Result<()> {
    let path = path.ok_or_else(|| anyhow::anyhow!("Missing update URL"))?;
    let bytes = client
        .get(server.join(path.trim_start_matches('/'))?)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    ensure!(
        bytes.len() <= 512 * 1024 * 1024,
        "Update download too large"
    );
    std::fs::write(destination, bytes)?;
    Ok(())
}
fn verify(bytes: &[u8], signature: &[u8], key: &[u8], digest: &[u8]) -> Result<Manifest> {
    ensure!(bytes.len() <= 4096, "Manifest too large");
    VerifyingKey::from_public_key_der(key)?.verify(bytes, &Signature::from_der(signature)?)?;
    let manifest: Manifest = serde_json::from_slice(bytes)?;
    let version = semver::Version::parse(&manifest.version)?;
    ensure!(
        version > semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
        "Update must be newer than this agent"
    );
    ensure!(
        manifest.os == std::env::consts::OS && manifest.arch == std::env::consts::ARCH,
        "Update target mismatch"
    );
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    ensure!(manifest.sha256 == hash, "Artifact digest mismatch");
    Ok(manifest)
}
fn bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    ensure!(
        std::fs::metadata(path)?.len() <= max,
        "Update metadata exceeds limit"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= max, "Update metadata exceeds limit");
    Ok(bytes)
}

/// Copies into an exclusive staging file and verifies that exact copy before keeping it.
/// An interrupted update never replaces the running executable.
pub fn stage(
    state: &Path,
    manifest: &Path,
    signature: &Path,
    artifact: &Path,
    key: &Path,
) -> Result<PathBuf> {
    let bytes = bounded(manifest, 4096)?;
    let signature = bounded(signature, 256)?;
    let key = bounded(key, 4096)?;
    let directory = state.join("updates");
    if directory.exists() {
        ensure!(
            !directory.symlink_metadata()?.file_type().is_symlink(),
            "Update directory cannot be a symlink"
        );
    }
    std::fs::create_dir_all(&directory)?;
    let destination = directory.join(format!("{}.staged", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(&destination)?;
    let result: Result<()> = (|| {
        let mut input = std::fs::File::open(artifact)?;
        ensure!(
            input.metadata()?.len() <= 512 * 1024 * 1024,
            "Artifact too large"
        );
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut total = 0usize;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count;
            ensure!(total <= 512 * 1024 * 1024, "Artifact too large");
            hash.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        let verified = verify(&bytes, &signature, &key, &hash.finalize())?;
        output.sync_all()?;
        tracing::info!(version = verified.version, path = %destination.display(), "Verified staged artifact");
        Ok(())
    })();
    drop(output);
    if result.is_err() {
        let _ = std::fs::remove_file(&destination);
    }
    result?;
    Ok(destination)
}
/// Install using a helper copied from the currently installed agent. The service must be stopped.
/// Rejects an unrelated or different-version target, preserves the previous binary and rolls back rename failure.
pub fn install(
    state: &Path,
    manifest: &Path,
    signature: &Path,
    artifact: &Path,
    key: &Path,
    target: &Path,
) -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    install_from(
        state,
        manifest,
        signature,
        artifact,
        key,
        target,
        &executable,
    )
}
fn digest_file(path: &Path) -> Result<Vec<u8>> {
    let mut input = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().to_vec())
}
fn unpack_agent_binary(artifact: &Path, destination: &Path) -> Result<()> {
    let input = std::fs::File::open(artifact)?;
    let decoder = flate2::read::GzDecoder::new(input);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?;
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name != "remvora-agent" && name != "remvora-agent.exe" {
            continue;
        }
        let mut output = std::fs::File::create(destination)?;
        std::io::copy(&mut entry, &mut output)?;
        output.sync_all()?;
        return Ok(());
    }
    Err(anyhow::anyhow!("Update artifact does not contain remvora-agent binary"))
}
fn install_from(
    state: &Path,
    manifest: &Path,
    signature: &Path,
    artifact: &Path,
    key: &Path,
    target: &Path,
    helper: &Path,
) -> Result<PathBuf> {
    ensure!(target.is_absolute(), "Install target must be absolute");
    ensure!(
        target.symlink_metadata()?.is_file()
            && !target.symlink_metadata()?.file_type().is_symlink(),
        "Install target must be a regular file"
    );
    ensure!(
        digest_file(target)? == digest_file(helper)?,
        "Installer must be a copy of the currently installed agent"
    );
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing target directory"))?;
    ensure!(
        parent.canonicalize()? == parent,
        "Install directory must be canonical and contain no symlinks"
    );
    let staged = stage(state, manifest, signature, artifact, key)?;
    let candidate = parent.join(format!(".remvora-{}.new", uuid::Uuid::new_v4()));
    let backup = parent.join(format!(".remvora-{}.previous", uuid::Uuid::new_v4()));
    let result: Result<()> = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        drop(options.open(&candidate)?);
        unpack_agent_binary(&staged, &candidate)?;
        std::fs::set_permissions(&candidate, target.metadata()?.permissions())?;
        ensure!(
            digest_file(target)? == digest_file(helper)?,
            "Installed executable changed during update"
        );
        std::fs::rename(target, &backup)?;
        if let Err(error) = std::fs::rename(&candidate, target) {
            std::fs::rename(&backup, target)?;
            return Err(error.into());
        }
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&staged);
    if result.is_err() {
        let _ = std::fs::remove_file(&candidate);
    }
    result?;
    Ok(backup)
}
#[cfg(test)]
mod tests {
    use super::*;
    use p256::{
        ecdsa::{SigningKey, signature::Signer},
        pkcs8::EncodePublicKey,
    };
    #[test]
    fn installer_preserves_previous_and_rejects_unrelated_targets() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".runtime")
            .join(format!("update-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let key = SigningKey::random(&mut rand_core::OsRng);
        let public = directory.join("public.der");
        std::fs::write(
            &public,
            key.verifying_key().to_public_key_der().unwrap().as_bytes(),
        )
        .unwrap();
        let artifact = directory.join("new.tar.gz");
        {
            let output = std::fs::File::create(&artifact).unwrap();
            let encoder =
                flate2::write::GzEncoder::new(output, flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            let payload = b"new-test-binary-not-executed";
            header.set_path("remvora-agent").unwrap();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            archive.append(&header, &payload[..]).unwrap();
            archive.finish().unwrap();
        }
        let digest = digest_file(&artifact).unwrap();
        let hash: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let bytes=serde_json::to_vec(&serde_json::json!({"version":"99.0.0","os":std::env::consts::OS,"arch":std::env::consts::ARCH,"sha256":hash})).unwrap();
        let manifest = directory.join("manifest");
        std::fs::write(&manifest, &bytes).unwrap();
        let signature = directory.join("signature");
        let sig: Signature = key.sign(&bytes);
        std::fs::write(&signature, sig.to_der().as_bytes()).unwrap();
        let helper = directory.join("helper");
        std::fs::write(&helper, b"old-test-binary-not-executed").unwrap();
        let target = directory.join("installed");
        std::fs::copy(&helper, &target).unwrap();
        let backup = install_from(
            &directory, &manifest, &signature, &artifact, &public, &target, &helper,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"new-test-binary-not-executed"
        );
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            std::fs::read(&helper).unwrap()
        );
        assert!(
            install_from(
                &directory, &manifest, &signature, &artifact, &public, &target, &helper
            )
            .is_err()
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }
    #[test]
    fn signed_release_rejects_tampering_wrong_target_and_rollback() {
        let key = SigningKey::random(&mut rand_core::OsRng);
        let public = key.verifying_key().to_public_key_der().unwrap();
        let digest = Sha256::digest(b"test artifact");
        let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        let manifest = |version: &str, os: &str| {
            serde_json::to_vec(&serde_json::json!({"version":version,"os":os,"arch":std::env::consts::ARCH,"sha256":hash})).unwrap()
        };
        let bytes = manifest("99.0.0", std::env::consts::OS);
        let sig: Signature = key.sign(&bytes);
        assert!(verify(&bytes, sig.to_der().as_bytes(), public.as_bytes(), &digest).is_ok());
        assert!(
            verify(
                &bytes,
                sig.to_der().as_bytes(),
                public.as_bytes(),
                &Sha256::digest(b"modified")
            )
            .is_err()
        );
        for invalid in [
            manifest("0.0.0", std::env::consts::OS),
            manifest("99.0.0", "wrong-os"),
        ] {
            let sig: Signature = key.sign(&invalid);
            assert!(
                verify(
                    &invalid,
                    sig.to_der().as_bytes(),
                    public.as_bytes(),
                    &digest
                )
                .is_err()
            );
        }
        let mut modified = bytes.clone();
        modified.push(b' ');
        assert!(
            verify(
                &modified,
                sig.to_der().as_bytes(),
                public.as_bytes(),
                &digest
            )
            .is_err()
        );
    }
}
