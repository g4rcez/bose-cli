# Bose Bluetooth Protocol

This document explains the Bose control protocol used by this CLI, how it is
implemented here, and how to add new Bluetooth configuration writes safely.

The protocol implementation is inspired by the reverse-engineered
`aaronsb/bosectl` repository, especially its protocol notes and BMAP
implementations:

- `NOTES.md`
- `rust/src/protocol.rs`
- `rust/src/devices.rs`
- `rust/src/connection.rs`
- `python/pybmap/protocol.py`
- `python/pybmap/devices/qc_ultra2.py`
- `python/pybmap/devices/parsers.py`

## Scope

The current writable model profile is **Bose QC Ultra 2 HP** / Bose QuietComfort
Ultra Headphones 2, product ID `0x4082`, codename `wolverine` in `bosectl`.

The code has a model registry in `src/models/`. Known-but-unsupported models can
be detected and selected, but the CLI refuses configuration writes unless that
model implements the `HeadphoneModel` API. This avoids sending QC Ultra 2 BMAP
packets to unverified hardware.

The CLI currently syncs:

- built-in listening mode: `Quiet`, `Aware`, `Immersion`, `Cinema`
- live noise cancellation / ANC toggle
- live immersive audio mode
- EQ: bass, mid, treble
- battery readback for `status`

Custom mode creation is persisted locally but is not yet written to the headset.

## Transport

Bose settings are not written through BLE scanning. BLE is only used by this CLI
for discovery/listing.

Control traffic uses **BMAP over Bluetooth Classic RFCOMM/SPP**:

- transport: Bluetooth SPP/RFCOMM
- QC Ultra 2 channel: `2`
- BMAP service UUID seen in `bosectl`: `00000000-deca-fade-deca-deafdecacaff`

On macOS, CoreBluetooth is BLE-only and cannot open RFCOMM. This repo uses a
native `IOBluetooth` bridge:

- `build.rs` compiles `src/macos_rfcomm.m` on macOS targets.
- `src/macos_rfcomm.m` opens `IOBluetoothDevice` by Bluetooth address, opens the
  model profile’s RFCOMM channel, writes a BMAP packet, waits for complete BMAP
  frames, and returns bytes to Rust over FFI.
- `src/bmap.rs` builds/parses BMAP frames and validates responses.
- `src/models/qc_ultra_2.rs` contains the verified QC Ultra 2 BMAP addresses,payload encoders, sync sequence, and battery readback.

Do not rely on `/dev/cu.Bose...` serial devices for BMAP. They can exist but arenot a reliable control path for this headset.

## BMAP frame format

Every packet is:

```text
[fblock_id, function_id, flags, payload_length, ...payload]
```

`flags` stores routing bits plus the operator in the low nibble:

```text
(device_id << 6) | (port_num << 4) | (operator & 0x0f)
```

The CLI currently writes flags as just the operator. When parsing, it masks with
`0x0f` so responses like `0x46` still parse as `RESULT` (`6`).

Payload length is one byte, so one BMAP frame can carry at most 255 payload
bytes.

## Operators

| Value | Name | Meaning |
| --- | --- | --- |
| `0` | `SET` | write-only; often auth-gated |
| `1` | `GET` | read |
| `2` | `SETGET` | write and return current/updated value |
| `3` | `STATUS` | status response/update |
| `4` | `ERROR` | error response |
| `5` | `START` | action/command trigger |
| `6` | `RESULT` | command result |
| `7` | `PROCESSING` | long-running command started |

Known error codes:

| Code | Meaning |
| --- | --- |
| `1` | length error |
| `2` | checksum error |
| `3` | function block unsupported |
| `4` | function unsupported |
| `5` | operator unsupported or authentication required |
| `6` | invalid data |
| `7` | data unavailable |
| `8` | runtime error |
| `9` | timeout |
| `10` | invalid state |
| `15` | invalid transition |
| `20` | insecure transport |

## Authentication boundary

The reference repo found that Bose auth/cloud ECDH gates many `SET` writes, butnot all useful operations.

Unauthenticated operations used by this CLI:

- `GET` reads across known function blocks.
- `START [31.3]` switches the current audio mode.
- `SETGET [31.10]` writes live audio settings.
- `SETGET [1.7]` writes EQ bands.

Avoid implementing plain `SET` writes unless you have verified they are acceptedwithout auth. Error `5` usually means the write path is auth-gated.

## QC Ultra 2 function map

Important function block addresses from `aaronsb/bosectl`:

