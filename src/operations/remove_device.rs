use anyhow::{bail, Result};
use std::process::Command;

use crate::output::*;

/// Find phantom biometric devices using PowerShell Get-PnpDevice.
/// Returns a list of instance IDs where Problem == "CM_PROB_PHANTOM" (code 45).
fn find_phantom_biometric_devices() -> Result<Vec<String>> {
    let ps_script = r#"
        $devs = Get-PnpDevice -Class Biometric -ErrorAction SilentlyContinue
        if ($null -eq $devs) { exit 0 }
        $devs | Where-Object { $_.Problem -eq 'CM_PROB_PHANTOM' -or [string]$_.Problem -eq '45' } |
            ForEach-Object { [string]$_.InstanceId } |
            ConvertTo-Json -Compress
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

    // PowerShell returns a single string (not array) for one result
    let ids: Vec<String> = if stdout.starts_with('[') {
        serde_json::from_str(stdout)?
    } else {
        let single: String = serde_json::from_str(stdout)?;
        vec![single]
    };

    Ok(ids)
}

/// Remove a single device by instance ID using CfgMgr32 APIs.
fn remove_device_by_instance_id(instance_id: &str) -> Result<()> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = instance_id.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut devnode: u32 = 0;
        let cr = CM_Locate_DevNodeW(
            &mut devnode,
            PCWSTR(wide.as_ptr()),
            CM_LOCATE_DEVNODE_PHANTOM,
        );
        if cr != CONFIGRET(0) {
            bail!(
                "CM_Locate_DevNodeW failed for '{}': CONFIGRET={}",
                instance_id,
                cr.0
            );
        }

        let cr = CM_Uninstall_DevNode(devnode, 0);
        if cr != CONFIGRET(0) {
            bail!(
                "CM_Uninstall_DevNode failed for '{}': CONFIGRET={}",
                instance_id,
                cr.0
            );
        }
    }

    Ok(())
}

pub fn run_remove_device(instance_id: Option<String>, phantom: bool) -> Result<()> {
    if !instance_id.is_some() && !phantom {
        bail!("Either --instance-id <ID> or --phantom is required");
    }

    if !crate::elevation::is_elevated()? {
        bail!("This command requires Administrator privileges. Re-run as Administrator.");
    }

    print_header("Remove PnP Device");

    let targets: Vec<String> = if let Some(id) = instance_id {
        vec![id]
    } else {
        print_step("Scanning for phantom biometric devices...");
        let ids = find_phantom_biometric_devices()?;
        if ids.is_empty() {
            print_pass("No phantom biometric devices found");
            return Ok(());
        }
        print_info("Found", &format!("{} phantom device(s)", ids.len()));
        ids
    };

    let mut removed = 0u32;
    let mut failed = 0u32;

    for id in &targets {
        print_step(&format!("Removing: {}", id));
        match remove_device_by_instance_id(id) {
            Ok(()) => {
                print_pass(&format!("Removed: {}", id));
                removed += 1;
            }
            Err(e) => {
                print_fail(&format!("Failed to remove {}: {}", id, e));
                failed += 1;
            }
        }
    }

    println!();
    print_info("Summary", &format!("{} removed, {} failed", removed, failed));

    Ok(())
}
