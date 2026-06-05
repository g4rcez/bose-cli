use crate::{
    bmap::{self, Operator},
    config::ConfigFile,
    domain::{self, ImmersiveAudio, ModePreset, ModelId},
};
use anyhow::{bail, Context, Result};

use super::{Capabilities, HeadphoneModel};

const RFCOMM_CHANNEL: u8 = 2;

pub struct QcUltra2Model;

impl HeadphoneModel for QcUltra2Model {
    fn id(&self) -> ModelId {
        ModelId::QcUltraHeadphones2
    }

    fn display_name(&self) -> &'static str {
        "Bose QuietComfort Ultra Headphones (2nd Gen)"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[
            "Bose QC Ultra 2 HP",
            "Bose QuietComfort Ultra Headphones (2nd Gen)",
            "Bose QuietComfort Ultra Headphones 2",
            "Bose QuietComfort Ultra Headphones II",
            "Bose QC Ultra Headphones 2",
        ]
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            config_writes: true,
            battery: true,
        }
    }

    fn rfcomm_channel(&self) -> Option<u8> {
        Some(RFCOMM_CHANNEL)
    }

    fn builtin_modes(&self) -> Vec<ModePreset> {
        domain::builtin_modes()
    }

    fn sync_config(&self, config: &ConfigFile) -> Result<()> {
        sync_config(config)
    }

    fn read_battery(&self, config: &ConfigFile) -> Result<u8> {
        read_battery(config)
    }
}

fn mode_index(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "quiet" => Some(0),
        "aware" => Some(1),
        "immersion" => Some(2),
        "cinema" => Some(3),
        _ => None,
    }
}

fn immersive_to_spatial(value: &ImmersiveAudio) -> u8 {
    match value {
        ImmersiveAudio::Off => 0,
        ImmersiveAudio::Still => 1,
        ImmersiveAudio::Motion => 2,
    }
}

fn eq_payload(value: i8, band_id: u8) -> [u8; 2] {
    [value as u8, band_id]
}

fn audio_payload(config: &ConfigFile) -> [u8; 5] {
    [
        10 - config.noise.level,
        0,
        immersive_to_spatial(&config.immersive),
        0,
        if config.noise.enabled { 1 } else { 0 },
    ]
}

fn device_address(config: &ConfigFile) -> Result<&str> {
    Ok(config
        .selected_device
        .as_ref()
        .context("no selected device")?
        .address
        .as_str())
}

pub fn sync_config(config: &ConfigFile) -> Result<()> {
    let address = device_address(config)?;

    if let Some(active) = &config.active_mode {
        let index = mode_index(active).with_context(|| {
            format!(
                "headphone sync only supports built-in modes Quiet, Aware, Immersion, and Cinema; custom mode '{active}' was saved locally only"
            )
        })?;
        let payload = [index, 0];
        let _ = bmap::send(address, RFCOMM_CHANNEL, 31, 3, Operator::Start, &payload)
            .context("switch audio mode")?;
        std::thread::sleep(std::time::Duration::from_millis(750));
    }

    let _ = bmap::send(
        address,
        RFCOMM_CHANNEL,
        31,
        10,
        Operator::SetGet,
        &audio_payload(config),
    )
    .context("set noise/immersive audio settings")?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    for (band_id, value) in [
        (0, config.eq.bass),
        (1, config.eq.mid),
        (2, config.eq.treble),
    ] {
        let _ = bmap::send(
            address,
            RFCOMM_CHANNEL,
            1,
            7,
            Operator::SetGet,
            &eq_payload(value, band_id),
        )
        .with_context(|| format!("set EQ band {band_id}"))?;
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Ok(())
}

pub fn read_battery(config: &ConfigFile) -> Result<u8> {
    let address = device_address(config)?;
    let frame = bmap::send(address, RFCOMM_CHANNEL, 2, 2, Operator::Get, &[])?;
    if frame.fblock != 2 || frame.func != 2 {
        bail!(
            "unexpected battery response [{}.{}]",
            frame.fblock,
            frame.func
        );
    }
    frame
        .payload
        .first()
        .copied()
        .context("battery response missing payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeviceRef, Eq, NoiseControl};

    #[test]
    fn maps_audio_and_eq_payloads() {
        let config = ConfigFile {
            selected_device: Some(DeviceRef {
                address: "AA:BB".into(),
                name: Some("Bose QC Ultra 2 HP".into()),
                model: Some(ModelId::QcUltraHeadphones2),
            }),
            active_mode: Some("Quiet".into()),
            custom_modes: vec![],
            noise: NoiseControl {
                enabled: true,
                level: 7,
            },
            immersive: ImmersiveAudio::Motion,
            eq: Eq {
                bass: -2,
                mid: 0,
                treble: 3,
            },
        };
        assert_eq!(mode_index("Quiet"), Some(0));
        assert_eq!(mode_index("Cinema"), Some(3));
        assert_eq!(audio_payload(&config), [3, 0, 2, 0, 1]);
        assert_eq!(eq_payload(-2, 0), [254, 0]);
    }
}