| Address | Name | Current CLI use |
| --- | --- | --- |
| `[0.5]` | firmware version | not yet exposed |
| `[1.2]` | product/device name | not yet exposed |
| `[1.3]` | voice prompts/language | not yet exposed |
| `[1.5]` | CNC read path | not used for live writes |
| `[1.7]` | EQ/range | write bass/mid/treble |
| `[1.9]` | buttons | not yet exposed |
| `[1.10]` | multipoint toggle | not yet exposed |
| `[1.11]` | sidetone | not yet exposed |
| `[1.24]` | auto pause | not yet exposed |
| `[1.27]` | auto answer | not yet exposed |
| `[2.2]` | battery | `status` |
| `[4.8]` | pairing mode | not yet exposed |
| `[4.12]` | routing / active multipoint source | not yet exposed |
| `[5.1]` | active audio source | not yet exposed |
| `[7.4]` | power | not yet exposed |
| `[31.1]` | all modes dump | not yet exposed |
| `[31.3]` | current mode | mode switching |
| `[31.6]` | mode config | future custom modes |
| `[31.8]` | favorites | not yet exposed |
| `[31.10]` | live audio settings | noise/immersive/ANC |

## Configuration writes implemented in this CLI

The central implementation is `src/models/qc_ultra_2.rs::sync_config`, reachedthrough the model registry wrapper `src/models/mod.rs::sync_selected_config`.`src/bmap.rs` is now generic frame/transport plumbing.

It requires `ConfigFile.selected_device.address` and a selected model resolved as`qc-ultra-headphones-2`, then sends packets in this order:

1. mode switch with `START [31.3]`, if `active_mode` is one of the built-ins
2. live noise/immersive settings with `SETGET [31.10]`
3. three EQ band writes with `SETGET [1.7]`

Short sleeps are used between commands because the headset can timeout if writesare sent back-to-back immediately after a mode switch.

### Listening mode

Packet:

```text
[31, 3, START, 2, MODE_INDEX, VOICE_PROMPT]
```

Current mode indexes:

| CLI mode | Index | Meaning |
| --- | --- | --- |
| `Quiet` | `0` | full ANC |
| `Aware` | `1` | passthrough/transparency |
| `Immersion` | `2` | spatial audio immersive/head tracking |
| `Cinema` | `3` | spatial audio fixed/still stage |

`VOICE_PROMPT` is currently `0` for silent switching.

Expected response: `RESULT` or `STATUS` for `[31.3]`. Live verification for
Quiet readback returns:

```text
[31.3] Status: 00
```

### Noise cancellation and immersive audio

Live audio settings use `[31.10]` with `SETGET`.

Payload:

```text
[cnc, auto_cnc, spatial, wind, anc]
```

Fields:

| Byte | Field | Values |
| --- | --- | --- |
| `0` | `cnc` | `0..10`, where `0=max ANC`, `10=most ambient` |
| `1` | `auto_cnc` | must be `0`; `1` is rejected on reference firmware |
| `2` | `spatial` | `0=off`, `1=room/still`, `2=head/motion` |
| `3` | `wind` | `0=off`, `1=on` |
| `4` | `anc` | `0=off`, `1=on` |

The project domain model uses the opposite noise scale:

- CLI/domain `noise.level = 0`: admits the most outside sound
- CLI/domain `noise.level = 10`: blocks the most outside sound
- Bose BMAP `cnc = 0`: maximum ANC
- Bose BMAP `cnc = 10`: most ambient

So the mapping is:

```text
cnc = 10 - config.noise.level
anc = config.noise.enabled ? 1 : 0
auto_cnc = 0
wind = 0
```

Immersive mapping:

| CLI/domain | BMAP spatial |
| --- | --- |
| `Off` | `0` |
| `Still` | `1` |
| `Motion` | `2` |

Example for local config `noise.enabled=true`, `noise.level=0`, `immersive=Off`:

```text
SETGET [31.10] payload = 0a00000001
```

Readback:

```text
[31.10] Status: 0a00000001
```

### EQ

EQ uses Settings block `[1.7]` with `SETGET`.

Each band is written separately:

```text
[value, band_id]
```

Band IDs:

| Band | ID |
| --- | --- |
| Bass | `0` |
| Mid | `1` |
| Treble | `2` |

`value` is signed `-10..+10`, encoded as one byte. Rust casts `i8` to `u8`, so
negative values use two's complement (`-10` becomes `0xf6`).

GET `[1.7]` returns three 4-byte groups:

```text
[min, max, current, band_id] * 3
```

Example readback for all bands at zero:

```text
[1.7] Status: f60a0000 f60a0001 f60a0002
```

That means each band has min `0xf6` (`-10`), max `0x0a` (`+10`), current `0`,
and band IDs `0`, `1`, `2`.

### Battery

Battery is a read-only `GET [2.2]`.

The first payload byte is the percentage. Example:

```text
[2.2] Status: 5affff00
```

`0x5a = 90`, so the battery is 90%.

## Response handling

RFCOMM can return partial frames or multiple concatenated BMAP frames. The macOS
bridge waits until complete BMAP frames have arrived before returning bytes to
Rust. Rust then calls `parse_frames` and validates:

- the response has complete frames
- at least one frame matches the requested `fblock` and `function`
- no matching frame is `ERROR`
- the matching frame operator is expected (`STATUS` or `RESULT` for currentsync writes)

