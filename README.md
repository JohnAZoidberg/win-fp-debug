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
| `enroll --finger N` | Enroll a new fingerprint (1-10, requires repeated touches) |
| `delete --finger N` | Delete a fingerprint template (1-10, requires touch to identify) |

### Database Commands

| Command | Description |
|---|---|
| `enum-databases` | List all biometric databases with file metadata, registry info, and sensor hardware |
| `delete-database --db N --file` | Delete the .DAT file for database N (service recreates it clean on restart) |
| `delete-database --db N --registry` | Remove the registry entry for database N (fully unregisters it) |
| `delete-database --db N --file --registry` | Both: wipe the file and unregister the database |
| `delete-database --all --file --registry` | Delete all databases (files + registry entries) |
| `credential-state` | Check if a Windows Hello password hash is linked to the biometric identity |

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

# Enroll your left thumb
win-fp-debug enroll --finger 6

# Delete your left thumb enrollment
win-fp-debug delete --finger 6

# List databases with sensor info, file metadata, and registry details
win-fp-debug enum-databases

# Reset a corrupted database (deletes file, service recreates it empty)
win-fp-debug delete-database --db 1 --file

# Remove a ghost database from old hardware
win-fp-debug delete-database --db 1 --registry

# Nuclear option: wipe file and unregister
win-fp-debug delete-database --db 1 --file --registry

# Delete everything: all databases, files and registry
win-fp-debug delete-database --all --file --registry
```

## Debugging Fingerprint Issues

### Step 1: Run diagnostics

```
win-fp-debug diagnose
```

This runs all three diagnostic levels:
- **Level 1 (Hardware)**: Checks if a biometric PnP device is detected. If nothing shows up, the sensor may be physically disconnected or the driver isn't installed.
- **Level 2 (Driver/Service)**: Checks if the `WbioSrvc` (Windows Biometric Service) is running. If stopped, fingerprint login won't work.
- **Level 3 (Sensor)**: Enumerates WinBio biometric units and tests opening a session. If no units appear but hardware is detected, the driver may not be WinBio-compatible.

### Step 2: Check databases

```
win-fp-debug enum-databases
```

Each database entry shows:
- **Database ID / Data Format**: GUIDs identifying the database and its template format
- **File Path + metadata**: Location of the `.DAT` file, its size, and created/modified timestamps
- **Registry info**: `AutoCreate`, `BiometricType`, and other WbioSrvc configuration values
- **Sensor**: Which sensor hardware the database belongs to, including manufacturer, model, device instance ID, engine/storage adapter DLLs, and whether it's currently active

Things to look for:
- **Missing .DAT file** (`Could not read file metadata: os error 2`): The database is registered but the file doesn't exist. If `AutoCreate=Yes`, restarting WbioSrvc should recreate it. If the sensor is `(not active)`, this is expected for old/replaced hardware.
- **Sensor "(not active)"**: The database belongs to a sensor that's registered in the device registry but not currently connected. Common after replacing a fingerprint module (e.g., Framework laptop expansion cards). These ghost entries are harmless but can be cleaned up with `delete-database --registry`.
- **Small file size** (< 1 KB): The database is likely empty (no enrollments).
- **Multiple databases per sensor**: Each sensor typically has two configurations — a Basic mode (Config #0) and an Advanced/VSM mode (Config #1). Each gets its own database.

### Step 3: Test the sensor

```
win-fp-debug identify
```

Touch the sensor. If it recognizes you, the sensor and enrollments are working. If you get `WINBIO_E_UNKNOWN_ID`, you're not enrolled in the active database. If it hangs or errors, the sensor may be malfunctioning.

### Step 4: Check enrollments

```
win-fp-debug list-fingerprints
```

Shows which fingers are enrolled for the current user. If empty, you need to re-enroll via Windows Settings or `win-fp-debug enroll --finger N`.

### Step 5: Check credential linkage

```
win-fp-debug credential-state
```

If this shows `CREDENTIAL_NOT_SET`, the biometric identity isn't linked to a Windows password hash. This can cause fingerprint login to fail even when enrollment and sensor work fine. Re-link by removing and re-adding fingerprints in Windows Settings > Accounts > Sign-in options.

### Fixing Common Issues

**Corrupted database (sensor works but login fails, or enrollment errors)**:
```
win-fp-debug delete-database --db N --file
```
Deletes the `.DAT` file. The service recreates it empty on restart. Re-enroll afterward.

**Ghost databases from old hardware**:
```
win-fp-debug delete-database --db N --registry
```
Removes the registry entry for databases belonging to sensors that are no longer connected.

**Complete reset of one database**:
```
win-fp-debug delete-database --db N --file --registry
```
Removes both the file and registry entry. The database is fully gone.

**Full system reset (delete all databases)**:
```
win-fp-debug delete-database --all --file --registry
```
Removes all database files and registry entries. The biometric subsystem is wiped clean.

**Sensor not detected**: Check Device Manager > Biometric devices. If the device shows an error, try disabling and re-enabling it, or reinstalling the driver.

**WbioSrvc not running**: Open Services (`services.msc`), find "Windows Biometric Service", and start it. Set startup type to "Automatic" if it keeps stopping.

## Notes

- **Administrator**: `delete-database` and some diagnostics require running as Administrator. The tool will warn if not elevated.
- **Focus**: Console apps must acquire focus via `WinBioAcquireFocus` before `WinBioIdentify`/`WinBioVerify`. The tool handles this automatically through a `SessionGuard` RAII wrapper.
- **Interactive commands block**: `identify`, `list-fingerprints`, `verify`, `capture`, and `enroll` will block waiting for a finger touch. Press Ctrl+C to cancel.
- **Database numbering**: The `--db` argument uses 1-based numbering from `enum-databases` output. Run `enum-databases` first to see which number corresponds to which database.
