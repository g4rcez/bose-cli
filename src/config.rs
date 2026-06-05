use crate::domain::{self, DeviceRef, Eq, ImmersiveAudio, ModePreset, NoiseControl};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub const MAX_CUSTOM_MODES: usize = 7;
pub const CONFIG_DIR: &str = ".config/bosecli";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub fn default_config_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .context("$HOME is not set; pass --config <path> to choose a config file")?;
    Ok(default_config_path_from_home(home))
}

pub fn default_config_path_from_home(home: impl Into<PathBuf>) -> PathBuf {
    home.into().join(CONFIG_DIR).join(CONFIG_FILE_NAME)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub selected_device: Option<DeviceRef>,
    #[serde(default)]
    pub active_mode: Option<String>,
    #[serde(default)]
    pub custom_modes: Vec<ModePreset>,
    #[serde(default = "default_noise")]
    pub noise: NoiseControl,
    #[serde(default = "default_immersive")]
    pub immersive: ImmersiveAudio,
    #[serde(default = "default_eq")]
    pub eq: Eq,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let quiet = domain::builtin_modes()
            .into_iter()
            .find(|mode| mode.name == "Quiet")
            .expect("Quiet built-in mode exists");
        Self {
            selected_device: None,
            active_mode: Some(quiet.name),
            custom_modes: Vec::new(),
            noise: quiet.noise,
            immersive: quiet.immersive,
            eq: default_eq(),
        }
    }
}

fn default_noise() -> NoiseControl {
    domain::builtin_modes()
        .into_iter()
        .find(|mode| mode.name == "Quiet")
        .expect("Quiet built-in mode exists")
        .noise
}

fn default_immersive() -> ImmersiveAudio {
    domain::builtin_modes()
        .into_iter()
        .find(|mode| mode.name == "Quiet")
        .expect("Quiet built-in mode exists")
        .immersive
}

fn default_eq() -> Eq {
    Eq {
        bass: 0,
        mid: 0,
        treble: 0,
    }
}

impl ConfigFile {
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            let s = fs::read_to_string(path)?;
            let config: Self = toml::from_str(&s).context("parse config")?;
            config.validate()?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
    pub fn all_modes(&self) -> Vec<ModePreset> {
        let mut modes = domain::builtin_modes();
        modes.extend(self.custom_modes.iter().cloned());
        modes
    }

    pub fn find_mode(&self, name: &str) -> Option<ModePreset> {
        self.all_modes()
            .into_iter()
            .find(|mode| mode.name.eq_ignore_ascii_case(name))
    }

    pub fn upsert_custom_mode(&mut self, mode: ModePreset) -> Result<()> {
        if domain::builtin_modes()
            .iter()
            .any(|builtin| builtin.name.eq_ignore_ascii_case(&mode.name))
        {
            bail!("cannot replace built-in mode '{}'", mode.name);
        }

        if let Some(existing) = self
            .custom_modes
            .iter_mut()
            .find(|custom| custom.name.eq_ignore_ascii_case(&mode.name))
        {
            *existing = mode;
        } else {
            if self.custom_modes.len() >= MAX_CUSTOM_MODES {
                bail!("headphones support up to {MAX_CUSTOM_MODES} custom modes");
            }
            self.custom_modes.push(mode);
            self.custom_modes.sort_by(|a, b| a.name.cmp(&b.name));
        }

        Ok(())
    }

    pub fn remove_custom_mode(&mut self, name: &str) -> bool {
        let before = self.custom_modes.len();
        self.custom_modes
            .retain(|mode| !mode.name.eq_ignore_ascii_case(name));
        before != self.custom_modes.len()
    }

    pub fn sync_active_mode_to_current_audio(&mut self) {
        self.active_mode = self
            .all_modes()
            .into_iter()
            .find(|mode| mode.noise == self.noise && mode.immersive == self.immersive)
            .map(|mode| mode.name);
    }

    pub fn validate(&self) -> Result<()> {
        NoiseControl::new(self.noise.enabled, self.noise.level).context("invalid noise config")?;
        Eq::new(self.eq.bass, self.eq.mid, self.eq.treble).context("invalid eq config")?;

        if self.custom_modes.len() > MAX_CUSTOM_MODES {
            bail!("config has more than {MAX_CUSTOM_MODES} custom modes");
        }

        let builtin_names = domain::builtin_modes()
            .into_iter()
            .map(|mode| mode.name.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let mut custom_names = BTreeSet::new();

        for mode in &self.custom_modes {
            let trimmed_name =
                domain::validate_mode_name(&mode.name).context("invalid custom mode")?;
            if trimmed_name != mode.name {
                bail!(
                    "custom mode '{}' has leading or trailing whitespace",
                    mode.name
                );
            }

            let lower_name = mode.name.to_ascii_lowercase();
            if builtin_names.contains(&lower_name) {
                bail!("custom mode '{}' conflicts with a built-in mode", mode.name);
            }
            if !custom_names.insert(lower_name) {
                bail!("duplicate custom mode '{}'", mode.name);
            }

            NoiseControl::new(mode.noise.enabled, mode.noise.level)
                .with_context(|| format!("invalid noise config for mode '{}'", mode.name))?;
        }

        if let Some(active_mode) = &self.active_mode {
            domain::validate_mode_name(active_mode).context("invalid active mode")?;
            if self.find_mode(active_mode).is_none() {
                bail!("active mode '{active_mode}' does not exist");
            }
        }

        Ok(())
    }
}
