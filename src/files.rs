//! Explicitly shared flat directory. No arbitrary paths, overwrite, execution or shell commands.
use crate::relay::Channel as RTCDataChannel;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
const MAX: u64 = 100 * 1024 * 1024;
const CHUNK: usize = 12 * 1024;
fn name(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 180
            && !value.starts_with('.')
            && !value.ends_with(['.', ' '])
            && !value
                .chars()
                .any(|c| c.is_control() || "/\\:<>\"|?*".contains(c)),
        "Invalid file name"
    );
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    ensure!(
        ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"
        ]
        .contains(&stem.as_str()),
        "Reserved name"
    );
    Ok(value)
}
struct Upload {
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
    size: u64,
    offset: u64,
    hash: Sha256,
}
impl Drop for Upload {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.temporary);
    }
}
struct Download {
    file: File,
    size: u64,
    offset: u64,
    hash: Sha256,
}
struct Session {
    root: PathBuf,
    upload: Option<Upload>,
    download: Option<Download>,
    bytes: u64,
}
impl Session {
    fn new(root: &Path) -> Result<Self> {
        ensure!(
            root.is_absolute() && root.canonicalize()? == root && root.is_dir(),
            "Share must be an existing canonical directory"
        );
        Ok(Self {
            root: root.into(),
            upload: None,
            download: None,
            bytes: 0,
        })
    }
    fn handle(&mut self, v: Value) -> Result<Value> {
        ensure!(self.root.canonicalize()? == self.root, "Share changed");
        let kind = v["type"].as_str().unwrap_or("");
        let offset = v["offset"].as_u64().unwrap_or(u64::MAX);
        match kind {
            "list" => {
                let mut entries = Vec::new();
                for item in std::fs::read_dir(&self.root)?.take(1000) {
                    let item = item?;
                    let n = item.file_name().to_string_lossy().into_owned();
                    if name(&n).is_ok() && item.file_type()?.is_file() {
                        let size = item.metadata()?.len();
                        if size <= MAX {
                            entries.push(json!({"name":n,"size":size}));
                        }
                    }
                    if entries.len() >= 200 {
                        break;
                    }
                }
                entries.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
                Ok(json!({"type":"list","files":entries}))
            }
            "cancel" => {
                self.upload = None;
                self.download = None;
                Ok(json!({"type":"cancelled"}))
            }
            "upload" => {
                ensure!(
                    self.upload.is_none() && self.download.is_none(),
                    "Transfer busy"
                );
                let size = v["size"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing size"))?;
                ensure!(
                    size <= MAX && self.bytes + size <= 5 * MAX,
                    "Transfer quota exceeded"
                );
                let destination = self.root.join(name(v["name"].as_str().unwrap_or(""))?);
                ensure!(!destination.try_exists()?, "File already exists");
                let temporary = self
                    .root
                    .join(format!(".remvora-{}.part", uuid::Uuid::new_v4()));
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let file = options.open(&temporary)?;
                self.bytes += size;
                self.upload = Some(Upload {
                    file: Some(file),
                    temporary,
                    destination,
                    size,
                    offset: 0,
                    hash: Sha256::new(),
                });
                Ok(json!({"type":"ready","offset":0}))
            }
            "chunk" => {
                let upload = self
                    .upload
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("No upload"))?;
                ensure!(offset == upload.offset, "Offset mismatch");
                let data = STANDARD.decode(v["data"].as_str().unwrap_or(""))?;
                ensure!(
                    !data.is_empty()
                        && data.len() <= CHUNK
                        && upload.offset + data.len() as u64 <= upload.size,
                    "Invalid chunk"
                );
                upload.file.as_mut().unwrap().write_all(&data)?;
                upload.hash.update(&data);
                upload.offset += data.len() as u64;
                Ok(json!({"type":"ack","offset":upload.offset}))
            }
            "finish" => {
                let upload = self
                    .upload
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("No upload"))?;
                ensure!(upload.offset == upload.size, "Incomplete upload");
                let hash = format!("{:x}", upload.hash.clone().finalize());
                ensure!(v["sha256"].as_str() == Some(&hash), "Digest mismatch");
                upload.file.as_ref().unwrap().sync_all()?;
                // hard_link is an atomic no-replace publish on the same filesystem.
                std::fs::hard_link(&upload.temporary, &upload.destination)?;
                Ok(json!({"type":"complete","sha256":hash,"size":upload.size}))
            }
            "download" => {
                ensure!(
                    self.upload.is_none() && self.download.is_none(),
                    "Transfer busy"
                );
                let path = self.root.join(name(v["name"].as_str().unwrap_or(""))?);
                ensure!(path.symlink_metadata()?.is_file(), "Regular files only");
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options.custom_flags(0x00200000);
                }
                let file = options.open(path)?;
                let meta = file.metadata()?;
                let size = meta.len();
                ensure!(
                    meta.is_file() && size <= MAX && self.bytes + size <= 5 * MAX,
                    "File unavailable or quota exceeded"
                );
                self.bytes += size;
                self.download = Some(Download {
                    file,
                    size,
                    offset: 0,
                    hash: Sha256::new(),
                });
                Ok(json!({"type":"download","size":size}))
            }
            "next" => {
                let download = self
                    .download
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("No download"))?;
                ensure!(offset == download.offset, "Offset mismatch");
                if offset == download.size {
                    let hash = format!("{:x}", download.hash.clone().finalize());
                    self.download = None;
                    return Ok(json!({"type":"complete","sha256":hash,"size":offset}));
                }
                let mut data = vec![0; CHUNK.min((download.size - offset) as usize)];
                download.file.read_exact(&mut data)?;
                download.hash.update(&data);
                download.offset += data.len() as u64;
                Ok(json!({"type":"chunk","offset":offset,"data":STANDARD.encode(data)}))
            }
            _ => anyhow::bail!("Unknown file request"),
        }
    }
}
pub fn attach(channel: Arc<RTCDataChannel>, root: PathBuf, stop: Arc<AtomicBool>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let overflow = stop.clone();
    channel.on_message(Box::new(move |message| {
        let tx = tx.clone();
        let stop = overflow.clone();
        Box::pin(async move {
            if !message.is_string
                || message.data.len() > 20000
                || tx.try_send(message.data).is_err()
            {
                stop.store(true, Ordering::Relaxed);
            }
        })
    }));
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let result: Result<()> = (|| {
            let mut session = Session::new(&root)?;
            while !stop.load(Ordering::Relaxed) {
                let bytes = match rx.try_recv() {
                    Ok(bytes) => bytes,
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => break,
                };
                let parsed = serde_json::from_slice::<Value>(&bytes);
                let id = parsed
                    .as_ref()
                    .ok()
                    .and_then(|v| v["id"].as_str())
                    .filter(|id| id.len() <= 64)
                    .unwrap_or("")
                    .to_owned();
                let response = parsed
                    .map_err(anyhow::Error::from)
                    .and_then(|v| session.handle(v));
                let mut response = match response {
                    Ok(v) => v,
                    Err(_) => {
                        session.upload = None;
                        session.download = None;
                        json!({"type":"error","code":"fileRejected"})
                    }
                };
                response["id"] = json!(id);
                runtime.block_on(tokio::time::timeout(
                    Duration::from_secs(10),
                    channel.send_text(response.to_string()),
                ))??;
            }
            Ok(())
        })();
        if result.is_err() {
            runtime.block_on(async {
                let _ = channel.close().await;
            });
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_paths_and_reserved_names() {
        for n in [
            "../secret",
            "a/b",
            "a\\b",
            ".hidden",
            "NUL.txt",
            "a:",
            "a.",
            "a\n",
        ] {
            assert!(name(n).is_err(), "{n}");
        }
    }
    #[test]
    fn transfer_is_verified_bounded_and_no_replace() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".runtime")
            .join(format!("files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let mut s = Session::new(&root).unwrap();
        s.handle(json!({"type":"upload","name":"hello.txt","size":5}))
            .unwrap();
        assert!(
            s.handle(json!({"type":"chunk","offset":1,"data":"aGVsbG8="}))
                .is_err()
        );
        s.handle(json!({"type":"chunk","offset":0,"data":"aGVsbG8="}))
            .unwrap();
        s.handle(json!({"type":"finish","sha256":format!("{:x}",Sha256::digest(b"hello"))}))
            .unwrap();
        assert_eq!(std::fs::read(root.join("hello.txt")).unwrap(), b"hello");
        assert!(
            s.handle(json!({"type":"upload","name":"hello.txt","size":0}))
                .is_err()
        );
        s.handle(json!({"type":"download","name":"hello.txt"}))
            .unwrap();
        assert_eq!(
            s.handle(json!({"type":"next","offset":0})).unwrap()["data"],
            "aGVsbG8="
        );
        assert_eq!(
            s.handle(json!({"type":"next","offset":5})).unwrap()["type"],
            "complete"
        );
        s.handle(json!({"type":"upload","name":"bad.txt","size":0}))
            .unwrap();
        assert!(s.handle(json!({"type":"finish","sha256":"bad"})).is_err());
        assert!(!root.join("bad.txt").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("hello.txt"), root.join("link")).unwrap();
            assert!(s.handle(json!({"type":"download","name":"link"})).is_err());
        }
        drop(s);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_disconnect_and_quota_are_enforced() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".runtime");
        std::fs::create_dir_all(&base).unwrap();
        let directory = tempfile::tempdir_in(base).unwrap();
        let root = directory.path().canonicalize().unwrap();
        let mut s = Session::new(&root).unwrap();
        assert!(
            s.handle(json!({"type":"upload","name":"big","size":MAX+1}))
                .is_err()
        );
        for _ in 0..5 {
            s.handle(json!({"type":"upload","name":"reserved","size":MAX}))
                .unwrap();
            assert!(
                s.handle(json!({"type":"download","name":"reserved"}))
                    .is_err()
            );
            s.handle(json!({"type":"cancel"})).unwrap();
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        }
        assert!(
            s.handle(json!({"type":"upload","name":"over-quota","size":1}))
                .is_err()
        );
        drop(s);
        let mut s = Session::new(&root).unwrap();
        s.handle(json!({"type":"upload","name":"partial","size":10}))
            .unwrap();
        s.handle(json!({"type":"chunk","offset":0,"data":"YQ=="}))
            .unwrap();
        drop(s);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }
    #[test]
    fn empty_files_and_publish_races_preserve_existing_content() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".runtime");
        std::fs::create_dir_all(&base).unwrap();
        let directory = tempfile::tempdir_in(base).unwrap();
        let root = directory.path().canonicalize().unwrap();
        let mut s = Session::new(&root).unwrap();
        let digest = format!("{:x}", Sha256::digest(b""));
        s.handle(json!({"type":"upload","name":"empty","size":0}))
            .unwrap();
        s.handle(json!({"type":"finish","sha256":digest})).unwrap();
        s.handle(json!({"type":"download","name":"empty"})).unwrap();
        assert_eq!(
            s.handle(json!({"type":"next","offset":0})).unwrap()["sha256"],
            digest
        );
        s.handle(json!({"type":"upload","name":"race","size":0}))
            .unwrap();
        std::fs::write(root.join("race"), b"preserve").unwrap();
        assert!(s.handle(json!({"type":"finish","sha256":digest})).is_err());
        assert_eq!(std::fs::read(root.join("race")).unwrap(), b"preserve");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    }
}
