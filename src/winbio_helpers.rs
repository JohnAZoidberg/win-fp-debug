use anyhow::Result;
use windows::Win32::Devices::BiometricFramework::*;

// Constants not exported by the windows crate v0.59
pub const WINBIO_TYPE_FINGERPRINT: u32 = 0x0000_0008;
pub const WINBIO_FLAG_DEFAULT: u32 = 0x0000_0000;
pub const WINBIO_FLAG_RAW: u32 = 0x2000_0000;
pub const WINBIO_PURPOSE_NO_PURPOSE_AVAILABLE: u8 = 0x00;
pub const WINBIO_ID_TYPE_SID: u32 = 3;

/// Open a WinBio session with the given flags.
/// Use `WINBIO_FLAG_DEFAULT` for normal operations,
/// `WINBIO_FLAG_RAW` for raw capture.
pub fn open_session(flags: u32) -> Result<u32> {
    unsafe {
        WinBioOpenSession(
            WINBIO_TYPE_FINGERPRINT,
            WINBIO_POOL_SYSTEM,
            flags,
            None,
            None,
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioOpenSession", &e))
    }
}

/// Close a WinBio session.
pub fn close_session(session: u32) {
    unsafe {
        let _ = WinBioCloseSession(session);
    }
}

/// A hidden window running on a background thread with a message pump.
/// This gives the process a real Win32 window that can receive focus,
/// which is required for WinBioIdentify/WinBioVerify to not block forever.
///
/// Windows Terminal's pseudo-console doesn't participate in the Win32 focus
/// system, so console apps need a real HWND for WinBio operations.
pub struct FocusWindow {
    hwnd_raw: isize,
    thread: Option<std::thread::JoinHandle<()>>,
    has_winbio_focus: bool,
}

unsafe extern "system" fn focus_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
}

impl FocusWindow {
    /// Create a hidden window on a background thread with a message pump,
    /// then bring it to the foreground and attempt WinBioAcquireFocus.
    pub fn new() -> Option<Self> {
        use std::sync::mpsc;
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::*;

        let (tx, rx) = mpsc::channel::<isize>();

        let thread = std::thread::spawn(move || unsafe {
            let class_name = w!("WinFpDebugFocus");
            let wc: WNDCLASSW = WNDCLASSW {
                lpfnWndProc: Some(focus_wnd_proc),
                lpszClassName: class_name,
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("win-fp-debug"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            );

            match hwnd {
                Ok(h) if !h.is_invalid() => {
                    // Show then immediately hide — this triggers WM_ACTIVATE
                    let _ = ShowWindow(h, SW_SHOW);
                    let _ = ShowWindow(h, SW_HIDE);
                    let _ = SetForegroundWindow(h);
                    let _ = tx.send(h.0 as isize);
                }
                _ => {
                    let _ = tx.send(0);
                    return;
                }
            }

            // Message pump — runs until WM_QUIT is posted
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });

        let hwnd_raw = rx.recv().ok()?;
        if hwnd_raw == 0 {
            return None;
        }

        // Also try WinBioAcquireFocus for good measure
        let has_winbio_focus = unsafe { WinBioAcquireFocus().is_ok() };

        Some(Self {
            hwnd_raw,
            thread: Some(thread),
            has_winbio_focus,
        })
    }
}

impl Drop for FocusWindow {
    fn drop(&mut self) {
        use windows::Win32::Foundation::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        if self.has_winbio_focus {
            unsafe {
                let _ = WinBioReleaseFocus();
            }
        }

        // Post WM_QUIT to stop the message pump
        let hwnd = HWND(self.hwnd_raw as *mut _);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Convert a WINBIO_BIOMETRIC_SUBTYPE (finger position) to a human-readable name.
/// Standard ANSI 381 positions are 1–10. MOC (Match-on-Chip) sensors like Goodix
/// may use vendor-specific subfactor values (e.g., 0xF5).
pub fn subfactor_name(subfactor: u8) -> String {
    match subfactor {
        1 => "Right Thumb".to_string(),
        2 => "Right Index".to_string(),
        3 => "Right Middle".to_string(),
        4 => "Right Ring".to_string(),
        5 => "Right Little".to_string(),
        6 => "Left Thumb".to_string(),
        7 => "Left Index".to_string(),
        8 => "Left Middle".to_string(),
        9 => "Left Ring".to_string(),
        10 => "Left Little".to_string(),
        0xFF => "Any Finger".to_string(),
        0 => "Unknown".to_string(),
        n => format!("Vendor-specific (0x{:02X})", n),
    }
}

/// Convert a null-terminated `[u16; N]` (UTF-16) buffer to a Rust String.
pub fn wchar_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Translate a WINBIO_REJECT_DETAIL to a human-readable reason.
pub fn reject_reason(detail: u32) -> &'static str {
    match detail {
        1 => "Too high",
        2 => "Too low",
        3 => "Too left",
        4 => "Too right",
        5 => "Too fast",
        6 => "Too slow",
        7 => "Poor quality",
        8 => "Too skewed",
        9 => "Too short",
        10 => "Merge failure",
        _ => "Unknown rejection reason",
    }
}

/// Free memory allocated by WinBio API calls.
/// # Safety
/// The pointer must have been returned by a WinBio enumeration or capture function.
pub unsafe fn winbio_free(ptr: *const std::ffi::c_void) {
    if !ptr.is_null() {
        let _ = WinBioFree(ptr);
    }
}

/// Convert a `WINBIO_BIOMETRIC_SENSOR_SUBTYPE` to a readable string.
pub fn sensor_subtype_name(subtype: u32) -> &'static str {
    match subtype {
        0x0000_0000 => "Unknown",
        0x0000_0001 => "Swipe",
        0x0000_0002 => "Touch",
        _ => "Other",
    }
}

/// Convert a `WINBIO_CAPABILITIES` bitmask to readable strings.
pub fn capabilities_string(caps: u32) -> String {
    let mut parts = Vec::new();
    if caps & 0x01 != 0 {
        parts.push("Sensor");
    }
    if caps & 0x02 != 0 {
        parts.push("Matching");
    }
    if caps & 0x04 != 0 {
        parts.push("Database");
    }
    if caps & 0x08 != 0 {
        parts.push("Processing");
    }
    if caps & 0x10 != 0 {
        parts.push("Encryption");
    }
    if caps & 0x20 != 0 {
        parts.push("Navigation");
    }
    if caps & 0x40 != 0 {
        parts.push("Indicator");
    }
    if caps & 0x80 != 0 {
        parts.push("VirtualSensor");
    }
    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(" | ")
    }
}
