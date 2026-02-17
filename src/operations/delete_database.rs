use anyhow::{bail, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::BiometricFramework::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Services::*;

use crate::output::*;
use crate::winbio_helpers;

/// Stop the WbioSrvc service. Returns Ok(true) if it was running and is now stopped,
/// Ok(false) if it was already stopped.
unsafe fn stop_wbiosrvc() -> Result<bool> {
    let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
        .map_err(|e| anyhow::anyhow!("Cannot open Service Control Manager: {}", e))?;

    let service = match OpenServiceW(
        scm,
        w!("WbioSrvc"),
        SERVICE_STOP | SERVICE_QUERY_STATUS | SERVICE_START,
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = CloseServiceHandle(scm);
            bail!("Cannot open WbioSrvc service: {} — run as Administrator", e);
        }
    };

    let mut status = SERVICE_STATUS::default();
    QueryServiceStatus(service, &mut status)
        .map_err(|e| anyhow::anyhow!("QueryServiceStatus failed: {}", e))?;

    if status.dwCurrentState == SERVICE_STOPPED {
        let _ = CloseServiceHandle(service);
        let _ = CloseServiceHandle(scm);
        return Ok(false);
    }

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
            return Ok(true);
        }
    }

    let _ = CloseServiceHandle(service);
    let _ = CloseServiceHandle(scm);
    bail!("WbioSrvc did not stop in time");
}

/// Start the WbioSrvc service.
unsafe fn start_wbiosrvc() -> Result<()> {
    let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
        .map_err(|e| anyhow::anyhow!("Cannot open Service Control Manager: {}", e))?;

    let service = match OpenServiceW(scm, w!("WbioSrvc"), SERVICE_START | SERVICE_QUERY_STATUS) {
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
            return Ok(());
        }
    }

    let _ = CloseServiceHandle(service);
    let _ = CloseServiceHandle(scm);
    bail!("WbioSrvc did not start in time");
}

/// Delete the WbioSrvc database registry key.
fn delete_database_registry_key(db_id: &str) -> Result<()> {
    unsafe {
        let subkey = format!(
            "SYSTEM\\CurrentControlSet\\Services\\WbioSrvc\\Databases\\{}",
            db_id
        );
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();

        let status = RegDeleteKeyW(HKEY_LOCAL_MACHINE, PCWSTR(subkey_wide.as_ptr()));

        if status.is_err() {
            bail!(
                "Failed to delete registry key: HKLM\\{} (error: {:?})",
                subkey,
                status
            );
        }
    }
    Ok(())
}

pub fn run_delete_database(db_number: usize, delete_file: bool, delete_registry: bool) -> Result<()> {
    if !delete_file && !delete_registry {
        bail!("Specify --file to delete the .DAT file, --registry to remove the registry entry, or both");
    }

    print_header("Delete Biometric Database");

    if db_number == 0 {
        bail!("Database number must be 1 or greater (use enum-databases to see the list)");
    }

    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    // Enumerate databases to find the target
    let (file_path, db_id) = unsafe {
        let mut schema_array: *mut WINBIO_STORAGE_SCHEMA = std::ptr::null_mut();
        let mut schema_count: usize = 0;

        WinBioEnumDatabases(
            winbio_helpers::WINBIO_TYPE_FINGERPRINT,
            &mut schema_array,
            &mut schema_count,
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioEnumDatabases", &e))?;

        if schema_count == 0 {
            if !schema_array.is_null() {
                winbio_helpers::winbio_free(schema_array as *const _);
            }
            bail!("No biometric databases found");
        }

        if db_number > schema_count {
            let count = schema_count;
            if !schema_array.is_null() {
                winbio_helpers::winbio_free(schema_array as *const _);
            }
            bail!(
                "Database number {} is out of range (found {} database(s))",
                db_number,
                count
            );
        }

        let schemas = std::slice::from_raw_parts(schema_array, schema_count);
        let schema = &schemas[db_number - 1];

        let file_path = winbio_helpers::wchar_to_string(&schema.FilePath);
        let db_id = format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            schema.DatabaseId.data1,
            schema.DatabaseId.data2,
            schema.DatabaseId.data3,
            schema.DatabaseId.data4[0],
            schema.DatabaseId.data4[1],
            schema.DatabaseId.data4[2],
            schema.DatabaseId.data4[3],
            schema.DatabaseId.data4[4],
            schema.DatabaseId.data4[5],
            schema.DatabaseId.data4[6],
            schema.DatabaseId.data4[7]
        );

        if !schema_array.is_null() {
            winbio_helpers::winbio_free(schema_array as *const _);
        }

        (file_path, db_id)
    };

    print_info("Target", &format!("Database {} — {}", db_number, db_id));
    if !file_path.is_empty() {
        print_info("File", &file_path);
    }

    let mut actions: Vec<&str> = Vec::new();
    if delete_file {
        actions.push("delete .DAT file");
    }
    if delete_registry {
        actions.push("delete registry entry");
    }
    print_info("Actions", &actions.join(", "));

    // Determine if we need to touch the file
    let file_exists = !file_path.is_empty() && std::path::Path::new(&file_path).exists();

    if delete_file && !file_path.is_empty() && file_exists {
        if let Ok(meta) = std::fs::metadata(&file_path) {
            print_info("File Size", &format!("{} bytes", meta.len()));
        }
    } else if delete_file && !file_path.is_empty() && !file_exists {
        print_warn("Database file does not exist on disk (already deleted or stored on-chip)");
    }

    // Stop the service (needed for both file and registry operations)
    print_step("Stopping WbioSrvc service...");
    let was_running = unsafe { stop_wbiosrvc()? };
    if was_running {
        print_pass("WbioSrvc stopped");
    } else {
        print_info("WbioSrvc", "was already stopped");
    }

    let mut any_error = false;

    // Delete the .DAT file
    if delete_file && file_exists {
        print_step(&format!("Deleting {}...", file_path));
        match std::fs::remove_file(&file_path) {
            Ok(()) => {
                print_pass("Database file deleted");
            }
            Err(e) => {
                print_fail(&format!("Failed to delete database file: {}", e));
                any_error = true;
            }
        }
    }

    // Delete the registry key
    if delete_registry {
        print_step(&format!(
            "Deleting registry key HKLM\\...\\WbioSrvc\\Databases\\{}...",
            db_id
        ));
        match delete_database_registry_key(&db_id) {
            Ok(()) => {
                print_pass("Registry entry deleted");
            }
            Err(e) => {
                print_fail(&format!("{}", e));
                any_error = true;
            }
        }
    }

    // Restart the service
    if was_running {
        print_step("Restarting WbioSrvc service...");
        unsafe { start_wbiosrvc()? };
        print_pass("WbioSrvc restarted");
    } else {
        print_info(
            "Note",
            "WbioSrvc was not running — start it manually if needed",
        );
    }

    if any_error {
        bail!("Some operations failed (see above)");
    }

    println!();
    if delete_registry {
        print_pass("Database unregistered");
        print_step("The database has been fully removed from the system");
    } else {
        print_pass("Database file deleted");
        print_step("Service will recreate a clean empty database");
        print_step("Re-enroll fingerprints via Windows Settings > Accounts > Sign-in options");
    }

    Ok(())
}
