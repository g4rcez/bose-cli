use bose_cli::{
    config::{ConfigFile, MAX_CUSTOM_MODES},
    domain::{ImmersiveAudio, ModePreset, NoiseControl},
};

#[test]
fn default_config_starts_in_quiet_mode() {
    let config = ConfigFile::default();

    assert_eq!(config.active_mode.as_deref(), Some("Quiet"));
    assert_eq!(config.noise.level, 10);
    assert_eq!(config.immersive, ImmersiveAudio::Off);
}

#[test]
fn custom_modes_round_trip_through_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bose.toml");
    let mut config = ConfigFile::default();
    config
        .upsert_custom_mode(
            ModePreset::new(
                "Commute",
                NoiseControl::new(true, 6).unwrap(),
                ImmersiveAudio::Still,
            )
            .unwrap(),
        )
        .unwrap();

    config.save(&path).unwrap();
    let loaded = ConfigFile::load_or_default(&path).unwrap();
    let commute = loaded.find_mode("commute").unwrap();

    assert_eq!(commute.name, "Commute");
    assert_eq!(commute.noise.level, 6);
    assert_eq!(commute.immersive, ImmersiveAudio::Still);
}

#[test]
fn rejects_invalid_persisted_noise_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bose.toml");
    std::fs::write(
        &path,
        r#"
active_mode = "Quiet"
custom_modes = []
immersive = "Off"

[noise]
enabled = true
level = 11

[eq]
bass = 0
mid = 0
treble = 0
"#,
    )
    .unwrap();

    assert!(ConfigFile::load_or_default(&path).is_err());
}

#[test]
fn enforces_headphone_custom_mode_limit() {
    let mut config = ConfigFile::default();

    for index in 0..MAX_CUSTOM_MODES {
        config
            .upsert_custom_mode(
                ModePreset::new(
                    format!("Custom {index}"),
                    NoiseControl::new(true, index as u8).unwrap(),
                    ImmersiveAudio::Off,
                )
                .unwrap(),
            )
            .unwrap();
    }

    assert!(config
        .upsert_custom_mode(
            ModePreset::new(
                "One too many",
                NoiseControl::new(true, 5).unwrap(),
                ImmersiveAudio::Off,
            )
            .unwrap(),
        )
        .is_err());
}
