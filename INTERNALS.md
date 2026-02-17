# Internals

How win-fp-debug works under the hood.

## Windows Biometric Framework architecture

Windows fingerprint authentication is managed by the **Windows Biometric Framework (WBF)**:

- **WbioSrvc** — the Windows Biometric Service (`services.msc` → "Windows Biometric Service"). It owns all biometric sessions, manages sensor access, and mediates between applications and biometric hardware.
- **Biometric units** — WbioSrvc's abstraction over a physical sensor + its software stack. Each unit has an engine adapter (matching algorithm), a storage adapter (template database), and a sensor adapter (hardware interface).
- **Databases** — template storage, typically `.DAT` files under `C:\WINDOWS\SYSTEM32\WINBIODATABASE\`. Each sensor configuration gets its own database, registered under `HKLM\SYSTEM\CurrentControlSet\Services\WbioSrvc\Databases\{GUID}`.
- **Driver INF** — when a biometric driver installs, its INF file contains `DatabaseAddReg` entries that create the registry keys telling WbioSrvc which databases to create. This is why a simple driver update doesn't always fix database issues — the INF's AddReg sections only run during a full driver install, not during a rescan.

Each sensor typically has two configurations:
- **Config #0 (Basic)** — standard WinBio mode
- **Config #1 (Advanced/VSM)** — Virtual Secure Mode, used by Windows Hello for enhanced security

Each configuration gets its own database GUID and `.DAT` file.

## How `diagnose` works

The `diagnose` command runs three levels of checks, each building on the previous:

1. **Level 1 — Hardware (PnP)**: Runs `Get-PnpDevice -Class Biometric` via PowerShell to check if a biometric device is present in the PnP device tree. Also checks for database mismatches — whether the databases registered in the WbioSrvc registry actually correspond to the currently connected sensor hardware. If no device shows up, the sensor is physically disconnected or has no driver.

2. **Level 2 — Driver/Service**: Opens the Service Control Manager and queries the `WbioSrvc` service status. Checks that the service is running and configured for automatic startup. If the service is stopped, fingerprint authentication won't work system-wide.

3. **Level 3 — WinBio session**: Calls `WinBioEnumBiometricUnits` to list all biometric units the framework knows about, then attempts to open a WinBio session with `WinBioOpenSession`. If hardware is detected but no biometric units appear, the driver isn't WinBio-compatible or the service failed to initialize the sensor.

The levels are ordered by dependency: there's no point checking the WinBio session if the service isn't running, and no point checking the service if no hardware is present.

## How `reinstall-driver` works

A common fix for fingerprint issues is "reinstall the driver," but simply rescanning for hardware changes (`pnputil /scan-devices`) doesn't re-run the driver INF's `AddReg` sections. The INF only executes fully during initial driver installation. This means database registry entries that were corrupted or deleted won't be recreated by a rescan.

`reinstall-driver` performs a full export→delete→add cycle:

1. **Find the device** — scans for biometric PnP devices and identifies the first one found.
2. **Identify the driver package** — queries `DEVPKEY_Device_DriverInfPath` to get the OEM INF name (e.g., `oem50.inf`).
3. **Export/backup** — runs `pnputil /export-driver` to copy the driver package to a temp directory. This is the safety net — the driver files are preserved even after deletion from the store.
4. **Delete with /uninstall** — runs `pnputil /delete-driver <inf> /uninstall /force`. The `/uninstall` flag is important: it removes the driver from the device but keeps the device node alive in the PnP tree. This avoids USB re-enumeration issues where the device would disappear entirely and might not come back without a physical replug.
5. **Re-add with /install** — runs `pnputil /add-driver <inf> /install` using the exported copy. Because the device node still exists but is now driverless, PnP matches it against the newly-added INF and performs a full driver installation — including all `AddReg` sections that create WinBio database entries.
6. **Verify** — waits briefly, then rescans for biometric devices to confirm the sensor came back.

The temp directory is cleaned up after the operation.

## How `delete-database` works

WinBio databases have two components:

- **Registry entry** — under `HKLM\SYSTEM\CurrentControlSet\Services\WbioSrvc\Databases\{GUID}`. Contains `AutoCreate`, `BiometricType`, `ConnectionString` (the `.DAT` file path), and other configuration. This is what tells WbioSrvc the database exists.
- **.DAT file** — the actual template storage at the path specified in `ConnectionString`, typically under `C:\WINDOWS\SYSTEM32\WINBIODATABASE\`.

The `--file` and `--registry` flags control which component to delete:

- `--file` only: deletes the `.DAT` file. If `AutoCreate=1` is set in the registry entry, WbioSrvc recreates an empty `.DAT` on next startup. This effectively resets the database (wipes all enrollments) without unregistering it.
- `--registry` only: removes the registry key. The `.DAT` file becomes orphaned — WbioSrvc no longer knows about it. Useful for cleaning up ghost entries from old hardware.
- Both: fully removes the database from the system.
- `--all`: applies the operation to every registered database, plus cleans up orphaned `.DAT` files in `WINBIODATABASE\` that aren't in any registry entry.

**Service restart behavior**: `delete-database` stops WbioSrvc before operating (the service locks the `.DAT` files), then restarts it when done. However, restarting the service can cause it to recreate `.DAT` files for active sensors — even if you just deleted registry entries for those sensors in a previous step. To avoid this race, use `stop-service` first, perform all cleanup, then `start-service` when ready.

## How `remove-device` works

`remove-device` removes PnP device entries using the CfgMgr32 API, bypassing Device Manager's GUI.

Two modes:

- **`--instance-id <ID>`**: removes a specific device by its PnP instance ID (the same ID shown by `check-hardware` and `enum-databases`).
- **`--phantom`**: automatically finds and removes all phantom (ghost) biometric devices. Phantom devices have problem code `CM_PROB_PHANTOM` (45) — they're registered in the PnP tree but the hardware isn't currently connected.

The removal process uses two CfgMgr32 calls:

1. **`CM_Locate_DevNodeW`** with `CM_LOCATE_DEVNODE_PHANTOM` — locates the device node, including phantom devices that aren't physically present.
2. **`CM_Uninstall_DevNode`** — uninstalls the device node from the PnP tree.

Phantom devices accumulate when hardware changes (e.g., swapping fingerprint modules on Framework laptops). They're harmless but leave behind orphaned database entries and can cause confusion in diagnostics. Removing them cleans up the device tree so only currently-connected hardware appears.
