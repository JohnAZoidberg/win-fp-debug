# win-fp-debug

Windows Fingerprint Reader Diagnostic Tool. A standalone CLI that debugs fingerprint reader issues at multiple levels: hardware detection, driver/service status, WinBio subsystem enumeration, and interactive operations.

See [BUILDING.md](BUILDING.md) for build instructions.

## Quick Start

```
win-fp-debug diagnose
```

This runs three diagnostic levels — hardware detection, service status, WinBio session — and reports what's working and what isn't. Based on the output:

| Diagnosis | Next step |
|---|---|
| No biometric device found | Check Device Manager > Biometric devices, try `reinstall-driver` |
| WbioSrvc not running | `start-service`, or set startup type to Automatic in `services.msc` |
| No WinBio units found | Driver isn't WinBio-compatible, try `reinstall-driver` |
| Database mismatch | Databases don't match current sensor — see [Common Fixes](#common-fixes) |
| Everything passes | Run `identify` to test the sensor interactively |

## Commands

### Diagnostic

| Command | Description |
|---|---|
| `diagnose` | Run all 3 diagnostic levels (hardware → driver → sensor) |
| `check-hardware` | Level 1: PnP biometric device detection |
| `check-driver` | Level 2: WbioSrvc service status and configuration |
| `check-sensor` | Level 3: WinBio unit enumeration + session test |

### Interactive

| Command | Description |
|---|---|
| `identify` | Touch sensor to identify the current user (blocks until touch) |
| `list-fingerprints` | List enrolled fingerprints (requires touch to identify first) |
| `verify --finger N` | Verify a specific finger matches (1-10) |
| `capture` | Capture a raw fingerprint sample and display BIR metadata |
| `enroll --finger N` | Enroll a new fingerprint (1-10, requires repeated touches) |
| `delete --finger N` | Delete a fingerprint template (1-10, requires touch to identify) |

### Database

| Command | Description |
|---|---|
| `enum-databases` | List databases with file metadata, registry info, and sensor hardware |
| `delete-database --db N --file` | Delete the .DAT file for database N (service recreates it clean) |
| `delete-database --db N --registry` | Remove the registry entry for database N |
| `delete-database --db N --file --registry` | Both: wipe the file and unregister |
| `delete-database --all --file --registry` | Delete all databases (files + registry + orphans) |
| `credential-state` | Check if a Windows Hello password hash is linked to biometric identity |

### Service

| Command | Description |
|---|---|
| `stop-service` | Stop WbioSrvc (Windows Biometric Service) |
| `start-service` | Start WbioSrvc (Windows Biometric Service) |

### Device Management

| Command | Description |
|---|---|
| `reinstall-driver` | Export, remove, and re-add the biometric driver to force full INF reinstallation |
| `remove-device --instance-id <ID>` | Remove a specific PnP device entry by instance ID |
| `remove-device --phantom` | Remove all phantom (ghost) biometric devices |

## Debugging Fingerprint Issues

### Step 1: Run diagnostics

```
win-fp-debug diagnose
```

- **Level 1 (Hardware)**: No device → sensor disconnected or driver missing.
- **Level 2 (Service)**: WbioSrvc stopped → fingerprint login won't work.
- **Level 3 (Sensor)**: No units but hardware detected → driver isn't WinBio-compatible.

### Step 2: Check databases

```
win-fp-debug enum-databases
```

Look for:
- **Missing .DAT file** (`os error 2`): registered but file doesn't exist. Restart WbioSrvc if `AutoCreate=Yes`.
- **Sensor "(not active)"**: database belongs to disconnected hardware. Clean up with `delete-database --registry` or `remove-device --phantom`.
- **Small file** (< 1 KB): database is empty, no enrollments.

### Step 3: Test the sensor

```
win-fp-debug identify
```

Touch the sensor. `WINBIO_E_UNKNOWN_ID` means you're not enrolled in the active database.

### Step 4: Check credential linkage

```
win-fp-debug credential-state
```

`CREDENTIAL_NOT_SET` means the biometric identity isn't linked to a Windows password hash. Re-link via Windows Settings > Accounts > Sign-in options.

## Common Fixes

**Reinstall the driver** (fixes corrupted driver state, missing database registry entries):
```
win-fp-debug reinstall-driver
```
This exports the driver, uninstalls it, then re-adds it — forcing the INF to re-run its AddReg sections. See [INTERNALS.md](INTERNALS.md#how-reinstall-driver-works) for details.

**Reset a corrupted database** (sensor works but login fails):
```
win-fp-debug delete-database --db N --file
```
Deletes the `.DAT` file. Service recreates it empty on restart. Re-enroll afterward.

**Clean up ghost devices and databases** (after swapping hardware):
```
win-fp-debug remove-device --phantom
win-fp-debug delete-database --db N --registry
```
Removes phantom PnP entries and their orphaned database registrations.

**Full system reset** (nuclear option):
```powershell
win-fp-debug stop-service
win-fp-debug delete-database --all --file --registry
win-fp-debug start-service
```
Wipes all databases while the service is stopped, preventing file recreation. The service creates clean databases for active sensors on restart. See [INTERNALS.md](INTERNALS.md#how-delete-database-works) for why stopping the service first matters.

## Notes

- **Administrator**: `delete-database`, `reinstall-driver`, `remove-device`, and some diagnostics require running as Administrator.
- **Focus**: Console apps must acquire focus via `WinBioAcquireFocus` before `WinBioIdentify`/`WinBioVerify`. The tool handles this automatically.
- **Interactive commands block**: `identify`, `list-fingerprints`, `verify`, `capture`, and `enroll` block waiting for a finger touch. Press Ctrl+C to cancel.
- **Database numbering**: `--db` uses 1-based numbering from `enum-databases` output.

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
