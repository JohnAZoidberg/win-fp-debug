# win-fp-debug

Windows Fingerprint Reader Diagnostic Tool. A standalone CLI that debugs fingerprint reader issues at multiple levels: hardware detection, driver/service status, WinBio subsystem enumeration, and interactive operations.

## Prerequisites

### Rust Toolchain

Install via [rustup](https://rustup.rs/):

```
winget install Rustlang.Rustup
```

This installs the `stable-x86_64-pc-windows-msvc` toolchain by default.

### MSVC Build Tools + Windows SDK

The Rust MSVC toolchain requires both the **MSVC compiler/linker** and the **Windows SDK** (which provides `kernel32.lib` and other import libraries).

**Option A: Via Visual Studio Installer (recommended)**

1. Install [Visual Studio Community 2022](https://visualstudio.microsoft.com/) or Build Tools
2. In the installer, select the **"Desktop development with C++"** workload
3. Ensure **"Windows 11 SDK"** (or Windows 10 SDK) is checked under "Individual components"

**Option B: Minimal install via command line**

```powershell
# Install MSVC Build Tools (if not already installed)
winget install Microsoft.VisualStudio.2022.BuildTools

# Install Windows SDK (required for kernel32.lib, user32.lib, etc.)
winget install Microsoft.WindowsSDK.10.0.18362
```

### PATH Conflict: Git's `link.exe`

If Git for Windows is installed, its `link.exe` (POSIX `link` command) may shadow the MSVC linker. If you see linker errors like `link: extra operand`, create `.cargo/config.toml` in the project root:

```toml
[target.x86_64-pc-windows-msvc]
linker = "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.44.35207\\bin\\Hostx64\\x64\\link.exe"
rustflags = [
    "-Lnative=C:\\Program Files (x86)\\Windows Kits\\10\\Lib\\10.0.18362.0\\um\\x64",
    "-Lnative=C:\\Program Files (x86)\\Windows Kits\\10\\Lib\\10.0.18362.0\\ucrt\\x64",
    "-Lnative=C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.44.35207\\lib\\x64",
]
```

Adjust the version numbers to match your installed MSVC and SDK versions. You can find them by browsing `C:\Program Files\Microsoft Visual Studio\2022\...\VC\Tools\MSVC\` and `C:\Program Files (x86)\Windows Kits\10\Lib\`.

Alternatively, build from a **Developer Command Prompt for VS 2022** which sets up all paths automatically.

## Building

```
cargo build --release
```

The output binary is at `target\release\win-fp-debug.exe` with no runtime dependencies.

## Usage

```
win-fp-debug <COMMAND>
```

### Diagnostic Commands

| Command | Description |
|---|---|
| `diagnose` | Run all 3 diagnostic levels sequentially |
| `check-hardware` | Level 1: Detect biometric PnP devices via PowerShell |
| `check-driver` | Level 2: Check WbioSrvc service status and configuration |
| `check-sensor` | Level 3: Enumerate WinBio units and test session open/close |

### Interactive Commands

| Command | Description |
|---|---|
| `identify` | Touch sensor to identify the current user (blocks until touch) |
| `list-fingerprints` | List enrolled fingerprints (requires touch to identify first) |
| `verify --finger N` | Verify a specific finger matches (1-10, requires two touches) |
| `capture` | Capture a raw fingerprint sample and display BIR metadata |
| `delete --finger N` | Delete a fingerprint template (1-10, requires touch to identify) |

### Finger Positions

| Number | Finger |
|---|---|
| 1 | Right Thumb |
| 2 | Right Index |
| 3 | Right Middle |
| 4 | Right Ring |
| 5 | Right Little |
| 6 | Left Thumb |
| 7 | Left Index |
| 8 | Left Middle |
| 9 | Left Ring |
| 10 | Left Little |

### Examples

```powershell
# Run full diagnostics
win-fp-debug diagnose

# Check if hardware is detected
win-fp-debug check-hardware

# Identify yourself by touching the sensor
win-fp-debug identify

# List all enrolled fingerprints
win-fp-debug list-fingerprints

# Verify your right index finger
win-fp-debug verify --finger 2

# Delete your left thumb enrollment
win-fp-debug delete --finger 6
```

## Notes

- **Administrator**: Several operations require running as Administrator. The tool will warn if not elevated.
- **Focus**: Console apps must acquire focus via `WinBioAcquireFocus` before `WinBioIdentify`/`WinBioVerify`. The tool handles this automatically through a `SessionGuard` RAII wrapper.
- **Interactive commands block**: `identify`, `list-fingerprints`, `verify`, and `capture` will block waiting for a finger touch. Press Ctrl+C to cancel.
