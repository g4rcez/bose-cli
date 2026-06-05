# Contributing

Thanks for helping improve `bose-cli`.

## Before opening a PR

Run the standard checks:

```sh
cargo fmt -- --check
cargo test
cargo check
```

Hardware-dependent behavior should stay out of automated tests. Prefer unit tests
with fake sync hooks for config and TUI behavior.

## Adding model support

Only **Bose QuietComfort Ultra Headphones (2nd Gen)** have verified write support
today. New writable headset support should:

1. Add or update a `HeadphoneModel` profile in `src/models/`.
2. Refuse writes until the model has been tested on real hardware.
3. Document every protocol command in `PROTOCOL.md`.
4. Preserve BMAP response validation for function block, function, operator, and
   error frames.

Do not send QC Ultra 2 packets to unknown or unsupported models.

## Reporting bugs

Include:

- macOS version
- headset model and Bluetooth name
- `bose doctor` output
- the command that failed
- whether the headset was paired and connected in macOS Bluetooth settings
