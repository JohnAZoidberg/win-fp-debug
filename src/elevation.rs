use anyhow::Result;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Returns true if the current process is running elevated (as Administrator).
pub fn is_elevated() -> Result<bool> {
    unsafe {
        let mut token_handle = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle)?;

        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        let size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;

        let result = GetTokenInformation(
            token_handle,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size,
            &mut return_length,
        );

        let _ = CloseHandle(token_handle);
        result?;

        Ok(elevation.TokenIsElevated != 0)
    }
}

/// Print a warning if not running as admin.
pub fn warn_if_not_elevated() {
    match is_elevated() {
        Ok(true) => {
            crate::output::print_pass("Running as Administrator");
        }
        Ok(false) => {
            crate::output::print_warn(
                "Not running as Administrator — some operations may fail or hang",
            );
        }
        Err(e) => {
            crate::output::print_warn(&format!("Could not check elevation: {}", e));
        }
    }
}
