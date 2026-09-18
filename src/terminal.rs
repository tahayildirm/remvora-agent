use crate::relay::Channel as RTCDataChannel;
use anyhow::Result;
use bytes::Bytes;
use portable_pty::{ChildKiller, CommandBuilder, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

/// Use an interactive shell with a normal directory-aware prompt when available.
pub fn default_shell() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
    } else if std::path::Path::new("/bin/bash").is_file() {
        PathBuf::from("/bin/bash")
    } else {
        PathBuf::from("/bin/sh")
    }
}

/// The device must explicitly opt in and the inherited service restriction must be absent.
pub fn elevation_available(local_opt_in: bool) -> bool {
    #[cfg(target_os = "linux")]
    {
        local_opt_in && unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) } == 0
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = local_opt_in;
        false
    }
}

fn shell_command(shell: &std::path::Path, allow_elevation: bool) -> Result<CommandBuilder> {
    #[cfg(target_os = "linux")]
    if !allow_elevation {
        // Restrict only this shell. The parent agent remains able to start a later authorized shell.
        let helper = ["/usr/bin/setpriv", "/bin/setpriv"]
            .into_iter()
            .find(|path| std::path::Path::new(path).is_file())
            .ok_or_else(|| {
                anyhow::anyhow!("util-linux setpriv is required for restricted terminals")
            })?;
        let mut command = CommandBuilder::new(helper);
        command.arg("--no-new-privs");
        command.arg("--");
        command.arg(shell);
        return Ok(command);
    }
    let _ = allow_elevation;
    Ok(CommandBuilder::new(shell))
}

pub type Killer = Arc<Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>>;
/// Spawn an interactive OS shell directly, never by concatenating a command supplied in signaling.
pub fn start(
    channel: Arc<RTCDataChannel>,
    shell: PathBuf,
    killer: Killer,
    allow_elevation: bool,
) -> Result<()> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = shell_command(&shell, allow_elevation)?;
    if !cfg!(windows) {
        match shell.file_name().and_then(|name| name.to_str()) {
            Some("bash") => {
                command.arg("-i");
                command.env("PS1", r"\u@\h:\w\$ ");
            }
            Some("sh" | "dash" | "ash") => {
                command.arg("-i");
                command.env("PS1", "${PWD} $ ");
            }
            _ => {}
        }
    }
    command.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(command)?;
    *killer.lock().unwrap() = Some(child.clone_killer());
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let master = Arc::new(Mutex::new(pair.master));
    let resized = master.clone();
    let (input_tx, mut input_rx) = mpsc::channel::<Bytes>(32);
    let input_channel = Arc::downgrade(&channel);
    channel.on_message(Box::new(move |message| {
        let input_channel = input_channel.clone();
        let input = input_tx.clone();
        let master = resized.clone();
        Box::pin(async move {
            if message.data.len() > 16384 {
                return;
            }
            if !message.is_string {
                if input.try_send(message.data).is_err()
                    && let Some(channel) = input_channel.upgrade()
                {
                    let _ = channel.close().await;
                }
                return;
            }
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&message.data)
                && value["type"] == "resize"
            {
                let cols = value["cols"].as_u64().unwrap_or(80).clamp(1, 500) as u16;
                let rows = value["rows"].as_u64().unwrap_or(24).clamp(1, 300) as u16;
                let _ = master.lock().unwrap().resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        })
    }));
    tokio::task::spawn_blocking(move || {
        while let Some(data) = input_rx.blocking_recv() {
            if writer
                .write_all(&data)
                .and_then(|_| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });
    let (output_tx, mut output_rx) = mpsc::channel::<Bytes>(32);
    tokio::task::spawn_blocking(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(size) = reader.read(&mut buffer) {
            if size == 0
                || output_tx
                    .blocking_send(Bytes::copy_from_slice(&buffer[..size]))
                    .is_err()
            {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(data) = output_rx.recv().await {
            if channel.buffered_amount().await > 1024 * 1024 || channel.send(&data).await.is_err() {
                break;
            }
        }
        let _ = channel.close().await;
    });
    tokio::task::spawn_blocking(move || {
        let _ = child.wait();
        drop(master);
    });
    Ok(())
}

#[cfg(test)]
pub fn test_input(output: &str, command: &str) -> Vec<u8> {
    // ConPTY may request Win32 key records. A bare CR then lacks VK_RETURN.
    // https://github.com/microsoft/terminal/blob/main/src/terminal/input/terminalInput.cpp
    if output.contains("\x1b[?9001h") {
        format!(
            "{}\x1b[13;28;13;1;0;1_\x1b[13;28;13;0;0;1_",
            command.trim_end_matches('\r')
        )
        .into_bytes()
    } else {
        command.as_bytes().to_vec()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod policy_tests {
    use super::*;
    #[test]
    fn restricted_shell_sets_no_new_privileges() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = shell_command(std::path::Path::new("/bin/sh"), false).unwrap();
        command.arg("-c");
        command.arg("grep NoNewPrivs /proc/self/status");
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        child.wait().unwrap();
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("NoNewPrivs:"), "{text}");
        assert!(text.split_whitespace().any(|part| part == "1"), "{text}");
    }
    #[test]
    fn elevation_requires_local_opt_in() {
        assert!(!elevation_available(false));
    }
}
