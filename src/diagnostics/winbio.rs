use anyhow::Result;
use std::process::Command;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers::*;

pub fn check_sensor() -> Result<()> {
    print_header("Level 3: WinBio Sensor Enumeration");

    unsafe {
        // Enumerate biometric units
        let mut unit_array: *mut WINBIO_UNIT_SCHEMA = std::ptr::null_mut();
        let mut unit_count: usize = 0;

        let result =
            WinBioEnumBiometricUnits(WINBIO_TYPE_FINGERPRINT, &mut unit_array, &mut unit_count);

        if let Err(e) = result {
            print_fail(&format!(
                "WinBioEnumBiometricUnits failed: {} (0x{:08X})",
                crate::error::hresult_message(e.code()),
                e.code().0 as u32
            ));
            return Ok(());
        }

        if unit_count == 0 {
            print_fail("No fingerprint biometric units found");
            winbio_free(unit_array as *const _);

            // Run follow-up diagnostics to surface the root cause
            println!();
            check_winbio_events();
            check_database_config();

            return Ok(());
        }

        print_pass(&format!("Found {} biometric unit(s)", unit_count));

        let units = std::slice::from_raw_parts(unit_array, unit_count);

        for (i, unit) in units.iter().enumerate() {
            println!();
            print_info(&format!("  Unit {}", i + 1), "");
            print_info("    Unit ID", &unit.UnitId.to_string());
            print_info(
                "    Pool type",
                match unit.PoolType {
                    1 => "System",
                    2 => "Private",
                    _ => "Unknown",
                },
            );
            print_info(
                "    Biometric factor",
                &format!("0x{:08X}", unit.BiometricFactor),
            );
            print_info(
                "    Sensor subtype",
                sensor_subtype_name(unit.SensorSubType),
            );
            print_info("    Capabilities", &capabilities_string(unit.Capabilities));

            let description = wchar_to_string(&unit.Description);
            let manufacturer = wchar_to_string(&unit.Manufacturer);
            let model = wchar_to_string(&unit.Model);
            let serial = wchar_to_string(&unit.SerialNumber);
            let firmware = format!(
                "{}.{}",
                unit.FirmwareVersion.MajorVersion, unit.FirmwareVersion.MinorVersion
            );

            print_info("    Description", &description);
            print_info("    Manufacturer", &manufacturer);
            print_info("    Model", &model);
            print_info(
                "    Serial number",
                if serial.is_empty() { "(none)" } else { &serial },
            );
            print_info("    Firmware version", &firmware);
        }

        winbio_free(unit_array as *const _);

        // Test session open/close
        println!();
        print_step("Testing WinBio session open/close...");
        match open_session(WINBIO_FLAG_DEFAULT) {
            Ok(session) => {
                print_pass("WinBioOpenSession succeeded");
                close_session(session);
                print_pass("WinBioCloseSession succeeded");
            }
            Err(e) => {
                print_fail(&format!("WinBioOpenSession failed: {}", e));
            }
        }
    }

    Ok(())
}

/// Check the WinBio operational event log for recent configuration errors.
fn check_winbio_events() {
    print_step("Checking WinBio event log...");

    let ps_script = r#"
        try {
            $events = Get-WinEvent -LogName 'Microsoft-Windows-Biometrics/Operational' -MaxEvents 20 -ErrorAction Stop
            $errors = $events | Where-Object { $_.Id -in @(1106, 1109) }
            $errors | ForEach-Object {
                [PSCustomObject]@{
                    Id      = [int]$_.Id
                    Level   = [int]$_.Level
                    Message = [string]$_.Message
                }
            } | ConvertTo-Json -Compress
        } catch {
            # Log may not exist or be inaccessible — silently return nothing
        }
    "#;

    let output = match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();

    if stdout.is_empty() {
        print_pass("No WinBio configuration errors in event log");
        return;
    }

    let events: Vec<serde_json::Value> = if stdout.starts_with('[') {
        match serde_json::from_str(stdout) {
            Ok(v) => v,
            Err(_) => return,
        }
    } else {
        match serde_json::from_str(stdout) {
            Ok(v) => vec![v],
            Err(_) => return,
        }
    };

    for event in &events {
        let id = event["Id"].as_i64().unwrap_or(0);
        let level = event["Level"].as_i64().unwrap_or(0);
        let message = event["Message"]
            .as_str()
            .unwrap_or("(no message)")
            .lines()
            .next()
            .unwrap_or("(no message)");

        let line = format!("Event {}: {}", id, message);
        if level <= 2 {
            print_fail(&line);
        } else {
            print_warn(&line);
        }
    }
}

