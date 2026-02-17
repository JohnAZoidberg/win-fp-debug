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

    StartServiceW(service, None).map_err(|e| anyhow::anyhow!("Failed to start WbioSrvc: {}", e))?;

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

fn format_guid(guid: &windows::core::GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

struct DatabaseTarget {
    index: usize,
    db_id: String,
    file_path: String,
}

/// Enumerate registered databases and return the targets to operate on.
/// When `all` is true and no databases are registered, returns an empty vec instead of an error.
fn enumerate_targets(db_number: Option<usize>, all: bool) -> Result<Vec<DatabaseTarget>> {
    unsafe {
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
            if all {
                return Ok(Vec::new());
            }
            bail!("No biometric databases found");
        }

        let schemas = std::slice::from_raw_parts(schema_array, schema_count);

        let targets = match db_number {
            Some(n) => {
                if n == 0 || n > schema_count {
                    if !schema_array.is_null() {
                        winbio_helpers::winbio_free(schema_array as *const _);
                    }
                    bail!(
                        "Database number {} is out of range (found {} database(s))",
                        n,
                        schema_count
                    );
                }
                let schema = &schemas[n - 1];
                vec![DatabaseTarget {
                    index: n,
                    db_id: format_guid(&schema.DatabaseId),
                    file_path: winbio_helpers::wchar_to_string(&schema.FilePath),
                }]
            }
            None => schemas
                .iter()
                .enumerate()
                .map(|(i, schema)| DatabaseTarget {
                    index: i + 1,
                    db_id: format_guid(&schema.DatabaseId),
                    file_path: winbio_helpers::wchar_to_string(&schema.FilePath),
                })
                .collect(),
        };

        if !schema_array.is_null() {
            winbio_helpers::winbio_free(schema_array as *const _);
        }

        Ok(targets)
    }
}

/// Find .DAT files in the WINBIODATABASE directory that aren't in the registered set.
/// Returns the paths of orphaned files.
fn find_orphaned_dat_files(registered_paths: &[&str]) -> Vec<std::path::PathBuf> {
    let db_dir = std::path::Path::new(r"C:\WINDOWS\SYSTEM32\WINBIODATABASE");
    let entries = match std::fs::read_dir(db_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let registered_upper: std::collections::HashSet<String> =
        registered_paths.iter().map(|p| p.to_uppercase()).collect();

    let mut orphans = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("DAT")
            && !registered_upper.contains(&path.to_string_lossy().to_uppercase())
        {
            orphans.push(path);
        }
    }
    orphans
}

/// Process a single database target. Returns true if all operations succeeded.
fn process_target(target: &DatabaseTarget, delete_file: bool, delete_registry: bool) -> bool {
    let mut ok = true;

    println!();
    print_step(&format!("Database {} — {}", target.index, target.db_id));

    let file_exists =
        !target.file_path.is_empty() && std::path::Path::new(&target.file_path).exists();

    if delete_file {
        if file_exists {
            if let Ok(meta) = std::fs::metadata(&target.file_path) {
                print_info("  File Size", &format!("{} bytes", meta.len()));
            }
            match std::fs::remove_file(&target.file_path) {
                Ok(()) => print_pass(&format!("  Deleted {}", target.file_path)),
                Err(e) => {
                    print_fail(&format!("  Failed to delete file: {}", e));
                    ok = false;
                }
            }
        } else if !target.file_path.is_empty() {
            print_info("  File", "does not exist (already deleted or on-chip)");
        }
    }

    if delete_registry {
        match delete_database_registry_key(&target.db_id) {
            Ok(()) => print_pass("  Registry entry deleted"),
            Err(e) => {
                print_fail(&format!("  {}", e));
                ok = false;
            }
        }
    }

    ok
}

