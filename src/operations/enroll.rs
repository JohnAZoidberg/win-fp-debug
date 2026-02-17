use anyhow::Result;
use windows::core::HRESULT;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

// Raw FFI binding for WinBioEnrollCapture so we can inspect the HRESULT directly.
// The windows crate wraps all success HRESULTs (including WINBIO_I_MORE_DATA = 0x00098001)
// as Ok(()), losing the distinction between "template complete" and "need more samples".
unsafe extern "system" {
    fn WinBioEnrollCapture(
        SessionHandle: u32,
        RejectDetail: *mut u32,
    ) -> HRESULT;
}

const WINBIO_I_MORE_DATA: HRESULT = HRESULT(0x0009_0001_u32 as i32);
const WINBIO_E_BAD_CAPTURE: HRESULT = HRESULT(0x8009_8008_u32 as i32);

const MAX_SAMPLES: u32 = 20;

pub fn run_enroll(finger: u8) -> Result<()> {
    print_header(&format!(
        "Enroll Fingerprint — Finger {} ({})",
        finger,
        winbio_helpers::subfactor_name(finger)
    ));

    if !(1..=10).contains(&finger) {
        print_fail("Finger must be 1–10");
        return Ok(());
    }

    crate::elevation::warn_if_not_elevated();

    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_DEFAULT, true)?;

    // Get the first fingerprint sensor unit ID via enumeration.
    // This works even when no fingers are enrolled (unlike the identify-first approach).
    let unit_id = get_first_unit_id()?;
    print_info("Using sensor unit", &unit_id.to_string());

    unsafe {
        // Begin enrollment
        print_step(&format!(
            "Starting enrollment for finger {} ({})...",
            finger,
            winbio_helpers::subfactor_name(finger)
        ));

        if let Err(e) = WinBioEnrollBegin(guard.session, finger, unit_id) {
            return Err(crate::error::wrap_winbio_error("WinBioEnrollBegin", &e));
        }

        // Capture loop
        let mut sample_num = 0u32;
        loop {
            sample_num += 1;
            if sample_num > MAX_SAMPLES {
                print_fail("Too many capture attempts — discarding enrollment");
                let _ = WinBioEnrollDiscard(guard.session);
                return Ok(());
            }

            print_step(&format!("Touch the sensor (sample {})...", sample_num));

            let mut reject_detail = 0u32;
            let hr = WinBioEnrollCapture(guard.session, &mut reject_detail);

            if hr == HRESULT(0) {
                // S_OK — template complete
                print_pass("Template complete");
                break;
            } else if hr == WINBIO_I_MORE_DATA {
                print_info("  Status", "Good sample — more needed");
                continue;
            } else if hr == WINBIO_E_BAD_CAPTURE {
                print_warn(&format!(
                    "Bad capture: {} — try again",
                    winbio_helpers::reject_reason(reject_detail)
                ));
                continue;
            } else {
                // Unexpected error — discard and bail
                let err = windows::core::Error::from(hr);
                print_fail(&format!(
                    "WinBioEnrollCapture failed: {} (0x{:08X})",
                    crate::error::hresult_message(hr),
                    hr.0 as u32
                ));
                let _ = WinBioEnrollDiscard(guard.session);
                return Err(crate::error::wrap_winbio_error("WinBioEnrollCapture", &err));
            }
        }

        // Commit the enrollment
        print_step("Committing enrollment...");
        let mut identity = WINBIO_IDENTITY::default();
        let mut is_new_template: u8 = 0;

        if let Err(e) = WinBioEnrollCommit(
            guard.session,
            Some(&mut identity),
            Some(&mut is_new_template),
        ) {
            let code = crate::error::error_code(&e);
            let _ = WinBioEnrollDiscard(guard.session);
            if code == 0x8009_8015 {
                print_fail("Duplicate enrollment — this finger is already enrolled");
                return Ok(());
            }
            return Err(crate::error::wrap_winbio_error("WinBioEnrollCommit", &e));
        }

        print_pass(&format!(
            "Finger {} ({}) enrolled successfully",
            finger,
            winbio_helpers::subfactor_name(finger)
        ));
        print_info(
            "Template status",
            if is_new_template != 0 {
                "New template created"
            } else {
                "Existing template updated"
            },
        );

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

/// Enumerate biometric units and return the first fingerprint sensor's unit ID.
fn get_first_unit_id() -> Result<u32> {
    unsafe {
        let mut unit_array: *mut WINBIO_UNIT_SCHEMA = std::ptr::null_mut();
        let mut unit_count: usize = 0;

        WinBioEnumBiometricUnits(
            winbio_helpers::WINBIO_TYPE_FINGERPRINT,
            &mut unit_array,
            &mut unit_count,
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioEnumBiometricUnits", &e))?;

        if unit_count == 0 {
            if !unit_array.is_null() {
                winbio_helpers::winbio_free(unit_array as *const _);
            }
            anyhow::bail!("No fingerprint biometric units found");
        }

        let unit_id = (*unit_array).UnitId;
        winbio_helpers::winbio_free(unit_array as *const _);
        Ok(unit_id)
    }
}
