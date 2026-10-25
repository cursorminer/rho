#!/bin/bash
# Runs Rho as a standalone app. Pass extra standalone flags after the script, e.g. ./run.sh --tempo 90
# Pick MIDI devices with RHO_MIDI_IN / RHO_MIDI_OUT, e.g. RHO_MIDI_OUT="IAC Driver Bus 1".
# An empty name lists what's available.

# The standalone must run at the output device's real rate, otherwise its transport (and so the
# whole sequencer) runs at the wrong speed.
RATE=$(system_profiler SPAudioDataType | grep -A6 "Default Output Device: Yes" \
    | grep "Current SampleRate" | awk '{print $3}')

MIDI_ARGS=()
[ -n "${RHO_MIDI_IN+set}" ] && MIDI_ARGS+=(--midi-input "$RHO_MIDI_IN")
[ -n "${RHO_MIDI_OUT+set}" ] && MIDI_ARGS+=(--midi-output "$RHO_MIDI_OUT")

# core-audio rather than auto, so a bad device name is an error instead of a silent dummy backend
cargo run --release --bin rho -- -b core-audio -r "${RATE:-48000}" "${MIDI_ARGS[@]}" "$@"
