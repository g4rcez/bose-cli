use crate::{bluetooth::ScannedDevice, bmap, config::ConfigFile, domain, domain::DeviceRef};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{io, path::PathBuf, time::Duration};

type SyncHeadphones = fn(&ConfigFile) -> Result<()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Devices,
    Actions,
    Modes,
    Noise,
    Immersive,
    Connections,
}

#[derive(Debug, Clone)]
struct TuiDevice {
    reference: DeviceRef,
    rssi: Option<i16>,
    connected: Option<bool>,
    saved: bool,
    scanned: bool,
}

impl TuiDevice {
    fn saved(reference: DeviceRef) -> Self {
        Self {
            reference,
            rssi: None,
            connected: None,
            saved: true,
            scanned: false,
        }
    }

    fn scanned(device: ScannedDevice) -> Self {
        Self {
            reference: DeviceRef {
                address: device.address,
                name: device.name,
            },
            rssi: device.rssi,
            connected: Some(device.connected),
            saved: false,
            scanned: true,
        }
    }

    fn merge_scan(&mut self, device: ScannedDevice) {
        self.reference.name = self.reference.name.clone().or(device.name);
        self.rssi = device.rssi;
        self.connected = Some(device.connected);
        self.scanned = true;
    }
}

fn accent_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn dim_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn bold_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn screen_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Devices => "devices",
        Screen::Actions => "actions",
        Screen::Modes => "modes",
        Screen::Noise => "noise",
        Screen::Immersive => "immersive",
        Screen::Connections => "connections",
    }
}

fn help_text(screen: Screen) -> &'static str {
    match screen {
        Screen::Noise => {
            "↑↓/jk move  space toggle  ←→ or -/+ adjust  esc/backspace back  q/Ctrl-C quit"
        }
        Screen::Modes | Screen::Immersive => {
            "↑↓/jk move  enter apply/reapply  esc/backspace back  q/Ctrl-C quit"
        }
        _ => "↑↓/jk move  enter open/apply  esc/backspace back  q/Ctrl-C quit",
    }
}

fn header_line(screen: Screen) -> Line<'static> {
    Line::from(vec![
        Span::styled("bose", accent_style()),
        Span::styled("  > ", dim_style()),
        Span::styled(screen_label(screen), bold_style()),
    ])
}

fn status_line(message: &str, saved: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(if saved { "saved" } else { "status" }, dim_style()),
        Span::raw("  "),
        Span::raw(message.to_owned()),
    ])
}

fn device_item(device: &TuiDevice) -> ListItem<'static> {
    let name = device
        .reference
        .name
        .clone()
        .unwrap_or_else(|| "<unknown>".into());
    let mut meta: Vec<String> = Vec::new();
    if device.saved {
        meta.push("saved".into());
    }
    if device.scanned {
        meta.push("scanned".into());
    }
    if device.connected == Some(true) {
        meta.push("connected".into());
    } else if device.connected == Some(false) {
        meta.push("discovered".into());
    }
    if let Some(rssi) = device.rssi {
        meta.push(format!("{rssi} dBm"));
    }

    let mut spans = vec![Span::styled(name, bold_style())];
    if !meta.is_empty() {
        spans.push(Span::styled(format!("  {}", meta.join(" · ")), dim_style()));
    }
    ListItem::new(Line::from(spans))
}

fn mode_item(mode: &domain::ModePreset, active: bool) -> ListItem<'static> {
    let mut spans = vec![Span::styled(
        mode.name.clone(),
        if active { accent_style() } else { bold_style() },
    )];
    let mut meta = vec![format!(
        "noise {} {}",
        if mode.noise.enabled { "on" } else { "off" },
        mode.noise.level
    )];
    meta.push(format!("immersive {}", mode.immersive));
    if active {
        meta.push("active".into());
    }
    spans.push(Span::styled(format!("  {}", meta.join(" · ")), dim_style()));
    ListItem::new(Line::from(spans))
}

fn action_item(label: &str) -> ListItem<'static> {
    ListItem::new(Line::from(Span::styled(label.to_owned(), bold_style())))
}

fn immersive_item(label: &str, active: bool) -> ListItem<'static> {
    let mut spans = vec![Span::styled(
        label.to_owned(),
        if active { accent_style() } else { bold_style() },
    )];
    if active {
        spans.push(Span::styled("  active", dim_style()));
    }
    ListItem::new(Line::from(spans))
}

