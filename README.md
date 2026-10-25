# Rho

A row-based MIDI arpeggiator/sequencer plugin (VST3/CLAP) written in Rust.

Uses nih-plug for the plugin and egui for the UI.

## Building

```shell
# Build release VST3/CLAP bundles
cargo xtask bundle rho --release

# Run tests
cargo test
```

On macOS, `./bmv.sh` builds and installs the VST3 to the system plugin folder.
