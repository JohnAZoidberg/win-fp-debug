use windows::core::HRESULT;

/// Translate common WinBio / HRESULT codes into human-readable strings.
pub fn hresult_message(hr: HRESULT) -> &'static str {
    match hr.0 as u32 {
        // Generic success / failure
        0x0000_0000 => "Success (S_OK)",
        0x8007_0005 => "Access denied (E_ACCESSDENIED)",
        0x8000_4005 => "Unspecified error (E_FAIL)",
        0x8000_4002 => "No such interface (E_NOINTERFACE)",
        0x8007_0057 => "Invalid argument (E_INVALIDARG)",
        0x8000_FFFF => "Catastrophic failure (E_UNEXPECTED)",

        // WinBio-specific (WINBIO_E_*)
        0x8009_8001 => "Sensor not calibrated (WINBIO_E_UNSUPPORTED_FACTOR)",
        0x8009_8002 => "Invalid unit (WINBIO_E_INVALID_UNIT)",
        0x8009_8003 => "Unknown ID (WINBIO_E_UNKNOWN_ID)",
        0x8009_8004 => "Operation canceled (WINBIO_E_CANCELED)",
        0x8009_8005 => "No match (WINBIO_E_NO_MATCH)",
        0x8009_8006 => "Capture sample failed (WINBIO_E_CAPTURE_ABORTED)",
        0x8009_8007 => "Enrollment in progress (WINBIO_E_ENROLLMENT_IN_PROGRESS)",
        0x8009_8008 => "Bad capture (WINBIO_E_BAD_CAPTURE)",
        0x8009_800B => "Session busy (WINBIO_E_SESSION_BUSY)",
        0x8009_800E => "No session (WINBIO_E_SESSION_HANDLE_CLOSED)",
        0x8009_8010 => "Database full (WINBIO_E_DATABASE_FULL)",
        0x8009_8011 => "Database locked (WINBIO_E_DATABASE_LOCKED)",
        0x8009_8014 => "Not enrolled (WINBIO_E_UNKNOWN_ID)",
        0x8009_8016 => "Database no such record (WINBIO_E_DATABASE_NO_SUCH_RECORD)",
        0x8009_8019 => "Sensor unavailable (WINBIO_E_DEVICE_BUSY)",
        0x8009_802E => "Data collection in progress",
        0x8009_8029 => "No preboot identity (WINBIO_E_NO_PREBOOT_IDENTITY)",

        // WinBio informational
        0x0009_8001 => "Sample needed for enrollment (WINBIO_I_MORE_DATA)",

        _ => "Unknown HRESULT",
    }
}

/// Format a windows::core::Error into an anyhow error with human-readable context.
pub fn wrap_winbio_error(context: &str, err: &windows::core::Error) -> anyhow::Error {
    let hr = err.code();
    anyhow::anyhow!(
        "{}: {} (0x{:08X})",
        context,
        hresult_message(hr),
        hr.0 as u32
    )
}

/// Extract the HRESULT code from a windows::core::Error as u32.
pub fn error_code(err: &windows::core::Error) -> u32 {
    err.code().0 as u32
}
