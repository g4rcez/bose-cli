use crate::{
    bluetooth, bmap,
    config::{self, ConfigFile},
    domain, fzf, models, tui,
};
use anyhow::{Context, Result};
use clap::{ArgAction, Args as ClapArgs, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

type SyncHeadphones = fn(&ConfigFile) -> Result<()>;
type ReadBattery = fn(&ConfigFile) -> Result<u8>;

#[derive(Parser, Debug)]
#[command(name = "bose", version, about = "Bose CLI")]
pub struct Args {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Config path (default: $HOME/.config/bosecli/config.toml)"
    )]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Doctor,
    Devices(DevicesArgs),
    Status(StatusArgs),
    Sync,
    #[command(hide = true)]
    BmapRaw {
        fblock: u8,
        func: u8,
        op: u8,
        payload_hex: Option<String>,
    },
    Mode {
        #[command(subcommand)]
        command: ModeCommand,
    },
    Noise {
        #[command(subcommand)]
        command: NoiseCommand,
    },
    Immersive {
        #[command(subcommand)]
        command: ImmersiveCommand,
    },
    Eq {
        #[command(subcommand)]
        command: EqCommand,
    },
    Tui {
        #[arg(long, default_value_t = 5)]
        scan_seconds: u64,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(ClapArgs, Debug)]
pub struct DevicesArgs {
    #[arg(long, default_value_t = 5)]
    pub scan_seconds: u64,
    #[arg(long, conflicts_with = "select")]
    pub json: bool,
    #[arg(long, conflicts_with = "json")]
    pub select: bool,
}

#[derive(ClapArgs, Debug)]
pub struct StatusArgs {
    #[arg(long, hide = true)]
    pub headphones: bool,
}

