use crate::{
    config::ConfigFile,
    domain::{DeviceRef, ModePreset, ModelId},
};
use anyhow::{bail, Context, Result};

pub mod qc_ultra_2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub config_writes: bool,
    pub battery: bool,
}

pub trait HeadphoneModel: Sync {
    fn id(&self) -> ModelId;
    fn display_name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str];
    fn capabilities(&self) -> Capabilities;
    fn rfcomm_channel(&self) -> Option<u8>;
    fn builtin_modes(&self) -> Vec<ModePreset>;
    fn sync_config(&self, config: &ConfigFile) -> Result<()>;
    fn read_battery(&self, config: &ConfigFile) -> Result<u8>;
}

struct UnsupportedBoseModel {
    id: ModelId,
    display_name: &'static str,
    aliases: &'static [&'static str],
}

impl HeadphoneModel for UnsupportedBoseModel {
    fn id(&self) -> ModelId {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            config_writes: false,
            battery: false,
        }
    }

    fn rfcomm_channel(&self) -> Option<u8> {
        None
    }

    fn builtin_modes(&self) -> Vec<ModePreset> {
        Vec::new()
    }

    fn sync_config(&self, _config: &ConfigFile) -> Result<()> {
        bail!(
            "model '{}' is recognized but configuration writes are not supported yet",
            self.display_name
        )
    }

    fn read_battery(&self, _config: &ConfigFile) -> Result<u8> {
        bail!(
            "model '{}' is recognized but battery readback is not supported yet",
            self.display_name
        )
    }
}

static QC_ULTRA_2: qc_ultra_2::QcUltra2Model = qc_ultra_2::QcUltra2Model;
static QC_ULTRA_HEADPHONES: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::QcUltraHeadphones,
    display_name: "Bose QuietComfort Ultra Headphones",
    aliases: &["Bose QuietComfort Ultra Headphones", "Bose QC Ultra HP"],
};
static QUIETCOMFORT_HEADPHONES: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::QuietComfortHeadphones,
    display_name: "Bose QuietComfort Headphones",
    aliases: &["Bose QuietComfort Headphones", "Bose QC Headphones"],
};
static QUIETCOMFORT_45: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::QuietComfort45,
    display_name: "Bose QuietComfort 45",
    aliases: &["Bose QuietComfort 45", "Bose QC45", "Bose QC 45"],
};
static NCH_700: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::NoiseCancellingHeadphones700,
    display_name: "Bose Noise Cancelling Headphones 700",
    aliases: &[
        "Bose Noise Cancelling Headphones 700",
        "Bose NC 700",
        "Bose NCH 700",
    ],
};
static QC_ULTRA_EARBUDS: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::QuietComfortUltraEarbuds,
    display_name: "Bose QuietComfort Ultra Earbuds",
    aliases: &["Bose QuietComfort Ultra Earbuds", "Bose QC Ultra Earbuds"],
};
static QC_EARBUDS_2: UnsupportedBoseModel = UnsupportedBoseModel {
    id: ModelId::QuietComfortEarbuds2,
    display_name: "Bose QuietComfort Earbuds II",
    aliases: &[
        "Bose QuietComfort Earbuds II",
        "Bose QuietComfort Earbuds 2",
        "Bose QC Earbuds II",
    ],
};

static REGISTERED_MODELS: [&'static dyn HeadphoneModel; 7] = [
    &QC_ULTRA_2,
    &QC_ULTRA_HEADPHONES,
    &QUIETCOMFORT_HEADPHONES,
    &QUIETCOMFORT_45,
    &NCH_700,
    &QC_ULTRA_EARBUDS,
    &QC_EARBUDS_2,
];

pub fn all_models() -> &'static [&'static dyn HeadphoneModel] {
    &REGISTERED_MODELS
}

pub fn model(id: ModelId) -> &'static dyn HeadphoneModel {
    all_models()
        .iter()
        .copied()
        .find(|model| model.id() == id)
        .expect("registered model id")
}

pub fn infer_model_from_name(name: &str) -> Option<ModelId> {
    let normalized = normalize_name(name);
    all_models().iter().find_map(|profile| {
        profile
            .aliases()
            .iter()
            .any(|alias| normalize_name(alias) == normalized)
            .then_some(profile.id())
    })
}

pub fn resolve_device_model(device: &DeviceRef) -> Option<ModelId> {
    let inferred = device.name.as_deref().and_then(infer_model_from_name);
    match (device.model, inferred) {
        (Some(explicit), Some(inferred)) if explicit != inferred => None,
        (Some(explicit), _) => Some(explicit),
        (None, inferred) => inferred,
    }
}

pub fn selected_device_model(config: &ConfigFile) -> Result<&'static dyn HeadphoneModel> {
    let device = config
        .selected_device
        .as_ref()
        .context("no selected device")?;
    let model_id = resolve_device_model(device).with_context(|| {
        format!(
            "unknown Bose model for selected device {}; refusing to send configuration writes",
            device.display()
        )
    })?;
    Ok(model(model_id))
}

pub fn ensure_selected_device_supports_config(config: &ConfigFile) -> Result<()> {
    let Some(_) = config.selected_device else {
        return Ok(());
    };
    let model = selected_device_model(config)?;
    if !model.capabilities().config_writes {
        bail!(
            "selected device model '{}' does not support mode/noise/immersive/EQ configuration in this CLI yet",
            model.display_name()
        );
    }
    Ok(())
}

pub fn sync_selected_config(config: &ConfigFile) -> Result<()> {
    let model = selected_device_model(config)?;
    model.sync_config(config)
}

pub fn read_selected_battery(config: &ConfigFile) -> Result<u8> {
    let model = selected_device_model(config)?;
    model.read_battery(config)
}

pub fn selected_rfcomm_channel(config: &ConfigFile) -> Result<u8> {
    let model = selected_device_model(config)?;
    model.rfcomm_channel().with_context(|| {
        format!(
            "model '{}' does not have a known RFCOMM channel in this CLI",
            model.display_name()
        )
    })
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_qc_ultra_2_aliases() {
        assert_eq!(
            infer_model_from_name("Bose QC Ultra 2 HP"),
            Some(ModelId::QcUltraHeadphones2)
        );
        assert_eq!(
            infer_model_from_name("Bose QuietComfort Ultra Headphones (2nd Gen)"),
            Some(ModelId::QcUltraHeadphones2)
        );
    }

    #[test]
    fn unknown_name_is_not_resolved() {
        assert_eq!(infer_model_from_name("WH-1000XM3"), None);
    }

    #[test]
    fn unsupported_model_rejects_config_writes() {
        let mut config = ConfigFile::default();
        config.selected_device = Some(DeviceRef {
            address: "AA:BB".into(),
            name: Some("Bose QuietComfort 45".into()),
            model: None,
        });

        let err = ensure_selected_device_supports_config(&config).unwrap_err();

        assert!(err.to_string().contains("does not support"));
    }

    #[test]
    fn model_name_mismatch_is_not_trusted() {
        let device = DeviceRef {
            address: "AA:BB".into(),
            name: Some("Bose QuietComfort 45".into()),
            model: Some(ModelId::QcUltraHeadphones2),
        };

        assert_eq!(resolve_device_model(&device), None);
    }
}
