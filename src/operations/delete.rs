use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_delete(finger: u8) -> Result<()> {
    print_header(&format!(
        "Delete Fingerprint — Finger {} ({})",
        finger,
        winbio_helpers::subfactor_name(finger)
    ));

    if !(1..=10).contains(&finger) {
        print_fail("Finger must be 1–10");
        return Ok(());
    }

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;

    // Identify user first
    print_step("Touch the sensor to identify yourself...");

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
        print_step(&format!(
            "Deleting finger {} ({}) from unit {}...",
            finger,
            winbio_helpers::subfactor_name(finger),
            unit_id
        ));

        let result = WinBioDeleteTemplate(guard.session, unit_id, &identity, finger);

        if let Err(e) = result {
            let code = crate::error::error_code(&e);
            if code == 0x8009_8016 {
                // WINBIO_E_DATABASE_NO_SUCH_RECORD
                print_fail("No enrollment found for that finger — nothing to delete");
            } else {
                return Err(crate::error::wrap_winbio_error("WinBioDeleteTemplate", &e));
            }
            return Ok(());
        }

        print_pass(&format!(
            "Successfully deleted finger {} ({})",
            finger,
            winbio_helpers::subfactor_name(finger)
        ));
    }

    Ok(())
}
