use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_identify() -> Result<()> {
    print_header("Identify (touch sensor)");

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;
    print_step("Session opened with focus. Touch the sensor now...");

    unsafe {
        let mut unit_id = 0u32;
        let mut identity = WINBIO_IDENTITY::default();
        let mut subfactor = 0u8;
        let mut reject_detail = 0u32;

        let result = WinBioIdentify(
            guard.session,
            Some(&mut unit_id),
            Some(&mut identity),
            Some(&mut subfactor),
            Some(&mut reject_detail),
        );

        if let Err(e) = result {
            let code = crate::error::error_code(&e);
            if code == 0x8009_8005 {
                // WINBIO_E_NO_MATCH
                print_fail("No match — finger not enrolled");
                if reject_detail != 0 {
                    print_info(
                        "Reject reason",
                        winbio_helpers::reject_reason(reject_detail),
                    );
                }
            } else if code == 0x8009_8008 {
                // WINBIO_E_BAD_CAPTURE
                print_fail("Bad capture — try again");
                print_info(
                    "Reject reason",
                    winbio_helpers::reject_reason(reject_detail),
                );
            } else {
                return Err(crate::error::wrap_winbio_error("WinBioIdentify", &e));
            }
            return Ok(());
        }

        print_pass("Finger identified successfully");
        print_info("Unit ID", &unit_id.to_string());
        print_info("Finger", &winbio_helpers::subfactor_name(subfactor));

        // Print identity info
        if identity.Type == winbio_helpers::WINBIO_ID_TYPE_SID {
            let sid_data = &identity.Value.AccountSid;
            let size = sid_data.Size as usize;
            let bytes = &sid_data.Data[..size.min(sid_data.Data.len())];
            let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
            print_info("Identity (SID)", &hex.join(" "));
        } else {
            print_info("Identity type", &format!("{}", identity.Type));
        }
    }

    Ok(())
}
