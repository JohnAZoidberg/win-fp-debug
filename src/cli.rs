use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "win-fp-debug",
    about = "Windows Fingerprint Reader Diagnostic Tool",
    long_about = "Diagnoses fingerprint reader issues on Windows at multiple levels:\n\
                  hardware detection, driver/service status, WinBio subsystem\n\
                  enumeration, and interactive operations."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run all 3 diagnostic levels sequentially (hardware → driver → sensor)
    Diagnose,

    /// Level 1: PnP biometric device detection via PowerShell
    CheckHardware,

    /// Level 2: WbioSrvc service status and configuration
    CheckDriver,

    /// Level 3: WinBio unit enumeration + session test
    CheckSensor,

    /// List enrolled fingerprints (requires finger touch to identify user)
    ListFingerprints,

    /// Touch sensor to identify the current user (blocks until touch)
    Identify,

    /// Verify a specific finger matches the enrolled template
    Verify {
        /// Finger position (1–10): 1=RThumb, 2=RIndex, … 6=LThumb, 7=LIndex, …
        #[arg(long)]
        finger: u8,
    },

    /// Capture a raw fingerprint sample and display metadata
    Capture,

    /// Delete a fingerprint template for a specific finger
    Delete {
        /// Finger position (1–10) to delete
        #[arg(long)]
        finger: u8,
    },

    /// Enroll a new fingerprint (requires repeated touches)
    Enroll {
        /// Finger position (1–10): 1=RThumb, 2=RIndex, … 6=LThumb, 7=LIndex, …
        #[arg(long)]
        finger: u8,
    },

    /// List biometric storage databases (paths, GUIDs, attributes)
    EnumDatabases,

    /// Delete a biometric database file (by number from enum-databases)
    DeleteDatabase {
        /// Database number (1-based, from enum-databases output)
        #[arg(long)]
        db: usize,
    },

    /// Check if a Windows Hello credential (password hash) is linked to the biometric identity
    CredentialState,
}
