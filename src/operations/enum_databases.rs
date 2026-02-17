use anyhow::Result;
use std::collections::HashMap;
use windows::Win32::Devices::BiometricFramework::*;
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, PWSTR};

use crate::output::*;
use crate::winbio_helpers;

fn format_guid(guid: &windows::core::GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

fn attributes_string(attrs: u32) -> String {
    let mut parts = Vec::new();
    if attrs & 0x01 != 0 {
        parts.push("OWNED");
    }
    if attrs & 0x02 != 0 {
        parts.push("REMOTE");
    }
    if parts.is_empty() {
        format!("0x{:08X}", attrs)
    } else {
        format!("{} (0x{:08X})", parts.join(" | "), attrs)
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB ({} bytes)", bytes as f64 / 1024.0, bytes)
    } else {
        format!(
            "{:.1} MB ({} bytes)",
            bytes as f64 / (1024.0 * 1024.0),
            bytes
        )
    }
}

fn format_system_time(time: std::time::SystemTime) -> String {
    let since_unix = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return format!("{:?}", time),
    };

    let secs = since_unix.as_secs() as i64;

    let mut days = secs / 86400;
    let day_secs = secs % 86400;
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    let mut year = 1970i32;
    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31,
        if is_leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];

    let mut month = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i;
            break;
        }
        days -= md;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year,
        month + 1,
        days + 1,
        hour,
        minute,
        second
    )
}

fn print_file_metadata(file_path: &str) {
    match std::fs::metadata(file_path) {
        Ok(meta) => {
            print_info("  File Size", &format_file_size(meta.len()));
            if let Ok(created) = meta.created() {
                print_info("  Created", &format_system_time(created));
            }
            if let Ok(modified) = meta.modified() {
                print_info("  Modified", &format_system_time(modified));
            }
        }
        Err(e) => {
            print_warn(&format!("  Could not read file metadata: {}", e));
        }
    }
}

