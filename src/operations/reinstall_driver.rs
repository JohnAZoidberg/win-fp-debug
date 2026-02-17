use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::output::*;

/// Device info returned from PowerShell Get-PnpDevice.
struct BiometricDevice {
    friendly_name: String,
    instance_id: String,
}

/// Find all biometric PnP devices using PowerShell Get-PnpDevice.
fn find_biometric_devices() -> Result<Vec<BiometricDevice>> {
    let ps_script = r#"
        $devs = Get-PnpDevice -Class Biometric -ErrorAction SilentlyContinue |
            Where-Object { $_.Status -eq 'OK' -or $_.Status -eq 'Error' -or $_.Status -eq 'Degraded' -or $_.Status -eq 'Unknown' }
        if ($null -eq $devs) { exit 0 }
        $devs | ForEach-Object {
            [PSCustomObject]@{
                FriendlyName = [string]$_.FriendlyName
                InstanceId   = [string]$_.InstanceId
                Status       = [string]$_.Status
            }
        } | ConvertTo-Json -Compress
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run PowerShell: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();

    if stdout.is_empty() {
        return Ok(vec![]);
    }

    // PowerShell returns a single object (not array) for one result
    let raw: Vec<serde_json::Value> = if stdout.starts_with('[') {
        serde_json::from_str(stdout)?
    } else {
        let single: serde_json::Value = serde_json::from_str(stdout)?;
        vec![single]
    };

    let devices = raw
        .into_iter()
        .filter_map(|v| {
            let friendly_name = v.get("FriendlyName")?.as_str()?.to_string();
            let instance_id = v.get("InstanceId")?.as_str()?.to_string();
            Some(BiometricDevice {
                friendly_name,
                instance_id,
            })
        })
        .collect();

    Ok(devices)
}

/// Get the OEM INF name (e.g. "oem50.inf") for a device by its instance ID.
fn get_driver_inf_name(instance_id: &str) -> Result<String> {
    let escaped_id = instance_id.replace('\'', "''");
    let ps_script = format!(
        "(Get-PnpDeviceProperty -InstanceId '{}' -KeyName 'DEVPKEY_Device_DriverInfPath').Data",
        escaped_id
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run PowerShell: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        bail!(
            "Could not determine driver INF for device '{}'",
            instance_id
        );
    }

    Ok(stdout)
}

/// Export the driver package from the driver store to a local directory.
/// Returns the path to the .inf file inside the export directory.
fn export_driver(oem_inf: &str, dest_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)?;

    let output = Command::new("pnputil")
        .args(["/export-driver", oem_inf, &dest_dir.to_string_lossy()])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run pnputil /export-driver: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "pnputil /export-driver failed: {} {}",
            stdout.trim(),
            stderr.trim()
        );
    }

    // Find the .inf file in the export directory
    for entry in std::fs::read_dir(dest_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "inf") {
            return Ok(path);
        }
    }

    bail!("No .inf file found in exported driver directory");
}

pub fn run_reinstall_driver() -> Result<()> {
    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    print_header("Reinstall Biometric Driver");

    // Step 1: Find biometric devices
    print_step("Scanning for biometric devices...");
    let devices = find_biometric_devices()?;

    if devices.is_empty() {
        bail!("No biometric devices found. Run 'check-hardware' to inspect PnP state.");
    }

    let device = &devices[0];
    print_step(&format!(
        "Found: {} ({})",
        device.friendly_name, device.instance_id
    ));

    // Step 2: Identify the driver package
    print_step("Identifying driver package...");
    let oem_inf = get_driver_inf_name(&device.instance_id)?;
    print_info("Driver INF", &oem_inf);

    // Step 3: Export/backup the driver package before removing anything
    print_step("Backing up driver package...");
    let temp_dir = std::env::temp_dir().join("win-fp-debug-driver-backup");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    let inf_path = export_driver(&oem_inf, &temp_dir)?;
    print_pass(&format!("Driver backed up to {}", temp_dir.display()));

    // Step 4: Delete driver from store AND uninstall from devices.
    // Using /uninstall keeps the device node alive (avoids USB re-enumeration)
    // but removes the driver, so re-adding it triggers a full INF install.
    print_step("Uninstalling driver from device and store...");
    let del_output = Command::new("pnputil")
        .args(["/delete-driver", &oem_inf, "/uninstall", "/force"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run pnputil /delete-driver: {}", e))?;

    let del_stdout = String::from_utf8_lossy(&del_output.stdout);
    let del_stderr = String::from_utf8_lossy(&del_output.stderr);
    if del_output.status.success() {
        print_pass("Driver uninstalled and removed from store");
    } else {
        print_warn(&format!(
            "pnputil /delete-driver /uninstall: {} {}",
            del_stdout.trim(),
            del_stderr.trim()
        ));
    }

    // Step 5: Re-add the driver and install on matching devices.
    // The device node still exists (driverless), so /install triggers full INF
    // processing including AddReg sections that create WinBio database entries.
    print_step("Reinstalling driver...");
    let add_output = Command::new("pnputil")
        .args(["/add-driver", &inf_path.to_string_lossy(), "/install"])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run pnputil /add-driver: {}", e))?;

    let add_stdout = String::from_utf8_lossy(&add_output.stdout);
    let add_stderr = String::from_utf8_lossy(&add_output.stderr);
    if add_output.status.success() {
        print_pass("Driver reinstalled");
        // Show pnputil output for transparency
        let msg = add_stdout.trim();
        if !msg.is_empty() {
            for line in msg.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    print_info("pnputil", line);
                }
            }
        }
    } else {
        bail!(
            "pnputil /add-driver /install failed: {} {}",
            add_stdout.trim(),
            add_stderr.trim()
        );
    }

    // Step 6: Verify the device is back with a driver
    print_step("Verifying device status...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    let after = find_biometric_devices()?;
    if after.is_empty() {
        print_fail("No biometric device found after reinstallation");
    } else {
        for dev in &after {
            print_pass(&format!(
                "Device present: {} ({})",
                dev.friendly_name, dev.instance_id
            ));
        }
    }

    // Clean up backup
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    println!();
    print_step("Driver reinstallation complete. Run 'diagnose' to verify.");

    Ok(())
}
