#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::{bail, Result};
use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::Manager;
use serde::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap};
use std::time::Duration;
#[cfg(target_os = "macos")]
use tokio::process::Command;

#[cfg(target_os = "macos")]
const PLATFORM_DEVICE_LIST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedDevice {
    pub name: Option<String>,
    pub address: String,
    pub rssi: Option<i16>,
    pub connected: bool,
}

pub async fn has_adapter() -> Result<bool> {
    Ok(!Manager::new().await?.adapters().await?.is_empty())
}

pub async fn scan_devices(seconds: u64) -> Result<Vec<ScannedDevice>> {
    let mut devices = BTreeMap::new();
    for device in platform_known_devices().await.unwrap_or_default() {
        upsert_device(&mut devices, device);
    }

    let manager = match Manager::new().await {
        Ok(manager) => manager,
        Err(_) if !devices.is_empty() => return Ok(devices.into_values().collect()),
        Err(err) => return Err(err.into()),
    };
    let adapters = match manager.adapters().await {
        Ok(adapters) => adapters,
        Err(_) if !devices.is_empty() => return Ok(devices.into_values().collect()),
        Err(err) => return Err(err.into()),
    };
    if adapters.is_empty() {
        return Ok(devices.into_values().collect());
    }

    let mut scanning_adapters = Vec::new();
    let mut start_errors = Vec::new();
    for adapter in adapters {
        match adapter.start_scan(ScanFilter::default()).await {
            Ok(()) => scanning_adapters.push(adapter),
            Err(err) => start_errors.push(err.to_string()),
        }
    }

    if scanning_adapters.is_empty() {
        if !devices.is_empty() {
            return Ok(devices.into_values().collect());
        }
        bail!(
            "failed to start Bluetooth scans: {}",
            start_errors.join("; ")
        );
    }

    tokio::time::sleep(Duration::from_secs(seconds)).await;

    for adapter in &scanning_adapters {
        let Ok(peripherals) = adapter.peripherals().await else {
            continue;
        };

        for peripheral in peripherals {
            let address = peripheral.address().to_string();
            let props = peripheral.properties().await.ok().flatten();
            let connected = peripheral.is_connected().await.unwrap_or(false);
            upsert_device(
                &mut devices,
                ScannedDevice {
                    name: props.as_ref().and_then(|props| props.local_name.clone()),
                    address,
                    rssi: props.as_ref().and_then(|props| props.rssi),
                    connected,
                },
            );
        }
    }

    for adapter in &scanning_adapters {
        let _ = adapter.stop_scan().await;
    }

    Ok(devices.into_values().collect())
}

fn upsert_device(devices: &mut BTreeMap<String, ScannedDevice>, device: ScannedDevice) {
    match devices.entry(device_key(&device.address)) {
        Entry::Vacant(slot) => {
            slot.insert(device);
        }
        Entry::Occupied(mut slot) => {
            let ScannedDevice {
                name,
                rssi,
                connected,
                ..
            } = device;
            let existing = slot.get_mut();

            if name.is_some() && (existing.name.is_none() || connected) {
                existing.name = name;
            }
            if rssi.is_some() {
                existing.rssi = rssi;
            }
            existing.connected |= connected;
        }
    }
}

