use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

use crate::output::*;
use crate::winbio_helpers;

use super::SessionGuard;

pub fn run_capture() -> Result<()> {
    print_header("Raw Fingerprint Capture");

    crate::elevation::warn_if_not_elevated();

    // Raw capture requires WINBIO_FLAG_RAW
    let guard = SessionGuard::new(winbio_helpers::WINBIO_FLAG_RAW, false)?;
    print_step("Session opened in RAW mode. Touch the sensor now...");

    unsafe {
        let mut sample: *mut WINBIO_BIR = std::ptr::null_mut();
        let mut sample_size: usize = 0;
        let mut unit_id = 0u32;
        let mut reject_detail = 0u32;

        let result = WinBioCaptureSample(
            guard.session,
            winbio_helpers::WINBIO_PURPOSE_NO_PURPOSE_AVAILABLE,
            WINBIO_DATA_FLAG_RAW as u8,
            Some(&mut unit_id),
            &mut sample,
            Some(&mut sample_size),
            Some(&mut reject_detail),
        );

        if let Err(e) = result {
            let code = crate::error::error_code(&e);
            if code == 0x8009_8008 {
                print_fail("Bad capture");
                print_info(
                    "Reject reason",
                    winbio_helpers::reject_reason(reject_detail),
                );
            } else {
                print_fail(&format!(
                    "WinBioCaptureSample failed: {} (0x{:08X})",
                    crate::error::hresult_message(e.code()),
                    code
                ));
            }
            if !sample.is_null() {
                winbio_helpers::winbio_free(sample as *const _);
            }
            return Ok(());
        }

        print_pass("Sample captured successfully");
        print_info("Unit ID", &unit_id.to_string());
        print_info("Sample size (bytes)", &sample_size.to_string());

        if !sample.is_null() {
            let bir = &*sample;
            print_info(
                "BIR header block",
                &format!(
                    "offset={}, size={}",
                    bir.HeaderBlock.Offset, bir.HeaderBlock.Size
                ),
            );
            print_info(
                "BIR standard data block",
                &format!(
                    "offset={}, size={}",
                    bir.StandardDataBlock.Offset, bir.StandardDataBlock.Size
                ),
            );
            print_info(
                "BIR vendor data block",
                &format!(
                    "offset={}, size={}",
                    bir.VendorDataBlock.Offset, bir.VendorDataBlock.Size
                ),
            );

            winbio_helpers::winbio_free(sample as *const _);
        }
    }

    Ok(())
}