fn read_registry_string(key: HKEY, value_name: &str) -> Option<String> {
    unsafe {
        let value_name_wide: Vec<u16> =
            value_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut data_type = REG_VALUE_TYPE::default();
        let mut data_size: u32 = 0;

        let status = RegQueryValueExW(
            key,
            PCWSTR(value_name_wide.as_ptr()),
            None,
            Some(&mut data_type),
            None,
            Some(&mut data_size),
        );

        if status.is_err() || data_size == 0 {
            return None;
        }

        let mut buf = vec![0u8; data_size as usize];
        let status = RegQueryValueExW(
            key,
            PCWSTR(value_name_wide.as_ptr()),
            None,
            Some(&mut data_type),
            Some(buf.as_mut_ptr()),
            Some(&mut data_size),
        );

        if status.is_err() {
            return None;
        }

        match data_type {
            REG_SZ | REG_EXPAND_SZ => {
                let wide: Vec<u16> = buf
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
                Some(String::from_utf16_lossy(&wide[..end]))
            }
            REG_DWORD => {
                if buf.len() >= 4 {
                    let val = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    Some(val.to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn print_registry_info(database_id: &str) {
    unsafe {
        let subkey = format!(
            "SYSTEM\\CurrentControlSet\\Services\\WbioSrvc\\Databases\\{}",
            database_id
        );
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();

        let mut hkey = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        );

        if status.is_err() {
            print_info("  Registry", "(no registry entry found)");
            return;
        }

        let value_names = [
            ("BiometricType", "  Biometric Type (reg)"),
            ("SensorPool", "  Sensor Pool (reg)"),
            ("AutoCreate", "  Auto Create (reg)"),
            ("AutoName", "  Auto Name (reg)"),
        ];

        for (reg_name, label) in &value_names {
            if let Some(val) = read_registry_string(hkey, reg_name) {
                let display = match (*reg_name, val.as_str()) {
                    ("BiometricType", "8") => "Fingerprint (0x08)".to_string(),
                    ("SensorPool", "1") => "System (1)".to_string(),
                    ("SensorPool", "2") => "Private (2)".to_string(),
                    ("AutoCreate", "1") => "Yes".to_string(),
                    ("AutoCreate", "0") => "No".to_string(),
                    ("AutoName", "1") => "Yes".to_string(),
                    ("AutoName", "0") => "No".to_string(),
                    _ => val,
                };
                print_info(label, &display);
            }
        }

        if let Some(val) = read_registry_string(hkey, "FilePath") {
            if !val.is_empty() {
                print_info("  File Path (reg)", &val);
            }
        }
        if let Some(val) = read_registry_string(hkey, "ConnectionString") {
            if !val.is_empty() {
                print_info("  Connection String (reg)", &val);
            }
        }

        let _ = RegCloseKey(hkey);
    }
}

/// Info about a sensor configuration that references a specific database.
struct SensorDatabaseLink {
    /// None if sensor is not currently active (disconnected / not enumerated).
    unit_id: Option<u32>,
    description: String,
    manufacturer: String,
    model: String,
    device_instance_id: String,
    sensor_subtype: Option<u32>,
    config_index: u32,
    engine_adapter: String,
    storage_adapter: String,
    sensor_mode: String,
    virtual_secure_mode: bool,
}

/// Read WinBio configuration values for a given device instance and config index.
/// Returns (DatabaseId key, SensorDatabaseLink) if a DatabaseId is found.
fn read_device_winbio_config(
    device_instance_id: &str,
    config_idx: u32,
    unit_id: Option<u32>,
    description: &str,
    manufacturer: &str,
    model: &str,
    sensor_subtype: Option<u32>,
) -> Option<(String, SensorDatabaseLink)> {
    unsafe {
        let subkey = format!(
            "SYSTEM\\CurrentControlSet\\Enum\\{}\\Device Parameters\\WinBio\\Configurations\\{}",
            device_instance_id, config_idx
        );
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();

        let mut hkey = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        );

        if status.is_err() {
            return None;
        }

        let result = read_registry_string(hkey, "DatabaseId").map(|db_id| {
            let engine =
                read_registry_string(hkey, "EngineAdapterBinary").unwrap_or_default();
            let storage =
                read_registry_string(hkey, "StorageAdapterBinary").unwrap_or_default();
            let sensor_mode_val =
                read_registry_string(hkey, "SensorMode").unwrap_or_default();
            let vsm = read_registry_string(hkey, "VirtualSecureMode")
                .map(|v| v == "1")
                .unwrap_or(false);

            let sensor_mode_display = match sensor_mode_val.as_str() {
                "1" => "Basic".to_string(),
                "2" => "Advanced".to_string(),
                other => format!("Unknown ({})", other),
            };

            let db_id_upper = db_id.to_uppercase();
            let db_id_key = if db_id_upper.starts_with('{') {
                db_id_upper
            } else {
                format!("{{{}}}", db_id_upper)
            };

            let link = SensorDatabaseLink {
                unit_id,
                description: description.to_string(),
                manufacturer: manufacturer.to_string(),
                model: model.to_string(),
                device_instance_id: device_instance_id.to_string(),
                sensor_subtype,
                config_index: config_idx,
                engine_adapter: engine,
                storage_adapter: storage,
                sensor_mode: sensor_mode_display,
                virtual_secure_mode: vsm,
            };

            (db_id_key, link)
        });

        let _ = RegCloseKey(hkey);
        result
    }
}

/// Enumerate registry subkeys under a given parent key.
fn enum_registry_subkeys(parent: HKEY, subpath: &str) -> Vec<String> {
    let mut result = Vec::new();
    unsafe {
        let subpath_wide: Vec<u16> =
            subpath.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = HKEY::default();
        let status = RegOpenKeyExW(
            parent,
            PCWSTR(subpath_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        );
        if status.is_err() {
            return result;
        }

        let mut index = 0u32;
        let mut name_buf = [0u16; 512];
        loop {
            let mut name_len = name_buf.len() as u32;
            let status = RegEnumKeyExW(
                hkey,
                index,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                Some(PWSTR::null()),
                None,
                None,
            );
            if status.is_err() {
                break;
            }
            let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            result.push(name);
            index += 1;
        }

        let _ = RegCloseKey(hkey);
    }
    result
}

/// Read the "FriendlyName" from a device's registry key for display when the device
/// is not currently active as a biometric unit.
fn read_device_friendly_name(device_instance_id: &str) -> String {
    unsafe {
        let subkey = format!(
            "SYSTEM\\CurrentControlSet\\Enum\\{}",
            device_instance_id
        );
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        );
        if status.is_err() {
            return String::new();
        }
        let name = read_registry_string(hkey, "FriendlyName")
            .or_else(|| read_registry_string(hkey, "DeviceDesc"))
            .unwrap_or_default();
        let _ = RegCloseKey(hkey);
        // Strip the driver store prefix (e.g., "@oem26.inf,%devdesc%;Actual Name")
        if let Some(pos) = name.rfind(';') {
            name[pos + 1..].to_string()
        } else {
            name
        }
    }
}

/// Build a map from DatabaseId -> Vec<SensorDatabaseLink>.
/// Pass 1: active sensors from WinBioEnumBiometricUnits.
/// Pass 2: registry scan for all USB devices with WinBio configurations (catches disconnected sensors).
fn build_sensor_database_map() -> HashMap<String, Vec<SensorDatabaseLink>> {
    let mut map: HashMap<String, Vec<SensorDatabaseLink>> = HashMap::new();
    // Track device instance IDs we've already processed from active sensors
    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pass 1: active biometric units
    unsafe {
        let mut unit_array: *mut WINBIO_UNIT_SCHEMA = std::ptr::null_mut();
        let mut unit_count: usize = 0;

        let result = WinBioEnumBiometricUnits(
            winbio_helpers::WINBIO_TYPE_FINGERPRINT,
            &mut unit_array,
            &mut unit_count,
        );

        if result.is_ok() && unit_count > 0 {
            let units = std::slice::from_raw_parts(unit_array, unit_count);
            for unit in units {
                let device_instance_id =
                    winbio_helpers::wchar_to_string(&unit.DeviceInstanceId);
                let description = winbio_helpers::wchar_to_string(&unit.Description);
                let manufacturer = winbio_helpers::wchar_to_string(&unit.Manufacturer);
                let model = winbio_helpers::wchar_to_string(&unit.Model);

                seen_devices.insert(device_instance_id.to_uppercase());

                for config_idx in 0..3u32 {
                    if let Some((key, link)) = read_device_winbio_config(
                        &device_instance_id,
                        config_idx,
                        Some(unit.UnitId),
                        &description,
                        &manufacturer,
                        &model,
                        Some(unit.SensorSubType),
                    ) {
                        map.entry(key).or_default().push(link);
                    }
                }
            }
        }

        if !unit_array.is_null() {
            winbio_helpers::winbio_free(unit_array as *const _);
        }
    }

    // Pass 2: scan registry for all USB devices with WinBio configurations
    // This catches sensors that are registered but not currently active
    let usb_vid_pids = enum_registry_subkeys(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Enum\\USB");
    for vid_pid in &usb_vid_pids {
        let serials = enum_registry_subkeys(
            HKEY_LOCAL_MACHINE,
            &format!("SYSTEM\\CurrentControlSet\\Enum\\USB\\{}", vid_pid),
        );
        for serial in &serials {
            let device_instance_id = format!("USB\\{}\\{}", vid_pid, serial);
            if seen_devices.contains(&device_instance_id.to_uppercase()) {
                continue;
            }

            // Check if this device has WinBio configurations
            let has_winbio = unsafe {
                let subkey = format!(
                    "SYSTEM\\CurrentControlSet\\Enum\\{}\\Device Parameters\\WinBio\\Configurations",
                    device_instance_id
                );
                let subkey_wide: Vec<u16> =
                    subkey.encode_utf16().chain(std::iter::once(0)).collect();
                let mut hkey = HKEY::default();
                let status = RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    PCWSTR(subkey_wide.as_ptr()),
                    None,
                    KEY_READ,
                    &mut hkey,
                );
                if status.is_ok() {
                    let _ = RegCloseKey(hkey);
                    true
                } else {
                    false
                }
            };

            if !has_winbio {
                continue;
            }

            let friendly_name = read_device_friendly_name(&device_instance_id);

            for config_idx in 0..3u32 {
                if let Some((key, link)) = read_device_winbio_config(
                    &device_instance_id,
                    config_idx,
                    None, // not an active unit
                    if friendly_name.is_empty() {
                        &device_instance_id
                    } else {
                        &friendly_name
                    },
                    "",
                    "",
                    None,
                ) {
                    map.entry(key).or_default().push(link);
                }
            }
        }
    }

    map
}

fn print_sensor_info(links: &[SensorDatabaseLink]) {
    for link in links {
        let vsm_tag = if link.virtual_secure_mode {
            " [VSM]"
        } else {
            ""
        };
        let active_tag = if link.unit_id.is_some() {
            ""
        } else {
            " (not active)"
        };
        let unit_str = match link.unit_id {
            Some(id) => format!("Unit {}", id),
            None => "no unit".to_string(),
        };
        print_info(
            "  Sensor",
            &format!(
                "{} ({}, {} mode{}{})",
                link.description, unit_str, link.sensor_mode, vsm_tag, active_tag
            ),
        );
        if !link.manufacturer.is_empty() {
            print_info("    Manufacturer", &link.manufacturer);
        }
        if !link.model.is_empty() {
            print_info("    Model", &link.model);
        }
        if let Some(subtype) = link.sensor_subtype {
            print_info(
                "    Sensor Type",
                winbio_helpers::sensor_subtype_name(subtype),
            );
        }
        print_info("    Device Instance", &link.device_instance_id);
        print_info(
            "    Config",
            &format!(
                "#{} — Engine: {}, Storage: {}",
                link.config_index, link.engine_adapter, link.storage_adapter
            ),
        );
    }
}

pub fn run_enum_databases() -> Result<()> {
    print_header("Biometric Storage Databases");

    // Build sensor-to-database map from registry
    let sensor_map = build_sensor_database_map();

    unsafe {
        let mut schema_array: *mut WINBIO_STORAGE_SCHEMA = std::ptr::null_mut();
        let mut schema_count: usize = 0;

        WinBioEnumDatabases(
            winbio_helpers::WINBIO_TYPE_FINGERPRINT,
            &mut schema_array,
            &mut schema_count,
        )
        .map_err(|e| crate::error::wrap_winbio_error("WinBioEnumDatabases", &e))?;

        if schema_count == 0 {
            print_warn("No biometric databases found");
        } else {
            print_pass(&format!("{} database(s) found", schema_count));
            let schemas = std::slice::from_raw_parts(schema_array, schema_count);
            for (i, schema) in schemas.iter().enumerate() {
                println!();
                print_step(&format!("Database {}", i + 1));
                let db_id = format_guid(&schema.DatabaseId);
                print_info("Database ID", &db_id);
                print_info("Data Format", &format_guid(&schema.DataFormat));
                print_info("Attributes", &attributes_string(schema.Attributes));
                let file_path = winbio_helpers::wchar_to_string(&schema.FilePath);
                let conn_string = winbio_helpers::wchar_to_string(&schema.ConnectionString);
                print_info(
                    "File Path",
                    if file_path.is_empty() {
                        "(empty)"
                    } else {
                        &file_path
                    },
                );
                print_info(
                    "Connection String",
                    if conn_string.is_empty() {
                        "(empty)"
                    } else {
                        &conn_string
                    },
                );

                // File metadata
                if !file_path.is_empty() {
                    print_file_metadata(&file_path);
                }

                // Registry cross-reference
                print_registry_info(&db_id);

                // Sensor cross-reference
                if let Some(links) = sensor_map.get(&db_id) {
                    print_sensor_info(links);
                } else {
                    print_info("  Sensor", "(no matching sensor found)");
                }
            }
        }

        if !schema_array.is_null() {
            winbio_helpers::winbio_free(schema_array as *const _);
        }
    }

    Ok(())
}