pub struct App {
    pub config: ConfigFile,
    path: PathBuf,
    devices: Vec<TuiDevice>,
    screen: Screen,
    selected: usize,
    message: String,
    saved: bool,
    noise_level: u8,
    noise_enabled: bool,
    scan_status: String,
    sync_headphones: SyncHeadphones,
}

impl App {
    pub fn new(
        config: ConfigFile,
        path: PathBuf,
        scanned_devices: Vec<ScannedDevice>,
        scan_status: Option<String>,
    ) -> Self {
        Self::new_with_sync(
            config,
            path,
            scanned_devices,
            scan_status,
            bmap::sync_config,
        )
    }

    fn new_with_sync(
        config: ConfigFile,
        path: PathBuf,
        scanned_devices: Vec<ScannedDevice>,
        scan_status: Option<String>,
        sync_headphones: SyncHeadphones,
    ) -> Self {
        let mut devices = Vec::new();
        if let Some(selected) = config.selected_device.clone() {
            devices.push(TuiDevice::saved(selected));
        }

        let scanned_count = scanned_devices.len();
        for device in scanned_devices {
            if let Some(existing) = devices
                .iter_mut()
                .find(|known| known.reference.address == device.address)
            {
                existing.merge_scan(device);
            } else {
                devices.push(TuiDevice::scanned(device));
            }
        }

        let scan_status = scan_status.unwrap_or_else(|| {
            if scanned_count == 0 {
                "No Bluetooth devices found; select a saved device or quit.".into()
            } else {
                format!("Found {scanned_count} Bluetooth device(s).")
            }
        });

        Self {
            noise_level: config.noise.level,
            noise_enabled: config.noise.enabled,
            config,
            path,
            devices,
            sync_headphones,
            screen: Screen::Devices,
            selected: 0,
            message: scan_status.clone(),
            saved: false,
            scan_status,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return true;
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc | KeyCode::Backspace => self.back(),
            KeyCode::Up | KeyCode::Char('k') => self.up(),
            KeyCode::Down | KeyCode::Char('j') => self.down(),
            KeyCode::Enter => self.enter(),
            KeyCode::Char(' ') => self.space(),
            KeyCode::Left | KeyCode::Char('-') => self.adjust_noise(false),
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_noise(true),
            _ => {}
        }
        false
    }

    fn current_items_len(&self) -> usize {
        match self.screen {
            Screen::Devices => self.devices.len().max(1),
            Screen::Actions => 4,
            Screen::Modes => self.config.all_modes().len(),
            Screen::Noise => 1,
            Screen::Immersive => 3,
            Screen::Connections => 1,
        }
    }

    fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn down(&mut self) {
        let len = self.current_items_len();
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    fn back(&mut self) {
        self.screen = match self.screen {
            Screen::Devices => Screen::Devices,
            Screen::Actions => Screen::Devices,
            _ => Screen::Actions,
        };
        self.selected = 0;
    }

    fn enter(&mut self) {
        match self.screen {
            Screen::Devices => {
                if let Some(device) = self.devices.get(self.selected).cloned() {
                    let previous = self.config.clone();
                    self.config.selected_device = Some(device.reference);
                    if self
                        .commit_config(previous, String::from("Selected device saved to config."))
                    {
                        self.screen = Screen::Actions;
                        self.selected = 0;
                    }
                } else {
                    self.message = String::from("No devices available; scan again or quit.");
                }
            }
            Screen::Actions => {
                self.screen = match self.selected {
                    0 => Screen::Modes,
                    1 => Screen::Noise,
                    2 => Screen::Immersive,
                    _ => Screen::Connections,
                };
                self.selected = match self.screen {
                    Screen::Modes => self
                        .config
                        .active_mode
                        .as_ref()
                        .and_then(|active| {
                            self.config
                                .all_modes()
                                .iter()
                                .position(|mode| mode.name.eq_ignore_ascii_case(active))
                        })
                        .unwrap_or(0),
                    Screen::Immersive => match self.config.immersive {
                        domain::ImmersiveAudio::Off => 0,
                        domain::ImmersiveAudio::Still => 1,
                        domain::ImmersiveAudio::Motion => 2,
                    },
                    _ => 0,
                };
            }
            Screen::Modes => {
                self.apply_selected_mode();
            }
            Screen::Immersive => self.apply_selected_immersive(),
            Screen::Noise | Screen::Connections => {}
        }
    }

    fn apply_selected_mode(&mut self) {
        if let Some(mode) = self.config.all_modes().get(self.selected).cloned() {
            let previous = self.config.clone();
            self.config.active_mode = Some(mode.name.clone());
            self.config.noise = mode.noise;
            self.config.immersive = mode.immersive;
            self.sync_controls_from_config();
            self.commit_config_and_sync(previous, format!("Saved mode: {}", mode.name));
        }
    }

    fn apply_selected_immersive(&mut self) {
        let previous = self.config.clone();
        self.config.immersive = match self.selected {
            0 => domain::ImmersiveAudio::Off,
            1 => domain::ImmersiveAudio::Still,
            _ => domain::ImmersiveAudio::Motion,
        };
        self.config.sync_active_mode_to_current_audio();
        self.commit_config_and_sync(
            previous,
            format!("Saved immersive audio: {}", self.config.immersive),
        );
    }

    fn space(&mut self) {
        if matches!(self.screen, Screen::Noise) {
            self.noise_enabled = !self.noise_enabled;
            self.persist_noise();
        }
    }

    fn adjust_noise(&mut self, increase: bool) {
        if matches!(self.screen, Screen::Noise) {
            self.noise_level = if increase {
                self.noise_level.saturating_add(1)
            } else {
                self.noise_level.saturating_sub(1)
            }
            .min(10);
            self.persist_noise();
        }
    }

    fn persist_noise(&mut self) {
        if let Ok(noise) = domain::NoiseControl::new(self.noise_enabled, self.noise_level) {
            let previous = self.config.clone();
            self.config.noise = noise;
            self.config.sync_active_mode_to_current_audio();
            self.commit_config_and_sync(
                previous,
                format!(
                    "Saved noise: enabled={} level={}",
                    self.noise_enabled, self.noise_level
                ),
            );
        }
    }

    fn commit_config_and_sync(&mut self, previous: ConfigFile, success_message: String) -> bool {
        if !self.commit_config(previous.clone(), success_message.clone()) {
            return false;
        }

        if self.config.selected_device.is_none() {
            self.message = format!("{success_message}; saved locally (no selected device).");
            return true;
        }

        match (self.sync_headphones)(&self.config) {
            Ok(()) => {
                self.message = format!("{success_message}; synced headphones.");
                true
            }
            Err(err) => {
                self.config = previous;
                self.sync_controls_from_config();
                self.sync_selection_from_config();

                let rollback_save = self.config.save(&self.path);
                let rollback_sync = (self.sync_headphones)(&self.config);
                self.saved = rollback_save.is_ok();

                self.message = match (rollback_save, rollback_sync) {
                    (Ok(()), Ok(())) => {
                        format!("Sync failed: {err}; rolled back to previous settings.")
                    }
                    (Ok(()), Err(rollback_err)) => format!(
                        "Sync failed: {err}; local rollback saved, but headset rollback failed: {rollback_err}; headset state unknown."
                    ),
                    (Err(save_err), Ok(())) => format!(
                        "Sync failed: {err}; rollback save failed: {save_err}; synced previous settings to headphones."
                    ),
                    (Err(save_err), Err(rollback_err)) => format!(
                        "Sync failed: {err}; rollback save failed: {save_err}; headset rollback failed: {rollback_err}; headset state unknown."
                    ),
                };
                false
            }
        }
    }

    fn commit_config(&mut self, previous: ConfigFile, success_message: String) -> bool {
        match self.config.save(&self.path) {
            Ok(()) => {
                self.saved = true;
                self.message = success_message;
                true
            }
            Err(err) => {
                self.config = previous;
                self.sync_controls_from_config();
                self.saved = false;
                self.message = format!("Save failed: {err}");
                false
            }
        }
    }

    fn sync_controls_from_config(&mut self) {
        self.noise_enabled = self.config.noise.enabled;
        self.noise_level = self.config.noise.level;
    }

    fn sync_selection_from_config(&mut self) {
        self.selected = match self.screen {
            Screen::Modes => self
                .config
                .active_mode
                .as_ref()
                .and_then(|active| {
                    self.config
                        .all_modes()
                        .iter()
                        .position(|mode| mode.name.eq_ignore_ascii_case(active))
                })
                .unwrap_or(0),
            Screen::Immersive => match self.config.immersive {
                domain::ImmersiveAudio::Off => 0,
                domain::ImmersiveAudio::Still => 1,
                domain::ImmersiveAudio::Motion => 2,
            },
            _ => self.selected,
        };
    }
}

pub async fn run(
    config: ConfigFile,
    path: PathBuf,
    scanned_devices: Vec<ScannedDevice>,
    scan_status: Option<String>,
) -> Result<()> {
    let mut app = App::new(config, path, scanned_devices, scan_status);
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    run_terminal(&mut terminal, &mut app)
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn run_terminal<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    terminal.clear()?;
    loop {
        terminal.draw(|f| render(f, app))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.on_key(key) {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());
    f.render_widget(Paragraph::new(header_line(app.screen)), chunks[0]);
    match app.screen {
        Screen::Devices => render_list(
            f,
            chunks[1],
            app,
            app.devices.iter().map(device_item).collect(),
            app.scan_status.as_str(),
        ),
        Screen::Actions => render_list(
            f,
            chunks[1],
            app,
            vec![
                action_item("Modes"),
                action_item("Noise control"),
                action_item("Immersive Audio"),
                action_item("Connections"),
            ],
            "Choose a section.",
        ),
        Screen::Modes => render_list(
            f,
            chunks[1],
            app,
            app.config
                .all_modes()
                .into_iter()
                .map(|mode| {
                    let active = app
                        .config
                        .active_mode
                        .as_deref()
                        .is_some_and(|active| active.eq_ignore_ascii_case(&mode.name));
                    mode_item(&mode, active)
                })
                .collect(),
            "Enter applies the selected mode and syncs supported headphones.",
        ),
        Screen::Noise => {
            let text = vec![
                Line::from(vec![
                    Span::styled("enabled", dim_style()),
                    Span::raw("  "),
                    Span::styled(if app.noise_enabled { "on" } else { "off" }, bold_style()),
                ]),
                Line::from(vec![
                    Span::styled("level", dim_style()),
                    Span::raw("  "),
                    Span::styled(format!("{}/10", app.noise_level), bold_style()),
                ]),
                Line::from(vec![
                    Span::styled("control", dim_style()),
                    Span::raw("  "),
                    Span::raw("space toggle  ←→ or -/+ adjust"),
                ]),
            ];
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), chunks[1]);
        }
        Screen::Immersive => render_list(
            f,
            chunks[1],
            app,
            ["Off", "Still", "Motion"]
                .into_iter()
                .map(|label| {
                    let active = app.config.immersive.to_string().eq_ignore_ascii_case(label);
                    immersive_item(label, active)
                })
                .collect(),
            "Enter applies the selected immersive setting and syncs supported headphones.",
        ),
        Screen::Connections => {
            let conn_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(chunks[1]);
            let selected = app
                .config
                .selected_device
                .as_ref()
                .map(|d| d.display())
                .unwrap_or_else(|| "<none>".into());
            let summary = vec![
                Line::from(vec![
                    Span::styled("selected", dim_style()),
                    Span::raw("  "),
                    Span::styled(selected, bold_style()),
                ]),
                Line::from(vec![
                    Span::styled("scan", dim_style()),
                    Span::raw("  "),
                    Span::raw(app.scan_status.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("sync", dim_style()),
                    Span::raw("  "),
                    Span::raw("mode/noise/immersive apply via BMAP when supported"),
                ]),
            ];
            f.render_widget(
                Paragraph::new(summary).wrap(Wrap { trim: true }),
                conn_chunks[0],
            );
            render_list(
                f,
                conn_chunks[1],
                app,
                if app.devices.is_empty() {
                    Vec::new()
                } else {
                    app.devices.iter().map(device_item).collect()
                },
                "No known devices.",
            );
        }
    }
    f.render_widget(
        Paragraph::new(status_line(&app.message, app.saved)),
        chunks[2],
    );
    f.render_widget(
        Paragraph::new(help_text(app.screen)).style(dim_style()),
        chunks[3],
    );
}

fn render_list(f: &mut Frame, area: Rect, app: &App, items: Vec<ListItem<'static>>, empty: &str) {
    if items.is_empty() {
        f.render_widget(Paragraph::new(empty).style(dim_style()), area);
        return;
    }
    let len = items.len();
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default();
    state.select(Some(app.selected.min(len.saturating_sub(1))));
    f.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NOISE_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static MODE_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static MODE_NAV_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static IMMERSIVE_NAV_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static MODE_ROLLBACK_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static NOISE_ROLLBACK_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static NO_SELECTED_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn config_with_device() -> ConfigFile {
        let mut config = ConfigFile::default();
        config.selected_device = Some(DeviceRef {
            address: "AA:BB".into(),
            name: Some("Headphones".into()),
        });
        config
    }

    fn fake_noise_sync(_: &ConfigFile) -> Result<()> {
        NOISE_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn fake_mode_sync(_: &ConfigFile) -> Result<()> {
        MODE_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn fake_mode_nav_sync(_: &ConfigFile) -> Result<()> {
        MODE_NAV_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn fake_immersive_nav_sync(_: &ConfigFile) -> Result<()> {
        IMMERSIVE_NAV_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn fake_mode_sync_fails_once(_: &ConfigFile) -> Result<()> {
        let call = MODE_ROLLBACK_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            anyhow::bail!("mode sync failed")
        }
        Ok(())
    }

    fn fake_noise_sync_fails_once(_: &ConfigFile) -> Result<()> {
        let call = NOISE_ROLLBACK_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            anyhow::bail!("noise sync failed")
        }
        Ok(())
    }

    fn fake_no_selected_sync(_: &ConfigFile) -> Result<()> {
        NO_SELECTED_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn reset_noise_sync_calls() {
        NOISE_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_mode_sync_calls() {
        MODE_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_mode_nav_sync_calls() {
        MODE_NAV_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_immersive_nav_sync_calls() {
        IMMERSIVE_NAV_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_mode_rollback_sync_calls() {
        MODE_ROLLBACK_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_noise_rollback_sync_calls() {
        NOISE_ROLLBACK_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn reset_no_selected_sync_calls() {
        NO_SELECTED_SYNC_CALLS.store(0, Ordering::SeqCst);
    }

    fn buffer_text(buffer: &Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                line.push_str(buffer[(x, y)].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    #[test]
    fn saves_device_selection_and_moves_to_actions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let config = ConfigFile::default();
        let mut app = App::new(
            config,
            path,
            vec![ScannedDevice {
                name: Some("Headphones".into()),
                address: "AA:BB".into(),
                rssi: Some(-42),
                connected: false,
            }],
            None,
        );
        app.enter();
        assert!(matches!(app.screen, Screen::Actions));
        assert_eq!(
            app.config.selected_device.as_ref().unwrap().address,
            "AA:BB"
        );
        assert_eq!(app.devices[0].reference.display(), "Headphones (AA:BB)");
        assert_eq!(app.devices[0].rssi, Some(-42));
    }

    #[test]
    fn device_selection_does_not_advance_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file_parent = dir.path().join("not-a-directory");
        std::fs::write(&file_parent, "file blocks directory creation").unwrap();
        let path = file_parent.join("bose.toml");
        let mut app = App::new(
            ConfigFile::default(),
            path,
            vec![ScannedDevice {
                name: Some("Headphones".into()),
                address: "AA:BB".into(),
                rssi: None,
                connected: false,
            }],
            None,
        );

        app.enter();

        assert!(matches!(app.screen, Screen::Devices));
        assert!(app.config.selected_device.is_none());
        assert!(app.message.starts_with("Save failed:"));
    }

    #[test]
    fn ctrl_c_quits() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(
            ConfigFile::default(),
            dir.path().join("bose.toml"),
            vec![],
            None,
        );

        assert!(app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
    }

    #[test]
    fn noise_change_saves_and_clears_diverging_active_mode() {
        reset_noise_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(config_with_device(), path, vec![], None, fake_noise_sync);
        app.screen = Screen::Noise;

        app.adjust_noise(false);

        assert_eq!(app.config.noise.level, 9);
        assert_eq!(app.config.active_mode, None);
        assert_eq!(NOISE_SYNC_CALLS.load(Ordering::SeqCst), 1);
        assert!(app.message.contains("synced headphones"));
    }

    #[test]
    fn mode_selection_saves_and_syncs_headphones() {
        reset_mode_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(config_with_device(), path, vec![], None, fake_mode_sync);
        app.screen = Screen::Modes;
        app.selected = 0;

        app.enter();

        assert_eq!(app.config.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(MODE_SYNC_CALLS.load(Ordering::SeqCst), 1);
        assert!(app.message.contains("synced headphones"));
    }

    #[test]
    fn mode_selection_moves_without_applying_until_enter() {
        reset_mode_nav_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app =
            App::new_with_sync(config_with_device(), path, vec![], None, fake_mode_nav_sync);
        app.screen = Screen::Modes;
        app.selected = 0;

        app.down();

        assert_eq!(app.selected, 1);
        assert_eq!(app.config.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(app.config.noise.level, 10);
        assert_eq!(MODE_NAV_SYNC_CALLS.load(Ordering::SeqCst), 0);

        app.enter();

        assert_eq!(app.config.active_mode.as_deref(), Some("Aware"));
        assert_eq!(app.config.noise.level, 0);
        assert_eq!(MODE_NAV_SYNC_CALLS.load(Ordering::SeqCst), 1);
        assert!(app.message.contains("synced headphones"));
    }

    #[test]
    fn immersive_selection_moves_without_applying_until_enter() {
        reset_immersive_nav_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(
            config_with_device(),
            path,
            vec![],
            None,
            fake_immersive_nav_sync,
        );
        app.screen = Screen::Immersive;
        app.selected = 0;

        app.down();

        assert_eq!(app.selected, 1);
        assert_eq!(app.config.immersive, domain::ImmersiveAudio::Off);
        assert_eq!(app.config.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(IMMERSIVE_NAV_SYNC_CALLS.load(Ordering::SeqCst), 0);

        app.enter();

        assert_eq!(app.config.immersive, domain::ImmersiveAudio::Still);
        assert_eq!(app.config.active_mode.as_deref(), Some("Cinema"));
        assert_eq!(IMMERSIVE_NAV_SYNC_CALLS.load(Ordering::SeqCst), 1);
        assert!(app.message.contains("synced headphones"));
    }

    #[test]
    fn mode_selection_rolls_back_when_sync_fails() {
        reset_mode_rollback_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(
            config_with_device(),
            path.clone(),
            vec![],
            None,
            fake_mode_sync_fails_once,
        );
        app.screen = Screen::Modes;
        app.selected = 1;

        app.enter();

        let saved = ConfigFile::load_or_default(&path).unwrap();
        assert_eq!(app.config.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(app.config.noise.level, 10);
        assert_eq!(app.selected, 0);
        assert_eq!(saved.active_mode.as_deref(), Some("Quiet"));
        assert_eq!(saved.noise.level, 10);
        assert_eq!(MODE_ROLLBACK_SYNC_CALLS.load(Ordering::SeqCst), 2);
        assert!(app.message.contains("rolled back"));
    }

    #[test]
    fn noise_change_rolls_back_when_sync_fails() {
        reset_noise_rollback_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(
            config_with_device(),
            path.clone(),
            vec![],
            None,
            fake_noise_sync_fails_once,
        );
        app.screen = Screen::Noise;

        app.adjust_noise(false);

        let saved = ConfigFile::load_or_default(&path).unwrap();
        assert_eq!(app.config.noise.level, 10);
        assert_eq!(app.noise_level, 10);
        assert_eq!(saved.noise.level, 10);
        assert_eq!(NOISE_ROLLBACK_SYNC_CALLS.load(Ordering::SeqCst), 2);
        assert!(app.message.contains("rolled back"));
    }

    #[test]
    fn no_selected_device_saves_tui_change_locally_without_sync() {
        reset_no_selected_sync_calls();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bose.toml");
        let mut app = App::new_with_sync(
            ConfigFile::default(),
            path.clone(),
            vec![],
            None,
            fake_no_selected_sync,
        );
        app.screen = Screen::Noise;

        app.adjust_noise(false);

        let saved = ConfigFile::load_or_default(&path).unwrap();
        assert_eq!(app.config.noise.level, 9);
        assert_eq!(app.noise_level, 9);
        assert_eq!(app.config.active_mode, None);
        assert_eq!(saved.noise.level, 9);
        assert_eq!(NO_SELECTED_SYNC_CALLS.load(Ordering::SeqCst), 0);
        assert!(app.message.contains("saved locally"));
    }

    #[test]
    fn renders_with_test_backend() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut config = ConfigFile::default();
        config.selected_device = Some(DeviceRef {
            address: "AA:BB".into(),
            name: Some("Headphones".into()),
        });
        let app = App::new(
            config,
            dir.path().join("bose.toml"),
            vec![ScannedDevice {
                name: Some("Headphones".into()),
                address: "AA:BB".into(),
                rssi: Some(-42),
                connected: true,
            }],
            None,
        );
        terminal.draw(|f| render(f, &app)).unwrap();
        let rendered = buffer_text(terminal.backend_mut().buffer());

        assert!(rendered.contains("bose"));
        assert!(rendered.contains("devices"));
        assert!(rendered.contains("Headphones"));
        assert!(rendered.contains("saved"));
        assert!(!rendered.contains('┌'));
        assert!(!rendered.contains('│'));
        assert!(!rendered.contains('─'));
    }
}
