use anyhow::{bail, Result};
use windows::core::w;
use windows::Win32::System::Services::*;

use crate::output::*;

unsafe fn query_service_state() -> Result<u32> {
    let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
        .map_err(|e| anyhow::anyhow!("Cannot open Service Control Manager: {}", e))?;

    let service = match OpenServiceW(scm, w!("WbioSrvc"), SERVICE_QUERY_STATUS) {
        Ok(s) => s,
        Err(e) => {
            let _ = CloseServiceHandle(scm);
            bail!("Cannot open WbioSrvc service: {}", e);
        }
    };

    let mut status = SERVICE_STATUS::default();
    let result = QueryServiceStatus(service, &mut status);
    let _ = CloseServiceHandle(service);
    let _ = CloseServiceHandle(scm);
    result.map_err(|e| anyhow::anyhow!("QueryServiceStatus failed: {}", e))?;

    Ok(status.dwCurrentState.0)
}

pub fn run_stop_service() -> Result<()> {
    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    print_header("Stop WbioSrvc Service");

    let state = unsafe { query_service_state()? };
    if state == SERVICE_STOPPED.0 {
        print_info("WbioSrvc", "already stopped");
        return Ok(());
    }

    print_step("Stopping WbioSrvc...");

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
            .map_err(|e| anyhow::anyhow!("Cannot open Service Control Manager: {}", e))?;

        let service = match OpenServiceW(
            scm,
            w!("WbioSrvc"),
            SERVICE_STOP | SERVICE_QUERY_STATUS,
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = CloseServiceHandle(scm);
                bail!("Cannot open WbioSrvc service: {} — run as Administrator", e);
            }
        };

        let mut stop_status = SERVICE_STATUS::default();
        ControlService(service, SERVICE_CONTROL_STOP, &mut stop_status)
            .map_err(|e| anyhow::anyhow!("Failed to stop WbioSrvc: {}", e))?;

        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let mut poll_status = SERVICE_STATUS::default();
            let _ = QueryServiceStatus(service, &mut poll_status);
            if poll_status.dwCurrentState == SERVICE_STOPPED {
                let _ = CloseServiceHandle(service);
                let _ = CloseServiceHandle(scm);
                print_pass("WbioSrvc stopped");
                return Ok(());
            }
        }

        let _ = CloseServiceHandle(service);
        let _ = CloseServiceHandle(scm);
        bail!("WbioSrvc did not stop in time");
    }
}

pub fn run_start_service() -> Result<()> {
    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    print_header("Start WbioSrvc Service");

    let state = unsafe { query_service_state()? };
    if state == SERVICE_RUNNING.0 {
        print_info("WbioSrvc", "already running");
        return Ok(());
    }

    print_step("Starting WbioSrvc...");

    unsafe {
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
            .map_err(|e| anyhow::anyhow!("Cannot open Service Control Manager: {}", e))?;

        let service = match OpenServiceW(
            scm,
            w!("WbioSrvc"),
            SERVICE_START | SERVICE_QUERY_STATUS,
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = CloseServiceHandle(scm);
                bail!("Cannot open WbioSrvc service: {}", e);
            }
        };

        StartServiceW(service, None)
            .map_err(|e| anyhow::anyhow!("Failed to start WbioSrvc: {}", e))?;

        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let mut poll_status = SERVICE_STATUS::default();
            let _ = QueryServiceStatus(service, &mut poll_status);
            if poll_status.dwCurrentState == SERVICE_RUNNING {
                let _ = CloseServiceHandle(service);
                let _ = CloseServiceHandle(scm);
                print_pass("WbioSrvc started");
                return Ok(());
            }
        }

        let _ = CloseServiceHandle(service);
        let _ = CloseServiceHandle(scm);
        bail!("WbioSrvc did not start in time");
    }
}
