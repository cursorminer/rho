#!/bin/bash
cargo xtask bundle rho --release;
rm -r /Library/Audio/Plug-Ins/VST3/Rho.vst3;
mv /Users/rtu/dev/Rust/projects/rho/target/bundled/Rho.vst3 /Library/Audio/Plug-Ins/VST3/
