#![allow(dead_code)]

use nih_plug::prelude::*;
use std::sync::Arc;

pub mod grid_activations;
pub mod looping_state;
pub mod note_assigner;
pub mod rho;
pub mod rho_config;
pub mod step_switch;

struct RhoPlugin {
    params: Arc<RhoParams>,
    sample_rate: f32,
    // DEBUG: whether the debug beat note is currently sounding
    debug_note_on: bool,
}

#[derive(Default, Params)]
struct RhoParams {}

impl Default for RhoPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(RhoParams::default()),
            sample_rate: 44100.0,
            debug_note_on: false,
        }
    }
}

impl Plugin for RhoPlugin {
    const NAME: &'static str = "Rho";
    const VENDOR: &'static str = "Rob Tubb";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "rob@cursorminer.org";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // A silent stereo output by default, because Live won't open a VST3 without audio. Rho sits
    // on a MIDI track as an instrument and other tracks route MIDI from it. The MIDI only layout
    // is for hosts that support pure note effects.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        AudioIOLayout::const_default(),
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut debug_events = [None; DEBUG_MAX_EVENTS];
        let num_debug_events = self.debug_beat_events(buffer.samples(), context, &mut debug_events);
        let mut debug_events = debug_events[..num_debug_events].iter().flatten().peekable();

        // pass everything straight through for now, merged in time order with the debug notes
        while let Some(event) = context.next_event() {
            while let Some(debug_event) = debug_events.next_if(|e| e.timing() <= event.timing()) {
                context.send_event(*debug_event);
            }
            context.send_event(event);
        }
        for debug_event in debug_events {
            context.send_event(*debug_event);
        }

        for channel in buffer.as_slice() {
            channel.fill(0.0);
        }

        ProcessStatus::Normal
    }
}

// DEBUG: plays C3 on every beat while the transport runs, to check MIDI output reaches the host.
// Remove once the sequencer is wired up.
const DEBUG_NOTE: u8 = 60;
const DEBUG_MAX_EVENTS: usize = 8;

impl RhoPlugin {
    // fills `events` with a note on at each beat and a note off at each half beat in this block,
    // returns how many were written
    fn debug_beat_events(
        &mut self,
        num_samples: usize,
        context: &mut impl ProcessContext<Self>,
        events: &mut [Option<PluginNoteEvent<Self>>; DEBUG_MAX_EVENTS],
    ) -> usize {
        let transport = context.transport();
        let (Some(tempo), Some(start_beat)) = (transport.tempo, transport.pos_beats()) else {
            return 0;
        };
        let note_off = |timing| NoteEvent::NoteOff {
            timing,
            voice_id: None,
            channel: 0,
            note: DEBUG_NOTE,
            velocity: 0.0,
        };

        if !transport.playing {
            if self.debug_note_on {
                self.debug_note_on = false;
                events[0] = Some(note_off(0));
                return 1;
            }
            return 0;
        }

        let samples_per_beat = self.sample_rate as f64 * 60.0 / tempo;
        let end_beat = start_beat + num_samples as f64 / samples_per_beat;

        let mut count = 0;
        let mut half_beat = (start_beat * 2.0).ceil();
        while half_beat / 2.0 < end_beat && count < DEBUG_MAX_EVENTS {
            let timing = ((half_beat / 2.0 - start_beat) * samples_per_beat) as u32;
            let is_on = half_beat as i64 % 2 == 0;
            events[count] = Some(if is_on {
                NoteEvent::NoteOn {
                    timing,
                    voice_id: None,
                    channel: 0,
                    note: DEBUG_NOTE,
                    velocity: 0.8,
                }
            } else {
                note_off(timing)
            });
            self.debug_note_on = is_on;
            count += 1;
            half_beat += 1.0;
        }
        count
    }
}

impl ClapPlugin for RhoPlugin {
    const CLAP_ID: &'static str = "org.cursorminer.rho";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A row-based MIDI arpeggiator");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for RhoPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"CursorminerRhoAr";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Tools];
}

nih_export_clap!(RhoPlugin);
nih_export_vst3!(RhoPlugin);
