use anyhow::Result;
use std::process::Command;

use crate::output::*;

pub fn check_hardware() -> Result<()> {
    print_header("Level 1: Hardware Detection (PnP Biometric Devices)");

    // Use Format-List and string conversion to avoid JSON enum serialization issues
    let ps_script = r#"
        $devs = Get-PnpDevice -Class Biometric -ErrorAction SilentlyContinue
        if ($null -eq $devs) { exit 0 }
        $devs | ForEach-Object {
            [PSCustomObject]@{
                FriendlyName = [string]$_.FriendlyName
                InstanceId   = [string]$_.InstanceId
                Status       = [string]$_.Status
                Problem      = [string]$_.Problem
                Class        = [string]$_.Class
                Manufacturer = [string]$_.Manufacturer
            }
        } | ConvertTo-Json -Compress
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            print_fail(&format!("Failed to run PowerShell: {}", e));
            return Ok(());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();

    if stdout.is_empty() {
        print_fail("No biometric PnP devices found");
        print_step("Check Device Manager > Biometric devices");
        return Ok(());
    }

    // Parse JSON — PowerShell returns a single object (not array) for one device
    let devices: Vec<serde_json::Value> = if stdout.starts_with('[') {
        serde_json::from_str(stdout)?
    } else {
        vec![serde_json::from_str(stdout)?]
    };

    if devices.is_empty() {
        print_fail("No biometric PnP devices found");
        print_step("Check Device Manager > Biometric devices");
        return Ok(());
    }

    print_pass(&format!("Found {} biometric device(s)", devices.len()));

    for (i, dev) in devices.iter().enumerate() {
        let name = dev["FriendlyName"].as_str().unwrap_or("(unknown)");
        let manufacturer = dev["Manufacturer"].as_str().unwrap_or("(unknown)");
        let instance_id = dev["InstanceId"].as_str().unwrap_or("(unknown)");
        let status = dev["Status"].as_str().unwrap_or("Unknown");
        let problem = dev["Problem"].as_str().unwrap_or("");

        println!();
        print_info(&format!("  Device {}", i + 1), name);
        print_info("    Manufacturer", manufacturer);
        print_info("    Instance ID", instance_id);

        if status == "OK" {
            print_pass(&format!("    Status: {}", status));
        } else {
            print_fail(&format!("    Status: {}", status));
            if !problem.is_empty() && problem != "0" {
                print_info("    Problem", problem);
            }
        }
    }

    Ok(())
}
