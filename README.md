# bose-cli

`bose-cli` is a terminal tool for discovering and controlling supported Bose
headphones without opening the Bose app.

It currently targets **Bose QuietComfort Ultra Headphones (2nd Gen)**, also seen
on macOS as `Bose QC Ultra 2 HP`. The CLI can save desired headphone settings in
`$HOME/.config/bosecli/config.toml` and sync supported settings to the headset over Bluetooth.

## Context

The project started as a small CLI/TUI replacement for common Bose app actions:

- choose the active Bose headset
- switch listening modes
- change noise cancellation
- change immersive audio
- set EQ bands
- read basic headset status

Bluetooth device discovery uses BLE/macOS Bluetooth listing, but configuration
writes use **BMAP over Bluetooth Classic RFCOMM**. The BMAP/RFCOMM protocol work
is inspired by [`aaronsb/bosectl`](https://github.com/aaronsb/bosectl). Detailed
protocol notes are in [`PROTOCOL.md`](PROTOCOL.md).

## Supported features

Current sync support:

- built-in modes: `Quiet`, `Aware`, `Immersion`, `Cinema`
- noise control: enabled/disabled plus level `0..10`
- immersive audio: `off`, `still`, `motion`
- EQ: bass/mid/treble, each `-10..10`
- battery readback with `status --headphones`
- interactive TUI for selecting a device and changing modes/noise/immersive

Not implemented yet:

- custom mode writes to the headset
- paired-device/multipoint management
- device rename, sidetone, auto-pause, auto-answer, pairing, routing, power
- full headset-state reconciliation back into `config.toml`

## Platform support

This repo currently implements headset writes on **macOS**.

Why:

- The headset control path is RFCOMM/SPP, not BLE.
- macOS CoreBluetooth is BLE-only.
- The project uses an `IOBluetooth` Objective-C bridge in `src/macos_rfcomm.m`
  to open RFCOMM channel `2` and exchange BMAP packets.

Bluetooth discovery can still use `btleplug`, but syncing settings requires the
macOS RFCOMM bridge today.

## Requirements

- Rust stable toolchain
- macOS with Command Line Tools installed (`xcode-select --install`)
- Bluetooth enabled
- the Bose headset paired with macOS
- optional: [`fzf`](https://github.com/junegunn/fzf) for `bose devices --select`

## Build and compile

Run checks:

```sh
cargo fmt -- --check
cargo test
cargo check
```

Build debug binary:

```sh
cargo build
```

Build optimized release binary:

```sh
cargo build --release
```

Run without installing:

```sh
cargo run -- doctor
```

Install locally from this checkout:

```sh
cargo install --path .
```

After install, the binary is named:

```sh
bose
```

## Configuration

The default config file is `$HOME/.config/bosecli/config.toml`.

Use another path with the global `--config` flag:

```sh
bose --config ~/custom-bose.toml status
```

Initialize a config file:

```sh
bose config init
```

Show config:

```sh
bose config show
```

Example `config.toml`:

```toml
active_mode = "Quiet"
custom_modes = []
immersive = "Off"

[selected_device]
address = "68:F2:1F:0D:FE:42"
name = "Bose QC Ultra 2 HP"

[noise]
enabled = true
level = 10

[eq]
bass = 0
mid = 0
treble = 0
```

## Usage

### Diagnose setup

```sh
bose doctor
```

Checks:

- config path/status
- `fzf` availability
- Bluetooth adapter availability

### List Bluetooth devices

```sh
bose devices
```

Use JSON output:

```sh
bose devices --json
```

Change scan duration:

```sh
bose devices --scan-seconds 10
```

Select a device with `fzf` and save it to config:

```sh
bose devices --select
```

### Show status

Show desired local state from `config.toml`:

```sh
bose status
```

Also read battery from the headphones:

```sh
bose status --headphones
```

### Sync saved config to headphones

```sh
bose sync
```

This pushes the saved mode/noise/immersive/EQ state to the selected headset.

When a selected headset is available, setting changes use a best-effort
transaction: the CLI snapshots the previous desired config in memory, saves the
requested change, and attempts headset sync. If sync fails, it restores the
previous config on disk and attempts to sync those previous desired settings back
to the headset. The headset state can still be unknown if rollback sync also
fails. Without a selected headset, changes are saved locally only.

### Modes

List known modes:

```sh
bose mode list
```

Switch to Quiet:

```sh
bose mode set Quiet
```

Switch to Aware:

```sh
bose mode set Aware
```

Switch to immersive/cinema presets:

```sh
bose mode set Immersion
bose mode set Cinema
```

Mode changes save config and attempt headset sync when you press Enter in the TUI.

### Noise control

Noise level uses this project’s domain scale:

- `0`: admits the most outside sound
- `10`: blocks the most outside sound

Set full ANC:

```sh
bose noise set --enabled true --level 10
```

Set maximum ambient/passthrough:

```sh
bose noise set --enabled true --level 0
```

Disable noise control:

```sh
bose noise set --enabled false --level 0
```

Show saved noise setting:

```sh
bose noise show
```

### Immersive audio

```sh
bose immersive set off
bose immersive set still
bose immersive set motion
```

Show saved immersive state:

```sh
bose immersive show
```

### EQ

Each band accepts `-10..10`:

```sh
bose eq set --bass 2 --mid 0 --treble -1
```

Show saved EQ:

```sh
bose eq show
```

### TUI

Open the interactive terminal UI:

```sh
bose tui
```

Controls:

- `Up`/`Down` or `k`/`j`: move selection; modes and immersive do not apply until Enter
- `Enter`: select sections; apply/reapply the selected mode or immersive setting
- `Space`: toggle noise enabled on the noise screen
- `Left`/`Right` or `-`/`+`: adjust noise level
- `Esc`/`Backspace`: go back
- `q` or `Ctrl-C`: quit

The TUI saves changes and syncs supported mode/noise/immersive changes to the
selected headset. If sync fails, it rolls the local config and UI controls back to
the previous desired settings and attempts a best-effort headset rollback.

## Development notes

Key files:

- `src/cli.rs`: command definitions and command handling
- `src/tui.rs`: Ratatui interface
- `src/bluetooth.rs`: Bluetooth device discovery/listing
- `src/bmap.rs`: BMAP packet handling and config sync
- `src/macos_rfcomm.m`: macOS `IOBluetooth` RFCOMM bridge
- `PROTOCOL.md`: protocol notes and packet reference

Run focused tests:

```sh
cargo test bmap
cargo test tui
```

There is also a hidden raw BMAP probe command for development:

```sh
BOSE_BMAP_RAW=1 cargo run -- bmap-raw <fblock> <func> <op> [payload_hex]
```

Example safe reads:

```sh
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 2 2 1
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 31 10 1
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 1 7 1
```

Do not expose `bmap-raw` as a normal user command; it can send arbitrary writes.

## Troubleshooting

If the headset does not appear:

```sh
bose devices --scan-seconds 10
system_profiler SPBluetoothDataType -json
```

If sync fails:

1. Confirm the headset is paired and connected in macOS Bluetooth settings.
2. Confirm the selected device in `config.toml` has the correct Bluetooth address.
3. Run `bose status --headphones` to verify RFCOMM/BMAP readback.
4. Run `bose sync` again after reconnecting the headset.

If `devices --select` fails, install `fzf` or select/configure the device
manually in `config.toml`.
