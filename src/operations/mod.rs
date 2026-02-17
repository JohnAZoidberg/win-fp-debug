pub mod capture;
pub mod credential_state;
pub mod delete;
pub mod delete_database;
pub mod enroll;
pub mod enum_databases;
pub mod identify;
pub mod list;
pub mod verify;

use crate::winbio_helpers;

/// RAII guard that opens a WinBio session and automatically closes it on drop.
/// When `foreground` is true, creates a hidden focus window to satisfy WinBio's
/// window focus requirement for interactive operations (Identify/Verify).
pub struct SessionGuard {
    pub session: u32,
    _focus: Option<winbio_helpers::FocusWindow>,
}

impl SessionGuard {
    /// Open a new session with the given flags. If `foreground` is true,
    /// create a hidden focus window with a message pump.
    pub fn new(flags: u32, foreground: bool) -> anyhow::Result<Self> {
        let focus = if foreground {
            match winbio_helpers::FocusWindow::new() {
                Some(fw) => Some(fw),
                None => {
                    crate::output::print_warn("Could not create focus window");
                    None
                }
            }
        } else {
            None
        };
        let session = winbio_helpers::open_session(flags)?;
        Ok(Self {
            session,
            _focus: focus,
        })
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        winbio_helpers::close_session(self.session);
        // _focus drops automatically, releasing WinBio focus and stopping the message pump
    }
}
