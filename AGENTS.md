# [AGENTS.md](http://AGENTS.md)

Guidance for future agents working on this repository.

## Project snapshot

- Rust package: `bose-cli`; installed binary: `bose`.
- Entrypoint: `src/main.rs`; command wiring: `src/cli.rs`.
- Primary modules:
  - `src/config.rs`: config path, persistence, TOML model.
  - `src/domain.rs`: modes, model IDs, noise control, immersive audio, EQ, device refs.
  - `src/models/`: model registry/API plus per-model capability gates and protocol implementations.
  - `src/bluetooth.rs`: Bluetooth discovery/listing.
  - `src/bmap.rs`: generic Bose BMAP packet building, response parsing, transport helpers.
  - `src/macos_rfcomm.m`: macOS RFCOMM bridge over `IOBluetooth`.
  - `src/tui.rs`: Ratatui/Crossterm interactive UI.
- Product intent lives in `specs/001-init.md`.
- User docs live in `README.md`.
- Bluetooth protocol notes live in `PROTOCOL.md`.

## Model support

- Only writable model profile today: **Bose QuietComfort Ultra Headphones (2nd Gen)**.
- macOS commonly reports it as `Bose QC Ultra 2 HP`.
- Manual/model reference: model `443987`.
- `PROTOCOL.md` notes `Bose QC Ultra 2 HP` / product ID `0x4082` / `bosectl` codename `wolverine`.
- Other known Bose models may be recognized by `src/models`, but they must remain non-writable until a model profile is implemented and hardware-verified.
- Never send QC Ultra 2 BMAP packets to unknown or unsupported selected models.

## Configuration behavior

- Default config path: `$HOME/.config/bosecli/config.toml`.
- All commands accept global `--config <path>`.
- `devices --select` shells out to the external `fzf` binary and should fail clearly if `fzf` is unavailable.
- The existing mode/noise/immersive/EQ config shape is the QC Ultra 2 desired-config profile.
- Commands that change mode/noise/immersive/EQ use a best-effort transaction when a supported selected headset exists: snapshot the pre-change desired config in memory, save the requested config, attempt headset sync, then restore the previous config and attempt headset rollback if sync fails.
- If no selected headset exists, CLI setting commands save local desired config only; no device transaction can start.
- If the selected headset is unknown or recognized-but-unsupported, reject mode/noise/immersive/EQ configuration before saving.
- Transaction rollback restores the previous desired config; it does not read confirmed headset state because mode/noise/immersive/EQ readback is not implemented.
- Sync and rollback failures should report clearly when headset state may be unknown.

## Bluetooth and protocol model

- Discovery/listing uses `btleplug` BLE and supplements devices reported by macOS.
- Headset configuration writes are **not BLE**. They use Bose BMAP over Bluetooth Classic RFCOMM/SPP.
- macOS writes go through `src/macos_rfcomm.m`, which opens the model profile’s RFCOMM channel with `IOBluetooth`.
- Do not rely on `/dev/cu.Bose...` serial devices; they are not a dependable control path for this headset.
- Central sync entrypoint: `src/models/mod.rs::sync_selected_config`, dispatched to the selected model profile.
- Existing QC Ultra 2 sync support:
  - built-in mode switch via `START [31.3]`
  - live noise/immersive settings via `SETGET [31.10]`
  - EQ band writes via `SETGET [1.7]`
  - battery readback for `status`
- BMAP response handling should validate complete frames, expected function block/function, expected operators, and `ERROR` frames.
- Plain `SET` writes are often auth-gated. Prefer verified unauthenticated `GET`, `START`, or `SETGET` paths unless hardware testing proves otherwise.
- Raw/probing behavior should stay guarded, for example with `BOSE_BMAP_RAW=1`, and still use a resolved model RFCOMM channel.

## TUI guidance

- `bose tui` scans Bluetooth devices before entering the alternate screen.
- TUI changes persist desired config and sync supported mode/noise/immersive changes to supported selected headsets.
- TUI should reject configuration screens for unknown or unsupported selected models.
- TUI sync failures roll local config and controls back to the pre-change desired settings, then attempt a best-effort headset rollback.
- Mode and immersive selections move with arrows/j/k and apply only when Enter is pressed; noise already applies on toggle/adjust.
- During save/sync actions, the footer shows a busy status and list screens use a spinner in place of the selection marker.
- The current visual direction is compact and fzf-like:
  - borderless layout
  - prompt-style header such as `bose > devices`
  - `> `selection marker
  - dim metadata/status/help lines
  - no heavy nested boxes or large empty panels
- Keep keybindings stable unless the user explicitly asks for interaction changes.
- Tests in `src/tui.rs` use injected sync hooks; avoid real hardware dependencies in unit tests.

## Domain invariants to preserve

- Listening modes are presets combining noise cancellation and immersive audio.
- Built-in modes currently include `Quiet`, `Aware`, `Immersion`, and `Cinema`.
- Custom mode data may be persisted locally, but custom mode writes to the headset are not implemented yet.
- Noise control is enabled/disabled plus level `0..10`:
  - `0` admits the most outside sound
  - `10` blocks the most outside sound
- Immersive audio values are `Off`, `Still`, and `Motion`; calls may temporarily disable immersive audio on the headset.
- EQ controls are `Bass`, `Mid`, and `Treble`, each `-10..+10`.
- Headset Bluetooth device list can store up to 6 devices.
- Multipoint can have up to 2 active connections; only one source plays audio at a time.

## Unsupported or incomplete areas

- Custom mode writes to the headset.
- Paired-device and multipoint management.
- Device rename, sidetone, auto-pause, auto-answer, pairing, routing, and power controls.
- Full headset-state reconciliation back into `config.toml`.
- Cross-platform RFCOMM write support outside macOS.
- Writable model profiles beyond `qc-ultra-headphones-2`.

## Verification

- Standard checks:
  - `cargo fmt -- --check`
  - `cargo test`
  - `cargo check`
- Focused Rust test: `cargo test <test_name>`.
- Prefer unit tests with fake sync hooks for config/TUI behavior.
- Hardware sync behavior may need manual verification with a paired headset; do not make tests require live Bluetooth hardware.

## Maintenance notes

- Keep `README.md`, `PROTOCOL.md`, and this file aligned when feature support changes.
- Be explicit about desired local state versus confirmed headset state.
- If adding protocol commands, implement them behind a `HeadphoneModel` profile and document the BMAP address/operator/payload and expected response in `PROTOCOL.md`.
- Avoid broad rewrites of protocol code without preserving response validation and clear error messages.