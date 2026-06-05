# 001 Init - First iteration

This is the first spec about `bose-cli`

## Requirements

- CLI
- Use Rust
- Use [fzf](https://github.com/junegunn/fzf) to show the devices and options
- Work with bluetooth API from the OS to communicate with the devices
- Change modes from Bose

## Bose modes

At the original app we have "Modes", that is a preset of configurations. It will be awesome if we can manage them throw this CLI, using a TUI. As well, save the modes in `$HOME/.config/bosecli/config.toml` file to manage manually if the user wants. There are some defaults from BoseApp:

- Quiet
- Aware
- Immersion

## Noise control

At the BoseApp we have this menu, it's basically a boolean to tell the app is enabled or not and a slider, with 10 levels of noise cancelling where 0 is the most outside sounds and 10 is block most outside sounds

## Immersive Audio

Bose Immersive Audio is a spatial audio technology designed to pull standard stereo sound out of your head and project it in front of you. It mimics the experience of listening to a pair of premium external speakers in an acoustic sweet spot.

There are three available options:

- Off: immersive audio off
- Still: Audio stays in place. This one is best when you're sitting down
- Motion: Audio follows you. This one is best when you're moving around

## Source

Manage the bluetooth connections of your Bose phone. Here we have two features

### Multi point connection

Allow your Bose headphone to connect into two devices

### Paired devices

A list of the devices that you Bose headphone have, and this will allow the Multi point connection. Only two devices are allowed at the time.

## Equalizer

This feature allow us to manage the equalization of the sound, by changing

- Bass: The low-frequency part of sound. From -10 to +10
  - How it sounds: Deep, heavy, and powerful.
  - Examples: Kick drums, bass guitar, explosions, deep voices.
- Mid: The middle-frequency part of sound. From -10 to +10
  - How it sounds: Clear, present, and natural.
  - Examples: Human voice, guitars, piano, snare drum, many instruments.
- Treble: The high-frequency part of sound. From -10 to +10
  - How it sounds: Bright, sharp, detailed, and airy.
  - Examples: Cymbals, hi-hats, high notes, breathiness in vocals, small sound details.

Bass = depth and powerMid = voices and main instrumentsTreble = brightness and detail