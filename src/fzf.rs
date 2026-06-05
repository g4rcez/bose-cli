use crate::bluetooth::ScannedDevice;
use crate::domain::DeviceRef;
use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

pub fn is_available() -> bool {
    Command::new("fzf")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn select_device(devices: &[ScannedDevice]) -> Result<DeviceRef> {
    if devices.is_empty() {
        bail!("no Bluetooth devices found to select");
    }
    if !is_available() {
        bail!("fzf is required for --select but is not installed or not on PATH");
    }
    let input = devices
        .iter()
        .enumerate()
        .map(|(index, d)| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                index,
                d.name
                    .as_deref()
                    .map(sanitize_cell)
                    .unwrap_or_else(|| "<unknown>".into()),
                d.address,
                d.model
                    .map(|model| model.to_string())
                    .unwrap_or_else(|| "unknown-model".into()),
                d.rssi
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "<n/a>".into()),
                if d.connected {
                    "connected"
                } else {
                    "discovered"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut child = Command::new("fzf")
        .args(["--delimiter", "\t", "--with-nth", "2,3,4,5,6"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("launch fzf")?;
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .context("fzf stdin")?
        .write_all(input.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("fzf selection cancelled");
    }
    let line = String::from_utf8(out.stdout)?;
    let Some(raw_index) = line.trim_end().split('\t').next() else {
        bail!("fzf returned an invalid device row");
    };
    let index = raw_index
        .parse::<usize>()
        .context("fzf returned an invalid device index")?;
    let device = devices
        .get(index)
        .context("fzf selected a device index outside the scanned list")?;

    Ok(DeviceRef {
        name: device.name.clone(),
        address: device.address.clone(),
        model: device.model,
    })
}

fn sanitize_cell(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}