pub fn run_delete_database(
    db_number: Option<usize>,
    all: bool,
    delete_file: bool,
    delete_registry: bool,
) -> Result<()> {
    if !delete_file && !delete_registry {
        bail!("Specify --file to delete the .DAT file, --registry to remove the registry entry, or both");
    }

    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    let targets = enumerate_targets(if all { None } else { db_number }, all)?;

    // When --all --file, also find orphaned .DAT files not in the registered set
    let orphans = if all && delete_file {
        let registered: Vec<&str> = targets.iter().map(|t| t.file_path.as_str()).collect();
        find_orphaned_dat_files(&registered)
    } else {
        Vec::new()
    };

    let total_work = targets.len() + orphans.len();
    if total_work == 0 {
        println!();
        print_info(
            "Nothing to do",
            "no registered databases or .DAT files found",
        );
        return Ok(());
    }

    if all {
        print_header(&format!(
            "Delete All Biometric Databases ({}{})",
            if targets.is_empty() {
                String::new()
            } else {
                format!("{} registered", targets.len())
            },
            if !orphans.is_empty() {
                if targets.is_empty() {
                    format!("{} orphaned", orphans.len())
                } else {
                    format!(", {} orphaned", orphans.len())
                }
            } else {
                String::new()
            }
        ));
    } else {
        print_header("Delete Biometric Database");
    }

    let mut actions: Vec<&str> = Vec::new();
    if delete_file {
        actions.push("delete .DAT file(s)");
    }
    if delete_registry {
        actions.push("delete registry entry/entries");
    }
    print_info("Actions", &actions.join(", "));

    if all {
        for t in &targets {
            print_info(
                &format!("  Database {}", t.index),
                &format!(
                    "{} — {}",
                    t.db_id,
                    if t.file_path.is_empty() {
                        "(no file)"
                    } else {
                        &t.file_path
                    }
                ),
            );
        }
        for path in &orphans {
            print_info("  Orphan", &path.to_string_lossy());
        }
    } else {
        let t = &targets[0];
        print_info("Target", &format!("Database {} — {}", t.index, t.db_id));
        if !t.file_path.is_empty() {
            print_info("File", &t.file_path);
        }
    }

    // Stop the service
    print_step("Stopping WbioSrvc service...");
    let was_running = unsafe { stop_wbiosrvc()? };
    if was_running {
        print_pass("WbioSrvc stopped");
    } else {
        print_info("WbioSrvc", "was already stopped");
    }

    // Process registered targets
    let mut any_error = false;
    for target in &targets {
        if !process_target(target, delete_file, delete_registry) {
            any_error = true;
        }
    }

    // Delete orphaned files
    for path in &orphans {
        println!();
        print_step(&format!("Orphan: {}", path.display()));
        match std::fs::remove_file(path) {
            Ok(()) => print_pass(&format!("  Deleted {}", path.display())),
            Err(e) => {
                print_fail(&format!("  Failed to delete: {}", e));
                any_error = true;
            }
        }
    }

    // Restart the service
    if was_running {
        println!();
        print_step("Restarting WbioSrvc service...");
        unsafe { start_wbiosrvc()? };
        print_pass("WbioSrvc restarted");
    } else {
        println!();
        print_info(
            "Note",
            "WbioSrvc was not running — start it manually if needed",
        );
    }

    if any_error {
        bail!("Some operations failed (see above)");
    }

    println!();
    let file_count = targets.iter().filter(|t| !t.file_path.is_empty()).count() + orphans.len();
    if delete_registry && targets.is_empty() && !orphans.is_empty() {
        print_pass(&format!("{} orphaned file(s) deleted", orphans.len()));
    } else if delete_registry {
        print_pass(&format!("{} database(s) unregistered", targets.len()));
        if !orphans.is_empty() {
            print_pass(&format!("{} orphaned file(s) deleted", orphans.len()));
        }
        print_step("Databases have been fully removed from the system");
    } else {
        print_pass(&format!("{} database file(s) deleted", file_count));
        if !targets.is_empty() {
            print_step("Service will recreate clean empty databases");
        }
    }
    if delete_file && was_running {
        print_warn(
            "WbioSrvc was restarted and may recreate .DAT files for active sensors. \
             To prevent this, use stop-service before deleting and start-service after.",
        );
    }
    print_step("Re-enroll fingerprints via Windows Settings > Accounts > Sign-in options");

    Ok(())
}
