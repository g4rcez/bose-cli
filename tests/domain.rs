use bose_cli::domain::{builtin_modes, Eq, ImmersiveAudio, ModePreset, NoiseControl};

#[test]
fn rejects_bad_noise_and_eq() {
    assert!(NoiseControl::new(true, 11).is_err());
    assert!(Eq::new(0, 0, 11).is_err());
}

#[test]
fn built_in_modes_preserve_bose_presets() {
    let modes = builtin_modes();
    assert!(modes.iter().any(|mode| mode.name == "Quiet"));
    assert!(modes.iter().any(|mode| mode.name == "Aware"));
    assert!(modes.iter().any(|mode| mode.name == "Immersion"));
    assert!(modes.iter().any(|mode| mode.name == "Cinema"));

    let cinema = modes.iter().find(|mode| mode.name == "Cinema").unwrap();
    assert_eq!(cinema.noise.level, 10);
    assert_eq!(cinema.immersive, ImmersiveAudio::Still);
}

#[test]
fn mode_preset_trims_and_rejects_blank_names() {
    let mode = ModePreset::new(
        "  Commute  ",
        NoiseControl::new(true, 5).unwrap(),
        ImmersiveAudio::Motion,
    )
    .unwrap();

    assert_eq!(mode.name, "Commute");
    assert!(ModePreset::new(
        " ",
        NoiseControl::new(true, 5).unwrap(),
        ImmersiveAudio::Motion,
    )
    .is_err());
}
