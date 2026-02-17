mod cli;
mod diagnostics;
mod elevation;
mod error;
mod operations;
mod output;
mod winbio_helpers;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Diagnose => {
            output::print_header("Windows Fingerprint Reader Diagnostics");
            elevation::warn_if_not_elevated();
            diagnostics::hardware::check_hardware()?;
            diagnostics::service::check_service()?;
            diagnostics::winbio::check_sensor()?;
            println!();
            output::print_step("Diagnostics complete.");
        }
        Command::CheckHardware => {
            diagnostics::hardware::check_hardware()?;
        }
        Command::CheckDriver => {
            diagnostics::service::check_service()?;
        }
        Command::CheckSensor => {
            diagnostics::winbio::check_sensor()?;
        }
        Command::ListFingerprints => {
            operations::list::run_list()?;
        }
        Command::Identify => {
            operations::identify::run_identify()?;
        }
        Command::Verify { finger } => {
            operations::verify::run_verify(finger)?;
        }
        Command::Capture => {
            operations::capture::run_capture()?;
        }
        Command::Delete { finger } => {
            operations::delete::run_delete(finger)?;
        }
        Command::Enroll { finger } => {
            operations::enroll::run_enroll(finger)?;
        }
    }

    Ok(())
}