This avoids treating unrelated async frames or partial RFCOMM chunks as success.

## CLI integration

### Selecting the device

`bose devices --select` saves the selected Bluetooth address/name/model into
`config.toml`:

```toml
[selected_device]
address = "68:F2:1F:0D:FE:42"
name = "Bose QC Ultra 2 HP"
```

BMAP sync uses `selected_device.address` to open RFCOMM.

### Explicit sync

Run:

```sh
cargo run -- sync
```

or after installing:

```sh
bose sync
```

This loads `$HOME/.config/bosecli/config.toml` unless `--config` is passed, thenresolves the selected model and calls its `HeadphoneModel::sync_config` method.Unknown or unsupported selected models are rejected before opening RFCOMM.

### Automatic sync after CLI changes

For supported selected models, these commands save config locally, then attemptheadphone sync:

```sh
bose mode set Quiet
bose noise set --enabled true --level 10
bose immersive set motion
bose eq set --bass 2 --mid 0 --treble -1
```

If sync fails, the CLI restores the previous desired config and attempts abest-effort headset rollback. If rollback sync fails, the headset state isreported as unknown. With no selected headset, changes are saved locally only;with an unsupported selected model, configuration commands are rejected beforesaving.

### TUI sync

The TUI also calls the same sync path after saving mode, noise, or immersivechanges. Tests inject a fake sync hook so unit tests do not touch real hardware.Unsupported selected models cannot enter the configuration screens.

### Status readback

`bose status` shows local desired state and, when the selected model supports it,
reads battery from the headset with `GET [2.2]`.

The legacy `--headphones` flag is accepted for compatibility but is no longer
required.

### Raw BMAP probe command

There is a hidden debug command for development:

```sh
BOSE_BMAP_RAW=1 cargo run -- bmap-raw <fblock> <func> <op> [payload_hex]
```

Examples:

```sh
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 2 2 1
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 31 10 1
BOSE_BMAP_RAW=1 cargo run -- bmap-raw 1 7 1
```

Do not expose this as a normal user command. It can send arbitrary writes.

## Adding a new Bluetooth configuration write

Use this process for new features:

1. **Add or update a model profile in** `src/models/`**.**
   - Every writable model must implement `HeadphoneModel`.
   - Keep unsupported models recognized but non-writable until hardware-verified.
2. **Find the BMAP address and operator** in `aaronsb/bosectl`.
   - Prefer `GET`, `START`, or verified unauthenticated `SETGET` paths.
   - Avoid plain `SET` unless the reference implementation proves it works.
3. **Define the domain model** in `src/domain.rs` or `src/config.rs`.
   - Validate ranges before writing hardware.
   - Keep the local `config.toml` representation human-readable.
4. **Add an encoder in the model profile, not generic** `src/bmap.rs`**.**
   - Convert config values into exact BMAP payload bytes.
   - Add unit tests for edge values and signed-byte casts.
5. **Send with response validation.**
   - Use the same `send`/`send_expect` pattern.
   - Validate `fblock`, `func`, operator, and `ERROR` frames.
   - Add delays if the headset times out with back-to-back writes.
6. **Wire CLI/TUI after local save.**
   - Save config first.
   - Attempt sync second.
   - If sync fails, restore the previous desired config and attempt rollback.
7. **Add tests that do not touch hardware.**
   - Test payload construction.
   - Test parser behavior.
   - In TUI tests, inject a fake sync function.
8. **Live-verify with safe reads.**
   - Use `status` for battery.
   - Use `BOSE_BMAP_RAW=1 ... bmap-raw <addr> <GET>` for readback.

## Current limitations

- macOS is the only implemented RFCOMM transport in this repo.
- Custom mode writes via `[31.6] ModeConfig` are documented by `bosectl`, but not
  implemented here yet.
- Device name, sidetone, multipoint, auto-pause, auto-answer, pairing, routing,
  and power are not exposed in the CLI yet.
- `bose status` only reads battery today; it does not yet reconcile
  all headset state back into local config.
- Captures in `aaronsb/bosectl` show additional changing fields (`5.7`, `5.13`)
  during app interactions. The CLI uses the cleaner verified direct write path
  `[31.10]` for live audio settings.

## Quick packet reference

```text
Battery GET:
  [2, 2, GET, 0]

Mode switch to Quiet:
  [31, 3, START, 2, 0, 0]

Mode switch to Aware:
  [31, 3, START, 2, 1, 0]

Live audio settings:
  [31, 10, SETGET, 5, cnc, 0, spatial, 0, anc]

EQ bass to -10:
  [1, 7, SETGET, 2, 0xf6, 0]

EQ mid to 0:
  [1, 7, SETGET, 2, 0x00, 1]

EQ treble to +10:
  [1, 7, SETGET, 2, 0x0a, 2]
```
