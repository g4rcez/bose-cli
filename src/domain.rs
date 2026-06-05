use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRef {
    pub address: String,
    pub name: Option<String>,
}
impl DeviceRef {
    pub fn display(&self) -> String {
        format!(
            "{} ({})",
            self.name.clone().unwrap_or_else(|| "<unknown>".into()),
            self.address
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoiseControl {
    pub enabled: bool,
    pub level: u8,
}
impl NoiseControl {
    pub fn new(enabled: bool, level: u8) -> Result<Self> {
        if level > 10 {
            bail!("noise level must be 0..10");
        }
        Ok(Self { enabled, level })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImmersiveAudio {
    Off,
    Still,
    Motion,
}

impl fmt::Display for ImmersiveAudio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Still => write!(f, "Still"),
            Self::Motion => write!(f, "Motion"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eq {
    pub bass: i8,
    pub mid: i8,
    pub treble: i8,
}
impl Eq {
    pub fn new(bass: i8, mid: i8, treble: i8) -> Result<Self> {
        for v in [bass, mid, treble] {
            if !(-10..=10).contains(&v) {
                bail!("eq values must be -10..10");
            }
        }
        Ok(Self { bass, mid, treble })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModePreset {
    pub name: String,
    pub noise: NoiseControl,
    pub immersive: ImmersiveAudio,
}

impl ModePreset {
    pub fn new(
        name: impl Into<String>,
        noise: NoiseControl,
        immersive: ImmersiveAudio,
    ) -> Result<Self> {
        let raw_name = name.into();
        let name = validate_mode_name(&raw_name)?;
        Ok(Self {
            name,
            noise,
            immersive,
        })
    }
}

impl fmt::Display for ModePreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: noise enabled={} level={} immersive={}",
            self.name, self.noise.enabled, self.noise.level, self.immersive
        )
    }
}

pub fn builtin_modes() -> Vec<ModePreset> {
    vec![
        ModePreset {
            name: "Quiet".into(),
            noise: NoiseControl {
                enabled: true,
                level: 10,
            },
            immersive: ImmersiveAudio::Off,
        },
        ModePreset {
            name: "Aware".into(),
            noise: NoiseControl {
                enabled: true,
                level: 0,
            },
            immersive: ImmersiveAudio::Off,
        },
        ModePreset {
            name: "Immersion".into(),
            noise: NoiseControl {
                enabled: true,
                level: 10,
            },
            immersive: ImmersiveAudio::Motion,
        },
        ModePreset {
            name: "Cinema".into(),
            noise: NoiseControl {
                enabled: true,
                level: 10,
            },
            immersive: ImmersiveAudio::Still,
        },
    ]
}

pub fn validate_mode_name(name: &str) -> Result<String> {
    if name.trim().is_empty() {
        bail!("mode name cannot be empty");
    }
    Ok(name.trim().to_string())
}
