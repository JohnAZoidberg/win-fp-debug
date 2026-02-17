use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_verify(finger: u8) -> Result<()> {
    print_header(&format!(
        "Verify Finger {} ({})",
        finger,
        winbio_helpers::subfactor_name(finger)
    ));

    if !(1..=10).contains(&finger) {
        print_fail("Finger must be 1–10");
        return Ok(());
    }

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;

    // First identify to get the WINBIO_IDENTITY
    print_step("Touch the sensor to identify yourself first...");

    unsafe {
        let mut unit_id = 0u32;
        let mut identity = WINBIO_IDENTITY::default();
        let mut subfactor = 0u8;
        let mut reject_detail = 0u32;

        WinBioIdentify(
            guard.session,
            Some(&mut unit_id),
            Some(&mut identity),
            Some(&mut subfactor),
            Some(&mut reject_detail),
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioIdentify", &e))?;

        print_pass("User identified");

        // Now verify the specific finger
        print_step(&format!(
            "Now touch with finger {} ({}) to verify...",
            finger,
            winbio_helpers::subfactor_name(finger)
        ));

        let mut match_result: u8 = 0;
        let mut verify_reject = 0u32;

        let result = WinBioVerify(
            guard.session,
            &identity,
            finger,
            Some(&mut unit_id),
            Some(&mut match_result),
            Some(&mut verify_reject),
        );

        if let Err(e) = result {
            let code = crate::error::error_code(&e);
            if code == 0x8009_8005 {
                print_fail("Verification failed — NO MATCH");
                if verify_reject != 0 {
                    print_info(
                        "Reject reason",
                        winbio_helpers::reject_reason(verify_reject),
                    );
                }
                return Ok(());
            }
            if code == 0x8009_8008 {
                print_fail("Bad capture — try again");
                print_info(
                    "Reject reason",
                    winbio_helpers::reject_reason(verify_reject),
                );
                return Ok(());
            }
            return Err(crate::error::wrap_winbio_error("WinBioVerify", &e));
        }

        if match_result != 0 {
            print_pass("Verification SUCCEEDED — finger matches");
        } else {
            print_fail("Verification FAILED — finger does not match");
        }
    }

    Ok(())
}
