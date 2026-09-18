//! Native Windows SCM lifecycle. Enrollment must use the same DPAPI account as the service.
use std::{ffi::OsString, time::Duration};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
const NAME: &str = "RemvoraAgent";
define_windows_service!(ffi_service_main, service_main);
pub fn dispatch() -> anyhow::Result<()> {
    service_dispatcher::start(NAME, ffi_service_main)?;
    Ok(())
}
fn service_main(_arguments: Vec<OsString>) {
    let result: anyhow::Result<()> = (|| {
        let handler = service_control_handler::register(NAME, |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                crate::STOP.notify_one();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })?;
        let status = |state, exit| ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(exit),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        };
        handler.set_service_status(status(ServiceState::StartPending, 0))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        use clap::Parser;
        let args = crate::Args::parse();
        anyhow::ensure!(
            matches!(&args.command, crate::Command::Run),
            "Service mode requires run"
        );
        handler.set_service_status(status(ServiceState::Running, 0))?;
        let outcome = runtime.block_on(crate::run(args));
        handler.set_service_status(status(
            ServiceState::Stopped,
            if outcome.is_ok() { 0 } else { 1 },
        ))?;
        outcome
    })();
    if result.is_err() {
        tracing::error!("Windows service stopped after an initialization or runtime failure");
    }
}