fn device_key(address: &str) -> String {
    address
        .chars()
        .filter(|ch| !matches!(ch, ':' | '-' | ' '))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

#[cfg(target_os = "macos")]
async fn platform_known_devices() -> Result<Vec<ScannedDevice>> {
    macos_known_devices().await
}

#[cfg(not(target_os = "macos"))]
async fn platform_known_devices() -> Result<Vec<ScannedDevice>> {
    Ok(Vec::new())
}

#[cfg(target_os = "macos")]
async fn macos_known_devices() -> Result<Vec<ScannedDevice>> {
    let output = tokio::time::timeout(
        PLATFORM_DEVICE_LIST_TIMEOUT,
        Command::new("system_profiler")
            .args(["SPBluetoothDataType", "-json"])
            .output(),
    )
    .await
    .context("system_profiler SPBluetoothDataType timed out")?
    .context("run system_profiler SPBluetoothDataType")?;

    if !output.status.success() {
        bail!("system_profiler SPBluetoothDataType failed");
    }

    let value =
        serde_json::from_slice(&output.stdout).context("parse system_profiler Bluetooth JSON")?;
    Ok(parse_macos_known_devices(&value))
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_known_devices(value: &serde_json::Value) -> Vec<ScannedDevice> {
    let mut devices = Vec::new();
    let Some(controllers) = value
        .get("SPBluetoothDataType")
        .and_then(|value| value.as_array())
    else {
        return devices;
    };

    for controller in controllers {
        for (field, connected) in [("device_connected", true), ("device_not_connected", false)] {
            let Some(system_devices) = controller.get(field).and_then(|value| value.as_array())
            else {
                continue;
            };

            for entry in system_devices {
                let Some(named_devices) = entry.as_object() else {
                    continue;
                };

                for (name, props) in named_devices {
                    let Some(address) = props
                        .get("device_address")
                        .and_then(|value| value.as_str())
                        .filter(|address| !address.is_empty())
                    else {
                        continue;
                    };

                    devices.push(ScannedDevice {
                        name: Some(name.clone()),
                        address: address.to_string(),
                        rssi: parse_macos_device_rssi(props),
                        connected,
                    });
                }
            }
        }
    }

    devices
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_device_rssi(props: &serde_json::Value) -> Option<i16> {
    let value = props.get("device_rssi")?;
    if let Some(number) = value.as_i64() {
        return i16::try_from(number).ok();
    }
    value.as_str()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_macos_known_devices() {
        let value = serde_json::json!({
            "SPBluetoothDataType": [
                {
                    "device_connected": [
                        {
                            "Bose QC Ultra 2 HP": {
                                "device_address": "68:F2:1F:0D:FE:42",
                                "device_rssi": "-42"
                            }
                        }
                    ],
                    "device_not_connected": [
                        {
                            "WH-1000XM3": {
                                "device_address": "38:18:4C:EA:0E:20"
                            }
                        }
                    ]
                }
            ]
        });

        let devices = parse_macos_known_devices(&value);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name.as_deref(), Some("Bose QC Ultra 2 HP"));
        assert_eq!(devices[0].address, "68:F2:1F:0D:FE:42");
        assert_eq!(devices[0].rssi, Some(-42));
        assert!(devices[0].connected);
        assert_eq!(devices[1].name.as_deref(), Some("WH-1000XM3"));
        assert_eq!(devices[1].address, "38:18:4C:EA:0E:20");
        assert!(!devices[1].connected);
    }

    #[test]
    fn upsert_device_keeps_connected_metadata() {
        let mut devices = BTreeMap::new();
        upsert_device(
            &mut devices,
            ScannedDevice {
                name: None,
                address: "68:F2:1F:0D:FE:42".into(),
                rssi: None,
                connected: false,
            },
        );
        upsert_device(
            &mut devices,
            ScannedDevice {
                name: Some("Bose QC Ultra 2 HP".into()),
                address: "68:F2:1F:0D:FE:42".into(),
                rssi: Some(-42),
                connected: true,
            },
        );

        let device = devices.values().next().unwrap();
        assert_eq!(device.name.as_deref(), Some("Bose QC Ultra 2 HP"));
        assert_eq!(device.rssi, Some(-42));
        assert!(device.connected);
    }

    #[test]
    fn upsert_device_matches_normalized_addresses() {
        let mut devices = BTreeMap::new();
        upsert_device(
            &mut devices,
            ScannedDevice {
                name: Some("lower".into()),
                address: "68:f2:1f:0d:fe:42".into(),
                rssi: None,
                connected: false,
            },
        );
        upsert_device(
            &mut devices,
            ScannedDevice {
                name: Some("Bose QC Ultra 2 HP".into()),
                address: "68-F2-1F-0D-FE-42".into(),
                rssi: None,
                connected: true,
            },
        );

        assert_eq!(devices.len(), 1);
        let device = devices.values().next().unwrap();
        assert_eq!(device.name.as_deref(), Some("Bose QC Ultra 2 HP"));
        assert!(device.connected);
    }
}
