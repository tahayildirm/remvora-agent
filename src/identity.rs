use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use p256::{
    SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey},
};
use rand_core::OsRng;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// The private key is generated locally and never serialized into an HTTP request.
pub struct Identity(SigningKey);
impl Identity {
    pub fn load_or_create(directory: &Path) -> Result<Self> {
        if !directory.exists() {
            fs::create_dir_all(directory)?;
        }
        ensure!(
            !fs::symlink_metadata(directory)?.file_type().is_symlink(),
            "State directory must not be a symlink"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let path = directory.join("identity.key");
        let secret = if path.exists() {
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "Identity must not be a symlink"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                ensure!(
                    metadata.permissions().mode() & 0o077 == 0,
                    "Identity permissions must be 0600"
                );
            }
            SecretKey::from_pkcs8_der(&unprotect(&fs::read(path)?)?).context("Invalid identity")?
        } else {
            let key = SecretKey::random(&mut OsRng);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path)?;
            file.write_all(&protect(key.to_pkcs8_der()?.as_bytes())?)?;
            file.sync_all()?;
            key
        };
        Ok(Self(secret.into()))
    }
    pub fn public_key(&self) -> Result<String> {
        Ok(STANDARD.encode(self.0.verifying_key().to_public_key_der()?.as_bytes()))
    }
    pub fn sign(&self, value: &str) -> String {
        let signature: Signature = self.0.sign(value.as_bytes());
        STANDARD.encode(signature.to_bytes())
    }
}
#[cfg(not(windows))]
fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(bytes.to_vec())
}
#[cfg(not(windows))]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(bytes.to_vec())
}
#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>> {
    dpapi(bytes, true)
}
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>> {
    dpapi(bytes, false)
}

/// DPAPI binds stored private keys to the enrolling Windows account; a service must use that same account.
#[cfg(windows)]
fn dpapi(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len())?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // Input lives for the duration of the call. DPAPI owns the returned LocalAlloc buffer,
    // which is copied once and released with the matching LocalFree API.
    let success = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    ensure!(success != 0, "Windows identity protection failed");
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::{
        ecdsa::{VerifyingKey, signature::Verifier},
        pkcs8::DecodePublicKey,
    };
    #[test]
    fn proof_round_trip_and_tamper_rejection() {
        let identity = Identity(SigningKey::random(&mut OsRng));
        let key = VerifyingKey::from_public_key_der(
            &STANDARD.decode(identity.public_key().unwrap()).unwrap(),
        )
        .unwrap();
        let signature =
            Signature::from_slice(&STANDARD.decode(identity.sign("bound-challenge")).unwrap())
                .unwrap();
        assert!(key.verify(b"bound-challenge", &signature).is_ok());
        assert!(key.verify(b"other-challenge", &signature).is_err());
    }
}