#[derive(Subcommand, Debug)]
pub enum ModeCommand {
    List,
    Set {
        name: String,
    },
    Add {
        name: String,
        #[arg(long, action = ArgAction::Set)]
        noise_enabled: Option<bool>,
        #[arg(long)]
        noise_level: Option<u8>,
        #[arg(long)]
        immersive: Option<ImmersiveValue>,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum NoiseCommand {
    Set {
        #[arg(long, action = ArgAction::Set)]
        enabled: bool,
        #[arg(long)]
        level: u8,
    },
    Show,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum ImmersiveValue {
    Off,
    Still,
    Motion,
}

impl From<ImmersiveValue> for domain::ImmersiveAudio {
    fn from(v: ImmersiveValue) -> Self {
        match v {
            ImmersiveValue::Off => Self::Off,
            ImmersiveValue::Still => Self::Still,
            ImmersiveValue::Motion => Self::Motion,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum ImmersiveCommand {
    Set { value: ImmersiveValue },
    Show,
}

#[derive(Subcommand, Debug)]
pub enum EqCommand {
    Set {
        #[arg(long)]
        bass: i8,
        #[arg(long)]
        mid: i8,
        #[arg(long)]
        treble: i8,
    },
    Show,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    Init {
        #[arg(long)]
        force: bool,
    },
    Show,
    Path,
}

pub async fn run(args: Args) -> Result<()> {
    let config_path = args.config.unwrap_or(config::default_config_path()?);

    match args.command {
        Command::Doctor => {
            println!("config: {}", config_path.display());
            let config_status = if config_path.exists() {
                match ConfigFile::load_or_default(&config_path) {
                    Ok(_) => "valid".to_string(),
                    Err(err) => format!("invalid ({err})"),
                }
            } else {
                "missing; defaults will be used".to_string()
            };
            println!("config status: {config_status}");
            println!(
                "fzf: {}",
                if fzf::is_available() {
                    "available"
                } else {
                    "missing"
                }
            );
            let bluetooth_status = match bluetooth::has_adapter().await {
                Ok(true) => "available".to_string(),
                Ok(false) => "missing".to_string(),
                Err(err) => format!("error ({err})"),
            };
            println!("bluetooth adapter: {bluetooth_status}");
            println!("caveat: Bluetooth discovery/listing plus BMAP sync over macOS RFCOMM serial for mode/noise/immersive/EQ.");
        }
        Command::Devices(cmd) => {
            let devices = bluetooth::scan_devices(cmd.scan_seconds).await?;
            if cmd.json {
                println!("{}", serde_json::to_string_pretty(&devices)?);
            } else {
                for d in &devices {
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        d.name.as_deref().unwrap_or("<unknown>"),
                        d.address,
                        d.model
                            .map(|model| model.to_string())
                            .unwrap_or_else(|| "unknown-model".into()),
                        d.rssi
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<n/a>".into()),
                        if d.connected {
                            "connected"
                        } else {
                            "discovered"
                        },
                    );
                }
            }
            if cmd.select {
                let selected = fzf::select_device(&devices)?;
                let mut config = ConfigFile::load_or_default(&config_path)?;
                config.selected_device = Some(selected.clone());
                config.save(&config_path)?;
                println!("saved selected device: {}", selected.address);
            }
        }
        Command::Status(status_args) => {
            let config = ConfigFile::load_or_default(&config_path)?;
            println!(
                "selected device: {}",
                config
                    .selected_device
                    .as_ref()
                    .map(|d| d.display())
                    .unwrap_or_else(|| "<none>".into())
            );
            println!(
                "selected model: {}",
                config
                    .selected_device
                    .as_ref()
                    .and_then(models::resolve_device_model)
                    .map(|model| model.to_string())
                    .unwrap_or_else(|| "<unknown>".into())
            );
            println!(
                "active mode: {}",
                config.active_mode.as_deref().unwrap_or("<none>")
            );
            println!(
                "noise: enabled={} level={}",
                config.noise.enabled, config.noise.level
            );
            println!("immersive: {}", config.immersive);
            println!(
                "eq: bass={} mid={} treble={}",
                config.eq.bass, config.eq.mid, config.eq.treble
            );
            print_status_battery(
                &config,
                models::read_selected_battery,
                status_args.headphones,
            );
        }
        Command::Sync => {
            let config = ConfigFile::load_or_default(&config_path)?;
            models::sync_selected_config(&config)?;
            println!("synced config to headphones");
        }
        Command::BmapRaw {
            fblock,
            func,
            op,
            payload_hex,
        } => {
            if std::env::var("BOSE_BMAP_RAW").as_deref() != Ok("1") {
                anyhow::bail!("bmap-raw is hidden and unsafe; set BOSE_BMAP_RAW=1 to use it");
            }
            let config = ConfigFile::load_or_default(&config_path)?;
            let payload = payload_hex
                .as_deref()
                .map(parse_hex)
                .transpose()?
                .unwrap_or_default();
            let channel = models::selected_rfcomm_channel(&config)?;
            let frame = bmap::send_raw(
                &config,
                channel,
                fblock,
                func,
                bmap::Operator::try_from(op)?,
                &payload,
            )?;
            println!(
                "[{}.{}] {:?}: {}",
                frame.fblock,
                frame.func,
                frame.op,
                bytes_to_hex(&frame.payload)
            );
        }
        Command::Mode { command: cmd } => {
            let mut config = ConfigFile::load_or_default(&config_path)?;
            handle_mode(cmd, &mut config, &config_path)?;
        }
        Command::Noise { command: cmd } => {
            let mut config = ConfigFile::load_or_default(&config_path)?;
            handle_noise(cmd, &mut config, &config_path)?;
        }
        Command::Immersive { command: cmd } => {
            let mut config = ConfigFile::load_or_default(&config_path)?;
            handle_immersive(cmd, &mut config, &config_path)?;
        }
        Command::Eq { command: cmd } => {
            let mut config = ConfigFile::load_or_default(&config_path)?;
            handle_eq(cmd, &mut config, &config_path)?;
        }
        Command::Tui { scan_seconds } => {
            let config = ConfigFile::load_or_default(&config_path)?;
            let (devices, scan_status) = match bluetooth::scan_devices(scan_seconds).await {
                Ok(devices) if devices.is_empty() => (
                    devices,
                    Some(String::from(
                        "No Bluetooth devices found during scan/listing.",
                    )),
                ),
                Ok(devices) => {
                    let status = format!("Found {} Bluetooth device(s).", devices.len());
                    (devices, Some(status))
                }
                Err(err) => (
                    Vec::new(),
                    Some(format!("Bluetooth scan/listing failed: {err}")),
                ),
            };
            tui::run(config, config_path, devices, scan_status).await?;
        }
        Command::Config { command: cmd } => match cmd {
            ConfigCommand::Init { force } => {
                if config_path.exists() && !force {
                    anyhow::bail!(
                        "{} already exists; pass --force to overwrite it",
                        config_path.display()
                    );
                }
                ConfigFile::default().save(&config_path)?;
                println!("initialized {}", config_path.display());
            }
            ConfigCommand::Show => {
                let config = ConfigFile::load_or_default(&config_path)?;
                println!("{}", toml::to_string_pretty(&config)?);
            }
            ConfigCommand::Path => println!("{}", config_path.display()),
        },
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum BatteryReadback {
    Unavailable,
    Level(u8),
    Error(String),
}

fn status_battery_readback(
    config: &ConfigFile,
    read_battery: ReadBattery,
    force_read: bool,
) -> BatteryReadback {
    let supports_battery = config
        .selected_device
        .as_ref()
        .and_then(models::resolve_device_model)
        .map(|model_id| models::model(model_id).capabilities().battery)
        .unwrap_or(false);

    if !supports_battery && !force_read {
        return BatteryReadback::Unavailable;
    }

    match read_battery(config) {
        Ok(level) => BatteryReadback::Level(level),
        Err(err) => BatteryReadback::Error(err.to_string()),
    }
}

fn print_status_battery(config: &ConfigFile, read_battery: ReadBattery, force_read: bool) {
    match status_battery_readback(config, read_battery, force_read) {
        BatteryReadback::Unavailable => {}
        BatteryReadback::Level(level) => println!("battery: {level}%"),
        BatteryReadback::Error(err) => {
            eprintln!("warning: failed to read battery from headphones: {err}")
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SyncTransactionOutcome {
    Synced,
    SavedLocallyNoSelectedDevice,
}

fn sync_headphones_transaction(
    previous: ConfigFile,
    config: &mut ConfigFile,
    path: &PathBuf,
    sync_headphones: SyncHeadphones,
) -> Result<SyncTransactionOutcome> {
    if config.selected_device.is_none() {
        eprintln!("warning: saved config locally but no selected device is available to sync");
        return Ok(SyncTransactionOutcome::SavedLocallyNoSelectedDevice);
    }

    match sync_headphones(config) {
        Ok(()) => {
            println!("synced config to headphones");
            Ok(SyncTransactionOutcome::Synced)
        }
        Err(sync_err) => {
            eprintln!("error: failed to sync requested change to headphones: {sync_err}");
            *config = previous;

            let rollback_save = config.save(path);
            let rollback_sync = sync_headphones(config);

            match (rollback_save, rollback_sync) {
                (Ok(()), Ok(())) => {
                    eprintln!(
                        "rolled back local config and synced previous desired settings to headphones"
                    );
                    anyhow::bail!(
                        "failed to sync requested change; rolled back to previous desired settings"
                    )
                }
                (Ok(()), Err(rollback_err)) => {
                    eprintln!(
                        "rolled back local config, but headset rollback failed: {rollback_err}; headset state unknown"
                    );
                    anyhow::bail!(
                        "failed to sync requested change; rolled back local config, but headset rollback failed: {rollback_err}; headset state unknown"
                    )
                }
                (Err(save_err), Ok(())) => Err::<SyncTransactionOutcome, _>(save_err).with_context(|| {
                    "failed to rollback local config after sync failure; synced previous desired settings to headphones"
                }),
                (Err(save_err), Err(rollback_err)) => Err::<SyncTransactionOutcome, _>(save_err).with_context(|| {
                    format!(
                        "failed to rollback local config after sync failure; headset rollback failed: {rollback_err}; headset state unknown"
                    )
                }),
            }
        }
    }
}

fn parse_hex(value: &str) -> Result<Vec<u8>> {
    let value = value.trim();
    if value.len() % 2 != 0 {
        anyhow::bail!("hex payload must have an even number of digits");
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        bytes.push(u8::from_str_radix(&value[index..index + 2], 16)?);
        index += 2;
    }
    Ok(bytes)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn handle_mode(cmd: ModeCommand, config: &mut ConfigFile, path: &PathBuf) -> Result<()> {
    match cmd {
        ModeCommand::List => {
            for m in config.all_modes() {
                println!("{}", m);
            }
        }
        ModeCommand::Set { name } => {
            models::ensure_selected_device_supports_config(config)?;
            let previous = config.clone();
            let mode = config
                .find_mode(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown mode '{name}'; use `bose mode list`"))?;
            config.active_mode = Some(mode.name.clone());
            config.noise = mode.noise;
            config.immersive = mode.immersive;
            config.save(path)?;
            sync_headphones_transaction(previous, config, path, models::sync_selected_config)?;
        }
        ModeCommand::Add {
            name,
            noise_enabled,
            noise_level,
            immersive,
        } => {
            models::ensure_selected_device_supports_config(config)?;
            let noise = domain::NoiseControl::new(
                noise_enabled.unwrap_or(config.noise.enabled),
                noise_level.unwrap_or(config.noise.level),
            )?;
            let immersive = immersive
                .map(domain::ImmersiveAudio::from)
                .unwrap_or_else(|| config.immersive.clone());
            let mode = domain::ModePreset::new(name, noise, immersive)?;
            let name = mode.name.clone();
            config.upsert_custom_mode(mode)?;
            config.save(path)?;
            println!("added mode: {}", name);
        }
        ModeCommand::Remove { name } => {
            models::ensure_selected_device_supports_config(config)?;
            if domain::builtin_modes()
                .iter()
                .any(|mode| mode.name.eq_ignore_ascii_case(&name))
            {
                anyhow::bail!("cannot remove built-in mode '{name}'");
            }
            if !config.remove_custom_mode(&name) {
                anyhow::bail!("custom mode '{name}' not found");
            }
            if config
                .active_mode
                .as_deref()
                .is_some_and(|active| active.eq_ignore_ascii_case(&name))
            {
                config.active_mode = None;
            }
            config.save(path)?;
            println!("removed mode: {}", name);
        }
    }
    Ok(())
}

fn handle_noise(cmd: NoiseCommand, config: &mut ConfigFile, path: &PathBuf) -> Result<()> {
    match cmd {
        NoiseCommand::Set { enabled, level } => {
            models::ensure_selected_device_supports_config(config)?;
            let previous = config.clone();
            config.noise = domain::NoiseControl::new(enabled, level)?;
            config.sync_active_mode_to_current_audio();
            config.save(path)?;
            sync_headphones_transaction(previous, config, path, models::sync_selected_config)?;
        }
        NoiseCommand::Show => {
            println!(
                "enabled={} level={}",
                config.noise.enabled, config.noise.level
            )
        }
    }
    Ok(())
}

fn handle_immersive(cmd: ImmersiveCommand, config: &mut ConfigFile, path: &PathBuf) -> Result<()> {
    match cmd {
        ImmersiveCommand::Set { value } => {
            models::ensure_selected_device_supports_config(config)?;
            let previous = config.clone();
            config.immersive = value.into();
            config.sync_active_mode_to_current_audio();
            config.save(path)?;
            sync_headphones_transaction(previous, config, path, models::sync_selected_config)?;
        }
        ImmersiveCommand::Show => {
            println!("{}", config.immersive)
        }
    }
    Ok(())
}

fn handle_eq(cmd: EqCommand, config: &mut ConfigFile, path: &PathBuf) -> Result<()> {
    match cmd {
        EqCommand::Set { bass, mid, treble } => {
            models::ensure_selected_device_supports_config(config)?;
            let previous = config.clone();
            config.eq = domain::Eq::new(bass, mid, treble)?;
            config.save(path)?;
            sync_headphones_transaction(previous, config, path, models::sync_selected_config)?;
        }
        EqCommand::Show => {
            println!(
                "bass={} mid={} treble={}",
                config.eq.bass, config.eq.mid, config.eq.treble
            )
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TRANSACTION_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn fail_once_then_sync(_: &ConfigFile) -> Result<()> {
        let call = TRANSACTION_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            anyhow::bail!("requested sync failed")
        }
        Ok(())
    }

    fn reset_transaction_sync_calls() {
        TRANSACTION_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn read_battery_88(_: &ConfigFile) -> Result<u8> {
        Ok(88)
    }

    fn fail_read_battery(_: &ConfigFile) -> Result<u8> {
        anyhow::bail!("rfcomm failed")
    }

    fn panic_read_battery(_: &ConfigFile) -> Result<u8> {
        panic!("battery reader should not be called")
    }

    fn selected_device_config(name: &str, model: domain::ModelId) -> ConfigFile {
        let mut config = ConfigFile::default();
        config.selected_device = Some(domain::DeviceRef {
            address: "AA:BB".into(),
            name: Some(name.into()),
            model: Some(model),
        });
        config
    }

    #[test]
    fn status_reads_battery_for_supported_selected_model() {
        let config =
            selected_device_config("Bose QC Ultra 2 HP", domain::ModelId::QcUltraHeadphones2);

        assert_eq!(
            status_battery_readback(&config, read_battery_88, false),
            BatteryReadback::Level(88)
        );
    }

    #[test]
    fn status_skips_battery_for_unsupported_selected_model() {
        let config =
            selected_device_config("Bose QuietComfort 45", domain::ModelId::QuietComfort45);

        assert_eq!(
            status_battery_readback(&config, panic_read_battery, false),
            BatteryReadback::Unavailable
        );
    }

    #[test]
    fn status_reports_supported_battery_read_errors() {
        let config =
            selected_device_config("Bose QC Ultra 2 HP", domain::ModelId::QcUltraHeadphones2);

        assert_eq!(
            status_battery_readback(&config, fail_read_battery, false),
            BatteryReadback::Error("rfcomm failed".into())
        );
    }

    #[test]
    fn status_headphones_flag_forces_legacy_battery_readback() {
        let config =
            selected_device_config("Bose QuietComfort 45", domain::ModelId::QuietComfort45);

        assert_eq!(
            status_battery_readback(&config, read_battery_88, true),
            BatteryReadback::Level(88)
        );
    }

    #[test]
    fn sync_transaction_rolls_back_config_after_sync_failure() {
        reset_transaction_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut previous = ConfigFile::default();
        previous.selected_device = Some(domain::DeviceRef {
            address: "AA:BB".into(),
            name: Some("Headphones".into()),
            model: Some(domain::ModelId::QcUltraHeadphones2),
        });
        previous.save(&path).unwrap();

        let mut config = previous.clone();
        config.noise = domain::NoiseControl::new(true, 5).unwrap();
        config.sync_active_mode_to_current_audio();
        config.save(&path).unwrap();

        let err = sync_headphones_transaction(previous, &mut config, &path, fail_once_then_sync)
            .unwrap_err();

        let saved = ConfigFile::load_or_default(&path).unwrap();
        assert!(err.to_string().contains("rolled back"));
        assert_eq!(TRANSACTION_SYNC_CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(config.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(config.noise.level, 10);
        assert_eq!(saved.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(saved.noise.level, 10);
    }

    #[test]
    fn unsupported_selected_model_rejects_qc_ultra_2_config_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut config = ConfigFile::default();
        config.selected_device = Some(domain::DeviceRef {
            address: "AA:BB".into(),
            name: Some("Bose QuietComfort 45".into()),
            model: Some(domain::ModelId::QuietComfort45),
        });
        config.save(&path).unwrap();

        let err = handle_noise(
            NoiseCommand::Set {
                enabled: true,
                level: 5,
            },
            &mut config,
            &path,
        )
        .unwrap_err();

        let saved = ConfigFile::load_or_default(&path).unwrap();
        assert!(err.to_string().contains("does not support"));
        assert_eq!(config.noise.level, 10);
        assert_eq!(saved.noise.level, 10);
    }
}