/// Check each biometric device's WinBio DatabaseId references against registered databases.
fn check_database_config() {
    println!();
    print_step("Checking device database configuration...");

    // This script:
    // 1. Gets all biometric PnP devices
    // 2. For each, reads WinBio\Configurations\*\DatabaseId values
    // 3. Lists registered databases under WbioSrvc\Databases
    // 4. Outputs a JSON structure with the comparison results
    let ps_script = r#"
        $dbBasePath = 'HKLM:\SYSTEM\CurrentControlSet\Services\WbioSrvc\Databases'
        $registeredDbs = @()
        if (Test-Path $dbBasePath) {
            $registeredDbs = Get-ChildItem $dbBasePath -ErrorAction SilentlyContinue |
                ForEach-Object { $_.PSChildName.ToLower() }
        }

        $devs = Get-PnpDevice -Class Biometric -ErrorAction SilentlyContinue
        if ($null -eq $devs) { exit 0 }

        $results = @()
        foreach ($dev in $devs) {
            $id = [string]$dev.InstanceId
            $name = [string]$dev.FriendlyName
            $configBase = "HKLM:\SYSTEM\CurrentControlSet\Enum\$id\Device Parameters\WinBio\Configurations"
            if (-not (Test-Path $configBase)) { continue }

            $configs = @()
            Get-ChildItem $configBase -ErrorAction SilentlyContinue | ForEach-Object {
                $configName = $_.PSChildName
                $dbId = (Get-ItemProperty -Path $_.PSPath -Name 'DatabaseId' -ErrorAction SilentlyContinue).DatabaseId
                if ($dbId) {
                    $dbIdLower = $dbId.ToLower()
                    $registered = $registeredDbs -contains $dbIdLower
                    $configs += [PSCustomObject]@{
                        ConfigName = [string]$configName
                        DatabaseId = [string]$dbId
                        Registered = [bool]$registered
                    }
                }
            }

            if ($configs.Count -gt 0) {
                $results += [PSCustomObject]@{
                    FriendlyName = $name
                    InstanceId   = $id
                    Configurations = $configs
                }
            }
        }

        if ($results.Count -gt 0) {
            $results | ConvertTo-Json -Depth 4 -Compress
        }
    "#;

    let output = match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            print_fail(&format!("Failed to run PowerShell: {}", e));
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();

    if stdout.is_empty() {
        print_step("No biometric devices with WinBio configuration found");
        return;
    }

    let devices: Vec<serde_json::Value> = if stdout.starts_with('[') {
        match serde_json::from_str(stdout) {
            Ok(v) => v,
            Err(_) => return,
        }
    } else {
        match serde_json::from_str(stdout) {
            Ok(v) => vec![v],
            Err(_) => return,
        }
    };

    let mut any_mismatch = false;

    for dev in &devices {
        let name = dev["FriendlyName"].as_str().unwrap_or("(unknown)");
        let instance_id = dev["InstanceId"].as_str().unwrap_or("(unknown)");
        println!("  Device: {} ({})", name, instance_id);

        let configs = match dev["Configurations"].as_array() {
            Some(c) => c,
            None => continue,
        };

        for config in configs {
            let config_name = config["ConfigName"].as_str().unwrap_or("?");
            let db_id = config["DatabaseId"].as_str().unwrap_or("?");
            let registered = config["Registered"].as_bool().unwrap_or(false);

            println!("    Configuration {} DatabaseId: {}", config_name, db_id);
            if registered {
                print_pass("    Registered in WbioSrvc\\Databases");
            } else {
                print_fail("    Not registered in WbioSrvc\\Databases");
                any_mismatch = true;
            }
        }
    }

    if any_mismatch {
        print_step("Reinstall the fingerprint sensor driver to recreate missing database entries");
    }
}
