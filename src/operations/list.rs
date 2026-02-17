use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_list() -> Result<()> {
    print_header("List Enrolled Fingerprints");

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;
    print_step("Session opened with focus. Touch the sensor to identify yourself...");

    unsafe {
        // First identify the user
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
                anyhow::anyhow!("No match — finger not enrolled. Cannot list enrollments.")
            } else {
                crate::error::wrap_winbio_error("WinBioIdentify", &e)
            }
        })?;

        print_pass("User identified on sensor");
        print_info("Unit ID", &unit_id.to_string());

        // Now enumerate enrollments for this identity
        let mut subfactor_array: *mut u8 = std::ptr::null_mut();
        let mut subfactor_count: usize = 0;

        WinBioEnumEnrollments(
            guard.session,
            unit_id,
            &identity,
            &mut subfactor_array,
            Some(&mut subfactor_count),
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioEnumEnrollments", &e))?;

        if subfactor_count == 0 {
            print_warn("No enrolled fingerprints found for this identity");
        } else {
            print_pass(&format!("{} fingerprint(s) enrolled", subfactor_count));
            let subfactors = std::slice::from_raw_parts(subfactor_array, subfactor_count);
            for (i, &sf) in subfactors.iter().enumerate() {
                print_info(
                    &format!("  {}.", i + 1),
                    &format!("Finger {} — {}", sf, winbio_helpers::subfactor_name(sf)),
                );
            }
        }

        if !subfactor_array.is_null() {
            winbio_helpers::winbio_free(subfactor_array as *const _);
        }
    }

    Ok(())
}
