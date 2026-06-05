use clap::Parser;

#[test]
fn parses_global_config_and_subcommand() {
    let args = bose_cli::cli::Args::parse_from(["bose", "--config", "x.toml", "config", "path"]);
    assert_eq!(args.config.as_ref().unwrap().to_string_lossy(), "x.toml");
}

#[test]
fn default_config_path_uses_home_config_directory() {
    let path = bose_cli::config::default_config_path_from_home("/home/alice");

    assert_eq!(
        path.to_string_lossy(),
        "/home/alice/.config/bosecli/config.toml"
    );
}

#[test]
fn config_override_is_optional() {
    let args = bose_cli::cli::Args::parse_from(["bose", "config", "path"]);

    assert!(args.config.is_none());
}

#[test]
fn parses_bool_value_for_noise_set() {
    let args = bose_cli::cli::Args::parse_from([
        "bose",
        "noise",
        "set",
        "--enabled",
        "false",
        "--level",
        "3",
    ]);

    match args.command {
        bose_cli::cli::Command::Noise { command } => match command {
            bose_cli::cli::NoiseCommand::Set { enabled, level } => {
                assert!(!enabled);
                assert_eq!(level, 3);
            }
            _ => panic!("expected noise set"),
        },
        _ => panic!("expected noise command"),
    }
}

#[test]
fn parses_custom_mode_preset_flags() {
    let args = bose_cli::cli::Args::parse_from([
        "bose",
        "mode",
        "add",
        "Commute",
        "--noise-enabled",
        "true",
        "--noise-level",
        "4",
        "--immersive",
        "motion",
    ]);

    match args.command {
        bose_cli::cli::Command::Mode { command } => match command {
            bose_cli::cli::ModeCommand::Add {
                name,
                noise_enabled,
                noise_level,
                immersive,
            } => {
                assert_eq!(name, "Commute");
                assert_eq!(noise_enabled, Some(true));
                assert_eq!(noise_level, Some(4));
                assert!(matches!(
                    immersive,
                    Some(bose_cli::cli::ImmersiveValue::Motion)
                ));
            }
            _ => panic!("expected mode add"),
        },
        _ => panic!("expected mode command"),
    }
}

#[test]
fn json_and_select_device_flags_conflict() {
    assert!(
        bose_cli::cli::Args::try_parse_from(["bose", "devices", "--json", "--select"]).is_err()
    );
}

#[tokio::test]
async fn config_path_does_not_parse_malformed_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bose.toml");
    std::fs::write(&path, "not valid toml =").unwrap();

    let args = bose_cli::cli::Args::parse_from([
        "bose",
        "--config",
        path.to_str().unwrap(),
        "config",
        "path",
    ]);

    bose_cli::cli::run(args).await.unwrap();
}

#[tokio::test]
async fn config_init_force_repairs_malformed_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bose.toml");
    std::fs::write(&path, "not valid toml =").unwrap();

    let args = bose_cli::cli::Args::parse_from([
        "bose",
        "--config",
        path.to_str().unwrap(),
        "config",
        "init",
        "--force",
    ]);

    bose_cli::cli::run(args).await.unwrap();
    assert!(bose_cli::config::ConfigFile::load_or_default(&path).is_ok());
}

#[tokio::test]
async fn manual_noise_change_syncs_active_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bose.toml");

    let args = bose_cli::cli::Args::parse_from([
        "bose",
        "--config",
        path.to_str().unwrap(),
        "noise",
        "set",
        "--enabled",
        "true",
        "--level",
        "5",
    ]);

    bose_cli::cli::run(args).await.unwrap();
    let config = bose_cli::config::ConfigFile::load_or_default(&path).unwrap();
    assert_eq!(config.active_mode, None);
}
