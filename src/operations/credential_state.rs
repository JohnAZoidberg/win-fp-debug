use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_credential_state() -> Result<()> {
    print_header("Credential State Check");

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;
    print_step("Session opened with focus. Touch the sensor to identify yourself...");

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
        .map_err(|e| {
            let code = crate::error::error_code(&e);
            if code == 0x8009_8005 {
                anyhow::anyhow!("No match — finger not enrolled. Cannot check credential state.")
            } else {
                crate::error::wrap_winbio_error("WinBioIdentify", &e)
            }
        })?;

        print_pass("User identified on sensor");
        print_info("Unit ID", &unit_id.to_string());
        print_info("Finger", &winbio_helpers::subfactor_name(subfactor));

        let credential_state = WinBioGetCredentialState(identity, WINBIO_CREDENTIAL_PASSWORD)
            .map_err(|e| crate::error::wrap_winbio_error("WinBioGetCredentialState", &e))?;

        println!();
        if credential_state == WINBIO_CREDENTIAL_SET {
            print_pass("Password credential is SET — Windows Hello login should work");
        } else if credential_state == WINBIO_CREDENTIAL_NOT_SET {
            print_fail("Password credential is NOT SET");
            print_warn("This means no password hash is linked to the biometric identity.");
            print_warn(
                "This is a common cause of \"fingerprint enrolled but login doesn't work.\"",
            );
            print_step(
                "Try: Settings → Accounts → Sign-in options → Remove and re-add fingerprint.",
            );
        } else {
            print_info(
                "Credential state",
                &format!("Unknown ({})", credential_state.0),
            );
        }
    }

    Ok(())
}
