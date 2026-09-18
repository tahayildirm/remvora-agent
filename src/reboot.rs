//! Local capability-gated reboot; commands and arguments are fixed, never supplied by a remote peer.
use std::process::Command;
fn command() -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new(r"C:\Windows\System32\shutdown.exe");
        command.args(["/r", "/t", "30", "/d", "p:4:1"]);
        command
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/sbin/shutdown");
        command.args(["-r", "+1"]);
        command
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("/usr/bin/systemctl");
        command.arg("reboot");
        command
    }
}
pub async fn request(allowed: bool) -> &'static str {
    if !allowed {
        return "denied";
    }
    match tokio::task::spawn_blocking(|| {
        command()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
    })
    .await
    {
        Ok(Ok(status)) if status.success() => "accepted",
        _ => "failed",
    }
}
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn default_capability_never_executes() {
        assert_eq!(super::request(false).await, "denied");
    }
    #[test]
    fn command_is_absolute_and_fixed() {
        let command = super::command();
        assert!(std::path::Path::new(command.get_program()).is_absolute());
        assert!(
            command
                .get_args()
                .all(|arg| !arg.to_string_lossy().contains(';'))
        );
    }
}
