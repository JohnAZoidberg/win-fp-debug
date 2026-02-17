use anyhow::Result;
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
            print_step("Ensure the fingerprint sensor driver is installed and WbioSrvc is running");
            winbio_free(unit_array as *const _);
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
