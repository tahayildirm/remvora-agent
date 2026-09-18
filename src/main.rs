mod audio;
mod desktop;
mod files;
mod identity;
mod protocol;
mod reboot;
mod relay;
#[cfg(windows)]
mod service;
mod terminal;
mod transport;
mod updater;
mod video_resize;
use anyhow::{Result, bail, ensure};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use identity::Identity;
use protocol::Signal;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Parser)]
#[command(version, about = "Remvora authenticated device agent")]
struct Args {
    #[cfg(windows)]
    #[arg(long, default_value_t = false)]
    service: bool,
    #[arg(long, env = "REMVORA_SERVER")]
    server: url::Url,
    #[arg(long, env = "REMVORA_STATE_DIR")]
    state: PathBuf,
    #[arg(long, default_value_t = false)]
    allow_terminal: bool,
    /// Allow server-authorized Linux terminal sessions to use normal OS sudo/su rules.
    #[arg(long, default_value_t = false, requires = "allow_terminal")]
    allow_terminal_privilege_escalation: bool,
    #[arg(long, default_value_t = false)]
    allow_reboot: bool,
    #[arg(long, default_value_t = false)]
    allow_desktop: bool,
    #[arg(long, default_value_t = false, requires = "allow_desktop")]
    allow_clipboard: bool,
    /// Explicit directory shared with authorized desktop sessions. No subdirectories.
    #[arg(long, requires = "allow_desktop")]
    file_root: Option<PathBuf>,
    #[arg(long, default_value_t = false, requires = "allow_desktop")]
    allow_audio: bool,
    #[arg(long)]
    monitor_id: Option<u32>,
    #[arg(long)]
    shell: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    allow_loopback_http: bool,
    #[arg(long, env = "REMVORA_CA_CERTIFICATE")]
    ca_certificate: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Enroll,
    Activate,
    Run,
    /// List displays without capturing their content.
    ListDisplays,
    /// Install a verified signed release, preserving the previous executable. Stop the service first.
    InstallUpdate {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        signature: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        trusted_key: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    StageUpdate {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        signature: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        trusted_key: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter("remvora_agent=info")
        .init();
    let args = Args::parse();
    #[cfg(windows)]
    if args.service {
        return service::dispatch();
    }
    run(args).await
}

static STOP: std::sync::LazyLock<tokio::sync::Notify> =
    std::sync::LazyLock::new(tokio::sync::Notify::new);
async fn shutdown() {
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = STOP.notified() => {} }
}
async fn run(args: Args) -> Result<()> {
    match args.command {
        Command::InstallUpdate {
            ref manifest,
            ref signature,
            ref artifact,
            ref trusted_key,
            ref target,
        } => {
            let backup = updater::install(
                &args.state,
                manifest,
                signature,
                artifact,
                trusted_key,
                target,
            )?;
            println!(
                "{}",
                serde_json::json!({"installed":target,"backup":backup})
            );
            return Ok(());
        }
        Command::ListDisplays => {
            println!("{}", serde_json::to_string(&desktop::displays()?)?);
            return Ok(());
        }
        Command::StageUpdate {
            ref manifest,
            ref signature,
            ref artifact,
            ref trusted_key,
        } => {
            updater::stage(&args.state, manifest, signature, artifact, trusted_key)?;
            tracing::info!(
                "Signed update staged; stop the service and install using the platform deployment procedure"
            );
            return Ok(());
        }
        _ => {}
    }
    ensure!(
        args.server.scheme() == "https"
            || (args.allow_loopback_http
                && args.server.scheme() == "http"
                && matches!(
                    args.server.host_str(),
                    Some("127.0.0.1" | "localhost" | "[::1]")
                )),
        "HTTPS is required outside explicitly enabled loopback tests"
    );
    ensure!(
        args.server.username().is_empty()
            && args.server.password().is_none()
            && args.server.query().is_none()
            && args.server.fragment().is_none(),
        "Server URL must not contain credentials, query or fragment"
    );
    let identity = Identity::load_or_create(&args.state)?;
    let mut client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(path) = &args.ca_certificate {
        client =
            client.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(path)?)?);
    }
    let client = client.build()?;
    match args.command {
        Command::InstallUpdate { .. } | Command::StageUpdate { .. } | Command::ListDisplays => {
            unreachable!("Maintenance commands handled before identity access")
        }
        Command::Enroll => {
            let token = std::env::var("REMVORA_ENROLLMENT_TOKEN")
                .map_err(|_| anyhow::anyhow!("Set REMVORA_ENROLLMENT_TOKEN"))?;
            let result=post(&client,&args,"api/v1/enrollment/request",json!({"token":token,"publicKey":identity.public_key()?,"operatingSystem":std::env::consts::OS,"architecture":std::env::consts::ARCH,"agentVersion":env!("CARGO_PKG_VERSION")})).await?;
            let device = result["deviceId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing device identity"))?;
            Uuid::parse_str(device)?;
            std::fs::write(args.state.join("device-id"), device)?;
            tracing::info!(
                device_id = device,
                "Enrollment submitted; administrator approval is required"
            );
        }
        Command::Activate => {
            let proof = proof(&client, &args, &identity, "activate").await?;
            client
                .post(args.server.join("api/v1/agent/activate")?)
                .json(&proof)
                .send()
                .await?
                .error_for_status()?;
            tracing::info!("Device activated");
        }
        Command::Run => {
            let mut attempt = 0u32;
            loop {
                tokio::select! {
                    result=connect(&client,&args,&identity) => {
                        if result.is_err() { tracing::warn!("Control connection ended; retrying with backoff"); }
                    }
                    _=shutdown()=>break,
                }
                attempt = (attempt + 1).min(6);
                let jitter = u64::from(rand_core::RngCore::next_u32(&mut rand_core::OsRng) % 1000);
                tokio::select! {_=tokio::time::sleep(Duration::from_millis((1u64<<attempt)*1000+jitter))=>{},_=shutdown()=>break}
            }
        }
    }
    Ok(())
}
async fn post(client: &reqwest::Client, args: &Args, path: &str, body: Value) -> Result<Value> {
    Ok(client
        .post(args.server.join(path)?)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}
async fn proof(
    client: &reqwest::Client,
    args: &Args,
    identity: &Identity,
    purpose: &str,
) -> Result<Value> {
    let device = std::fs::read_to_string(args.state.join("device-id"))?;
    Uuid::parse_str(&device)?;
    let challenge = post(
        client,
        args,
        "api/v1/agent/challenge",
        json!({"deviceId":device,"purpose":purpose}),
    )
    .await?;
    let text = challenge["challenge"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing challenge"))?;
    ensure!(
        text.starts_with(&format!("remvora:v1:{purpose}:{device}:")),
        "Challenge binding mismatch"
    );
    Ok(json!({"challengeId":challenge["challengeId"],"signature":identity.sign(text)}))
}
async fn connect(client: &reqwest::Client, args: &Args, identity: &Identity) -> Result<()> {
    let proof = proof(client, args, identity, "connect").await?;
    let mut url = args.server.join("ws/agent")?;
    url.set_scheme(if args.server.scheme() == "https" {
        "wss"
    } else {
        "ws"
    })
    .map_err(|_| anyhow::anyhow!("Invalid scheme"))?;
    let connector = if let Some(path) = &args.ca_certificate {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let pem = std::fs::read(path)?;
        use rustls::pki_types::pem::PemObject;
        for certificate in rustls::pki_types::CertificateDer::pem_slice_iter(&pem) {
            roots.add(certificate?)?;
        }
        let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        Some(tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(
            config,
        )))
    } else {
        None
    };
    let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(65536))
        .max_frame_size(Some(65536));
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async_tls_with_config(
            url.as_str(),
            Some(ws_config),
            false,
            connector,
        ),
    )
    .await??;
    let (mut sink, mut stream) = socket.split();
    sink.send(Message::Text(
        serde_json::to_string(&Signal::new("agent.authenticate", None, proof))?.into(),
    ))
    .await?;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut sessions: HashMap<Uuid, transport::RemotePeer> = HashMap::new();
    let mut grants: HashMap<Uuid, (String, bool)> = HashMap::new();
    let (relay_tx, mut relay_rx) = tokio::sync::mpsc::channel::<(Uuid, Value)>(32);
    let (ice_tx, mut ice_rx) = tokio::sync::mpsc::channel::<(Uuid, Value)>(128);
    let mut relays: HashMap<Uuid, relay::RelayPeer> = HashMap::new();
    let mut relay_sequence = 0u64;
    let mut cleanup = tokio::time::interval(Duration::from_secs(2));
    let mut reboot_seen = std::collections::HashSet::new();
    let result:Result<()>=async {
        loop { tokio::select! {
            _=cleanup.tick()=>{
                let ended:Vec<_>=relays.iter().filter(|(_,p)|p.finished()).map(|(id,_)|*id).collect();
                for id in ended {if let Some(peer)=sessions.remove(&id){peer.close().await;} relays.remove(&id); grants.remove(&id);let _=sink.send(Message::Text(serde_json::to_string(&Signal::new("session.close",Some(id),json!({})))?.into())).await;}
            },
            Some((id,payload))=ice_rx.recv()=>{
                if sessions.contains_key(&id) && !relays.contains_key(&id) {
                    tokio::time::timeout(Duration::from_secs(5),sink.send(Message::Text(serde_json::to_string(&Signal::new("webrtc.iceCandidate",Some(id),payload))?.into()))).await??;
                }
            },
            Some((id,mut payload))=relay_rx.recv()=>{
                if relays.contains_key(&id){relay_sequence+=1;payload["sequence"]=json!(relay_sequence);tokio::time::timeout(Duration::from_secs(5),sink.send(Message::Text(serde_json::to_string(&Signal::new("relay.data",Some(id),payload))?.into()))).await??;}
            },
            _=heartbeat.tick()=>sink.send(Message::Text(serde_json::to_string(&Signal::new("agent.heartbeat",None,json!({})))?.into())).await?,
            frame=stream.next()=>{
                let Some(frame)=frame else { break };let frame=frame?;
                if let Message::Ping(bytes)=frame { sink.send(Message::Pong(bytes)).await?; continue; }
                if matches!(frame, Message::Pong(_)) { continue; }
                let Message::Text(text)=frame else { break };
                ensure!(text.len()<=65536,"Signal too large");let signal:Signal=serde_json::from_str(&text)?;ensure!(signal.protocol_version==1,"Unsupported protocol");
                let sent=chrono::DateTime::parse_from_rfc3339(&signal.timestamp)?;
                ensure!((chrono::Utc::now()-sent.with_timezone(&chrono::Utc)).num_seconds().abs()<=120,"Stale signal");
                if signal.kind=="agent.connected" {tracing::info!("Authenticated control connection established");continue;}
                if signal.kind == "device.reboot" && signal.session_id.is_none() {
                    let command_id: Uuid = serde_json::from_value(signal.payload["commandId"].clone())?;
                    let expires = chrono::DateTime::parse_from_rfc3339(signal.payload["expiresAt"].as_str().ok_or_else(||anyhow::anyhow!("Missing command expiry"))?)?;
                    ensure!(expires > chrono::Utc::now() && expires <= chrono::Utc::now()+chrono::Duration::seconds(60), "Invalid command expiry");
                    ensure!(reboot_seen.len()<64 && reboot_seen.insert(command_id), "Command replay");
                    let code = reboot::request(args.allow_reboot).await;
                    sink.send(Message::Text(serde_json::to_string(&Signal::new("device.reboot.result",None,json!({"commandId":command_id,"code":code})))?.into())).await?;
                    continue;
                }
                let id=signal.session_id.ok_or_else(||anyhow::anyhow!("Missing session"))?;
                match signal.kind.as_str() {
                    "session.request"=>{
                        let kind=signal.payload["kind"].as_str().unwrap_or("");
                        let elevation = kind == "Terminal" && signal.payload["allowTerminalPrivilegeEscalation"].as_bool().unwrap_or(false);
                        let elevation_blocked = elevation && !terminal::elevation_available(args.allow_terminal_privilege_escalation);
                        let accepted=!elevation_blocked && ((kind=="Terminal" && args.allow_terminal)||(kind=="Desktop" && args.allow_desktop)) && grants.is_empty();
                        if accepted {grants.insert(id,(kind.into(),elevation));}
                        sink.send(Message::Text(serde_json::to_string(&Signal::new(if accepted{"session.accept"}else{"session.reject"},Some(id),json!({"terminalPolicyVersion":1,"terminalPrivilegeEscalation":elevation,"trickleIce":accepted,"code":if accepted{"OK"}else if elevation_blocked{"TERMINAL_ELEVATION_UNAVAILABLE"}else if !grants.is_empty(){"SESSION_BUSY"}else{"CAPABILITY_UNAVAILABLE"}})))?.into())).await?;
                    }
                    "webrtc.offer"=>{
                        ensure!(grants.contains_key(&id) && !sessions.contains_key(&id),"Unauthorized offer");
                        let sdp=signal.payload["sdp"].as_str().ok_or_else(||anyhow::anyhow!("Missing SDP"))?;
                        let shell=args.shell.clone().unwrap_or_else(terminal::default_shell);
                        ensure!(shell.is_absolute(),"Shell path must be absolute");
                        match transport::RemotePeer::answer(sdp,shell,grants.get(&id).is_some_and(|x|x.0=="Desktop"),transport::DesktopOptions{terminal_elevation:grants.get(&id).is_some_and(|x|x.1),clipboard:args.allow_clipboard,monitor_id:args.monitor_id,file_root:args.file_root.clone(),audio:args.allow_audio,state:args.state.clone()},Some((id,ice_tx.clone()))).await {
                            Ok((peer,answer)) => {
                                sessions.insert(id,peer);
                                sink.send(Message::Text(serde_json::to_string(&Signal::new("webrtc.answer",Some(id),json!({"sdp":answer,"type":"answer"})))?.into())).await?;
                            }
                            Err(_) => {
                                tracing::warn!("ICE negotiation failed; keeping authorized session available for relay");
                                sink.send(Message::Text(serde_json::to_string(&Signal::new("webrtc.answer",Some(id),json!({"error":"ICE_NEGOTIATION_FAILED"})))?.into())).await?;
                            }
                        }
                    }
                    "webrtc.iceCandidate"=>{
                        if let Some(peer)=sessions.get(&id) && !relays.contains_key(&id)
                            && peer.add_candidate(signal.payload).await.is_err() {
                            tracing::warn!("Ignoring invalid ICE candidate for active peer");
                        }
                    },
                    "relay.start"=>{
                        ensure!(grants.contains_key(&id),"Unauthorized relay");
                        if let Some(peer)=sessions.remove(&id){peer.close().await;}
                        ensure!(!relays.contains_key(&id),"Relay already active");
                        let shell=args.shell.clone().unwrap_or_else(terminal::default_shell);
                        let peer=relay::RelayPeer::start(id,relay_tx.clone(),shell,grants.get(&id).is_some_and(|x|x.0=="Desktop"),transport::DesktopOptions{terminal_elevation:grants.get(&id).is_some_and(|x|x.1),clipboard:args.allow_clipboard,monitor_id:args.monitor_id,file_root:args.file_root.clone(),audio:args.allow_audio,state:args.state.clone()}).await?;
                        relays.insert(id,peer);
                        sink.send(Message::Text(serde_json::to_string(&Signal::new("relay.ready",Some(id),json!({})))?.into())).await?;
                    },
                    "relay.data"=>{
                        // Frames already in flight may arrive after session cleanup.
                        // Never let an ended session disconnect the device control socket.
                        if let Some(peer)=relays.get(&id)
                            && peer.receive(&signal.payload).await.is_err() {
                                relays.remove(&id); grants.remove(&id);
                                sink.send(Message::Text(serde_json::to_string(&Signal::new("session.close",Some(id),json!({"code":"RELAY_INPUT_INVALID"})))?.into())).await?;
                        }
                    },
                    "session.close"=>{relays.remove(&id);grants.remove(&id);if let Some(peer)=sessions.remove(&id){peer.close().await;}},
                    _=>bail!("Unexpected signal type"),
                }
            }
        }} Ok(())
    }.await;
    for (_, peer) in sessions {
        peer.close().await;
    }
    result
}
