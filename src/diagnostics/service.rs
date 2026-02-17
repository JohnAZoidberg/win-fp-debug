use anyhow::Result;
use windows::core::w;
use windows::Win32::System::Services::*;

use crate::output::*;

pub fn check_service() -> Result<()> {
    print_header("Level 2: WbioSrvc Service Status");

    unsafe {
        // Open the Service Control Manager
        let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)?;

        // Open the WbioSrvc service
        let service_result = OpenServiceW(
            scm,
            w!("WbioSrvc"),
            SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        );

        let service = match service_result {
            Ok(s) => s,
            Err(e) => {
                let _ = CloseServiceHandle(scm);
                print_fail(&format!(
                    "Cannot open WbioSrvc service: {} — is the Biometric Service installed?",
                    e
                ));
                return Ok(());
            }
        };

        // Query service status
        let mut status = SERVICE_STATUS::default();
        let query_ok = QueryServiceStatus(service, &mut status);

        if let Err(e) = query_ok {
            print_fail(&format!("QueryServiceStatus failed: {}", e));
        } else {
            let state_str = match status.dwCurrentState {
                SERVICE_STOPPED => "Stopped",
                SERVICE_START_PENDING => "Start Pending",
                SERVICE_STOP_PENDING => "Stop Pending",
                SERVICE_RUNNING => "Running",
                SERVICE_CONTINUE_PENDING => "Continue Pending",
                SERVICE_PAUSE_PENDING => "Pause Pending",
                SERVICE_PAUSED => "Paused",
                _ => "Unknown",
            };

            if status.dwCurrentState == SERVICE_RUNNING {
                print_pass(&format!("WbioSrvc is {}", state_str));
            } else {
                print_fail(&format!("WbioSrvc is {}", state_str));
                if status.dwCurrentState == SERVICE_STOPPED {
                    print_step("Try: net start WbioSrvc (as Administrator)");
                }
            }
        }

        // Query service configuration (two-call buffer pattern)
        let mut bytes_needed = 0u32;
        let _ = QueryServiceConfigW(service, None, 0, &mut bytes_needed);

        if bytes_needed > 0 {
            let mut buf = vec![0u8; bytes_needed as usize];
            let config_ptr = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;

            let config_ok =
                QueryServiceConfigW(service, Some(config_ptr), bytes_needed, &mut bytes_needed);

            if config_ok.is_ok() {
                let config = &*config_ptr;
                let start_type = match config.dwStartType {
                    SERVICE_AUTO_START => "Automatic",
                    SERVICE_BOOT_START => "Boot",
                    SERVICE_DEMAND_START => "Manual (Demand)",
                    SERVICE_DISABLED => "Disabled",
                    SERVICE_SYSTEM_START => "System",
                    _ => "Unknown",
                };

                print_info("Start type", start_type);

                if config.dwStartType == SERVICE_DISABLED {
                    print_warn("Service is disabled — fingerprint operations will not work");
                    print_step("Enable via: sc config WbioSrvc start=auto (as Administrator)");
                }

                if !config.lpBinaryPathName.is_null() {
                    let path = config.lpBinaryPathName.to_string().unwrap_or_default();
                    print_info("Binary path", &path);
                }
            }
        }

        let _ = CloseServiceHandle(service);
        let _ = CloseServiceHandle(scm);
    }

    Ok(())
}
