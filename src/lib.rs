#![allow(dead_code)]

use arrayvec::ArrayVec;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::sync::{Arc, Mutex};

mod editor;
pub mod grid_activations;
pub mod looping_state;
pub mod note_assigner;
pub mod rho;
pub mod rho_config;
pub mod shared_state;
pub mod step_switch;
pub mod step_timer;

use grid_activations::GridActivations;
use note_assigner::Note;
use rho::Rho;
use rho_config::NUM_ROWS;
use shared_state::{unpack_row, SharedState};
use step_timer::{StepEvent, StepTimer};

// the rows rho starts with, before the gui or a saved state changes them
const DEFAULT_ROW_LENGTH: usize = 4;
const DEFAULT_DENSITY: f32 = 0.5;

// never a packed row, so every row gets applied
const ROWS_NOT_APPLIED: u32 = u32::MAX;

pub struct RhoPlugin {
    params: Arc<RhoParams>,
    shared: Arc<SharedState>,
    // the packed rows last given to rho, to spot when the gui changes them
    applied_rows: [u32; NUM_ROWS],
    sample_rate: f32,
    rho: Rho,
    step_timer: StepTimer,
    // the notes that have been started and not stopped yet
    sounding_notes: ArrayVec<Note, NUM_ROWS>,
    hold_enabled: bool,
}

#[derive(Enum, Debug, PartialEq, Clone, Copy)]
pub enum StepLength {
    #[name = "1/32"]
    ThirtySecond,
    #[name = "1/16"]
    Sixteenth,
    #[name = "1/8"]
    Eighth,
    #[name = "1/4"]
    Quarter,
}

impl StepLength {
    fn beats(self) -> f64 {
        match self {
            StepLength::ThirtySecond => 0.125,
            StepLength::Sixteenth => 0.25,
            StepLength::Eighth => 0.5,
            StepLength::Quarter => 1.0,
        }
    }
}

#[derive(Params)]
pub struct RhoParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // the step pattern. Only the gui and initialize() touch this, the audio thread gets the rows
    // through SharedState.
    #[persist = "grid"]
    grid: Mutex<GridActivations>,

    #[id = "step-length"]
    pub step_length: EnumParam<StepLength>,

    // how long notes last as a fraction of a step
    #[id = "gate"]
    pub gate: FloatParam,

    // keep notes on their rows when other notes are released
    #[id = "hold"]
    pub hold: BoolParam,
}

impl Default for RhoParams {
    fn default() -> Self {
        let mut grid = GridActivations::new(NUM_ROWS, DEFAULT_ROW_LENGTH);
        grid.set_normalized_density(DEFAULT_DENSITY);

        Self {
            editor_state: editor::default_state(),
            grid: Mutex::new(grid),
            step_length: EnumParam::new("Step Length", StepLength::Sixteenth),
            gate: FloatParam::new(
                "Gate",
                0.5,
                FloatRange::Linear {
                    min: 0.05,
                    max: 1.0,
                },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            hold: BoolParam::new("Hold", false),
        }
    }
}

impl Default for RhoPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(RhoParams::default()),
            shared: Arc::new(SharedState::default()),
            applied_rows: [ROWS_NOT_APPLIED; NUM_ROWS],
            sample_rate: 44100.0,
            rho: Rho::new(),
            step_timer: StepTimer::default(),
            sounding_notes: ArrayVec::new(),
            hold_enabled: false,
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

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;

        // this also runs after a state is loaded, so pick up the loaded grid
        self.reload_rows();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel in buffer.as_slice() {
            channel.fill(0.0);
        }

        while let Some(event) = context.next_event() {
            self.handle_event(event);
        }

        let num_samples = buffer.samples();
        let transport = context.transport();
        let position = match (transport.playing, transport.tempo, transport.pos_beats()) {
            (true, Some(tempo), Some(start_beat)) => Some(Position { tempo, start_beat }),
            _ => None,
        };
        let settings = Settings {
            step_beats: self.params.step_length.value().beats(),
            gate: self.params.gate.value() as f64,
            hold: self.params.hold.value(),
        };

        self.process_block(num_samples, position, settings, &mut |event| {
            context.send_event(event)
        });
        ProcessStatus::Normal
    }
}

// where the host's transport is at the start of a block
#[derive(Clone, Copy)]
struct Position {
    tempo: f64,
    start_beat: f64,
}

// the param values for a block
#[derive(Clone, Copy)]
struct Settings {
    step_beats: f64,
    gate: f64,
    hold: bool,
}

// The block processing works on plain values rather than a ProcessContext so it can be tested
// without a host.
impl RhoPlugin {
    // send the grid in the params to rho, e.g. after a state is loaded
    fn reload_rows(&mut self) {
        self.shared.publish_rows(&self.params.grid.lock().unwrap());
        self.applied_rows = [ROWS_NOT_APPLIED; NUM_ROWS];
    }

    // held notes go to the arpeggiator, everything else is dropped for now.
    // these are applied for the whole block rather than at their exact time.
    fn handle_event(&mut self, event: NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { note, velocity, .. } => self
                .rho
                .note_on(note as usize, (velocity * 127.0).round() as usize),
            NoteEvent::NoteOff { note, .. } => self.rho.note_off(note as usize),
            _ => (),
        }
    }

    // position is None when the transport is stopped
    fn process_block(
        &mut self,
        num_samples: usize,
        position: Option<Position>,
        settings: Settings,
        send: &mut impl FnMut(NoteEvent<()>),
    ) {
        for (row, applied) in self.applied_rows.iter_mut().enumerate() {
            let packed = self.shared.packed_row(row);
            if packed != *applied {
                self.rho.set_row(row, &unpack_row(packed));
                *applied = packed;
            }
        }

        if settings.hold != self.hold_enabled {
            self.hold_enabled = settings.hold;
            self.rho.set_hold_notes_enabled(settings.hold);
        }

        let rho = &mut self.rho;
        let sounding_notes = &mut self.sounding_notes;

        let Some(Position { tempo, start_beat }) = position.filter(|_| num_samples > 0) else {
            self.step_timer
                .stop(|_| send_note_offs(sounding_notes, send, 0));
            rho.stop();
            self.publish_display();
            return;
        };

        let samples_per_beat = self.sample_rate as f64 * 60.0 / tempo;
        let end_beat = start_beat + num_samples as f64 / samples_per_beat;
        let to_timing = |beat: f64| {
            let sample = ((beat - start_beat) * samples_per_beat).round();
            sample.clamp(0.0, (num_samples - 1) as f64) as u32
        };

        self.step_timer.process_block(
            start_beat,
            end_beat,
            settings.step_beats,
            settings.gate,
            |event| match event {
                StepEvent::NotesOff { beat } => {
                    send_note_offs(sounding_notes, send, to_timing(beat))
                }
                StepEvent::Step { index, beat } => {
                    for note in rho.on_step(index) {
                        send(NoteEvent::NoteOn {
                            timing: to_timing(beat),
                            voice_id: None,
                            channel: 0,
                            note: note.note_number as u8,
                            velocity: note.velocity as f32 / 127.0,
                        });
                        let _ = sounding_notes.try_push(note);
                    }
                }
            },
        );

        self.publish_display();
    }

    // let the gui show what rho is doing
    fn publish_display(&self) {
        self.shared.set_playing_steps(self.rho.get_playing_steps());
        self.shared
            .try_set_notes_for_rows(|row| self.rho.notes_for_row(row));
    }
}

fn send_note_offs(
    sounding_notes: &mut ArrayVec<Note, NUM_ROWS>,
    send: &mut impl FnMut(NoteEvent<()>),
    timing: u32,
) {
    for note in sounding_notes.drain(..) {
        send(NoteEvent::NoteOff {
            timing,
            voice_id: None,
            channel: 0,
            note: note.note_number as u8,
            velocity: 0.0,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f32 = 48000.0;
    const TEMPO: f64 = 120.0;
    // a 1/16 step at 120bpm
    const STEP_SAMPLES: u32 = 6000;

    const DEFAULT_SETTINGS: Settings = Settings {
        step_beats: 0.25,
        gate: 0.5,
        hold: false,
    };

    fn grid(rows: [&[bool]; NUM_ROWS]) -> GridActivations {
        let mut grid = GridActivations::new(NUM_ROWS, 1);
        for (row, steps) in rows.iter().enumerate() {
            grid.set_row_length(row, steps.len());
            for (step, &on) in steps.iter().enumerate() {
                grid.set(row, step, on);
            }
        }
        grid
    }

    const SILENT: [&[bool]; NUM_ROWS] = [&[false], &[false], &[false], &[false]];

    // a plugin as if it had loaded these rows from a saved state
    fn plugin(rows: [&[bool]; NUM_ROWS]) -> RhoPlugin {
        let mut plugin = RhoPlugin::default();
        plugin.sample_rate = SAMPLE_RATE;
        *plugin.params.grid.lock().unwrap() = grid(rows);
        plugin.reload_rows();
        plugin
    }

    fn at_beat(start_beat: f64) -> Option<Position> {
        Some(Position {
            tempo: TEMPO,
            start_beat,
        })
    }

    fn note_on(plugin: &mut RhoPlugin, note: u8) {
        plugin.handle_event(NoteEvent::NoteOn {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 100.0 / 127.0,
        });
    }

    fn note_off(plugin: &mut RhoPlugin, note: u8) {
        plugin.handle_event(NoteEvent::NoteOff {
            timing: 0,
            voice_id: None,
            channel: 0,
            note,
            velocity: 0.0,
        });
    }

    // the events from one block as (on, note, timing)
    fn run(
        plugin: &mut RhoPlugin,
        num_samples: usize,
        position: Option<Position>,
        settings: Settings,
    ) -> Vec<(bool, u8, u32)> {
        let mut events = Vec::new();
        plugin.process_block(num_samples, position, settings, &mut |event| {
            events.push(match event {
                NoteEvent::NoteOn { note, timing, .. } => (true, note, timing),
                NoteEvent::NoteOff { note, timing, .. } => (false, note, timing),
                other => panic!("unexpected event {other:?}"),
            })
        });
        events
    }

    #[test]
    fn held_notes_play_on_their_rows_at_step_times() {
        let mut plugin = plugin([&[true, false], &[false, true], &[false], &[false]]);
        note_on(&mut plugin, 60);
        note_on(&mut plugin, 64);

        let events = run(
            &mut plugin,
            2 * STEP_SAMPLES as usize,
            at_beat(0.0),
            DEFAULT_SETTINGS,
        );
        assert_eq!(
            events,
            vec![
                (true, 60, 0),
                (false, 60, STEP_SAMPLES / 2),
                (true, 64, STEP_SAMPLES),
                (false, 64, STEP_SAMPLES * 3 / 2),
            ]
        );
    }

    #[test]
    fn velocity_passes_through() {
        let mut plugin = plugin([&[true], &[false], &[false], &[false]]);
        note_on(&mut plugin, 60);

        let mut velocity = None;
        plugin.process_block(100, at_beat(0.0), DEFAULT_SETTINGS, &mut |event| {
            if let NoteEvent::NoteOn { velocity: v, .. } = event {
                velocity = Some(v);
            }
        });
        assert_eq!(velocity, Some(100.0 / 127.0));
    }

    #[test]
    fn steps_follow_the_song_position() {
        let mut plugin = plugin([&[false, true], &[false], &[false], &[false]]);
        note_on(&mut plugin, 60);

        // starting a fifth of a beat in, the next step (an odd one) is a twentieth of a beat away
        let events = run(&mut plugin, 2000, at_beat(0.2), DEFAULT_SETTINGS);
        assert_eq!(events, vec![(true, 60, 1200)]);
    }

    #[test]
    fn step_length_setting_spaces_steps() {
        let mut plugin = plugin([&[true], &[false], &[false], &[false]]);
        note_on(&mut plugin, 60);

        let eighths = Settings {
            step_beats: 0.5,
            ..DEFAULT_SETTINGS
        };
        let note_ons: Vec<_> = run(
            &mut plugin,
            4 * STEP_SAMPLES as usize,
            at_beat(0.0),
            eighths,
        )
        .into_iter()
        .filter(|(on, ..)| *on)
        .map(|(.., timing)| timing)
        .collect();
        assert_eq!(note_ons, vec![0, 2 * STEP_SAMPLES]);
    }

    #[test]
    fn row_edits_from_the_gui_apply_on_the_next_block() {
        let mut plugin = plugin(SILENT);
        note_on(&mut plugin, 60);
        assert_eq!(
            run(&mut plugin, 100, at_beat(0.0), DEFAULT_SETTINGS),
            vec![]
        );

        // what the editor does after a click
        plugin
            .shared
            .publish_rows(&grid([&[true], &[false], &[false], &[false]]));

        let events = run(&mut plugin, 100, at_beat(1.0), DEFAULT_SETTINGS);
        assert_eq!(events, vec![(true, 60, 0)]);
    }

    #[test]
    fn reload_picks_up_a_loaded_grid() {
        let mut plugin = plugin(SILENT);
        note_on(&mut plugin, 60);

        // loading a state replaces the grid, then initialize() reloads it
        *plugin.params.grid.lock().unwrap() = grid([&[true], &[false], &[false], &[false]]);
        assert_eq!(
            run(&mut plugin, 100, at_beat(0.0), DEFAULT_SETTINGS),
            vec![]
        );

        plugin.reload_rows();
        let events = run(&mut plugin, 100, at_beat(1.0), DEFAULT_SETTINGS);
        assert_eq!(events, vec![(true, 60, 0)]);
    }

    #[test]
    fn stopping_the_transport_ends_sounding_notes() {
        let mut plugin = plugin([&[true], &[false], &[false], &[false]]);
        note_on(&mut plugin, 60);

        // block ends before the gate closes
        let events = run(&mut plugin, 1000, at_beat(0.0), DEFAULT_SETTINGS);
        assert_eq!(events, vec![(true, 60, 0)]);

        assert_eq!(
            run(&mut plugin, 1000, None, DEFAULT_SETTINGS),
            vec![(false, 60, 0)]
        );
        assert_eq!(run(&mut plugin, 1000, None, DEFAULT_SETTINGS), vec![]);
    }

    #[test]
    fn hold_keeps_notes_on_their_rows_when_others_are_released() {
        for hold in [false, true] {
            // only the bottom row plays
            let mut plugin = plugin([&[true], &[false], &[false], &[false]]);
            let settings = Settings {
                hold,
                ..DEFAULT_SETTINGS
            };
            run(&mut plugin, 100, None, settings);
            note_on(&mut plugin, 60);
            note_on(&mut plugin, 64);
            note_off(&mut plugin, 60);

            // without hold 64 drops to the bottom row, with hold it stays put and leaves a gap
            let expected = if hold { vec![] } else { vec![(true, 64, 0)] };
            let events = run(&mut plugin, 100, at_beat(0.0), settings);
            assert_eq!(events, expected, "hold {hold}");
        }
    }

    #[test]
    fn display_shows_playing_steps_and_row_notes() {
        let mut plugin = plugin([&[true, false], &[false, false, true], &[false], &[false]]);
        note_on(&mut plugin, 60);
        note_on(&mut plugin, 64);

        // the fourth step of the song
        run(&mut plugin, 100, at_beat(0.75), DEFAULT_SETTINGS);
        assert_eq!(
            plugin.shared.playing_steps(),
            [Some(1), Some(0), Some(0), Some(0)]
        );

        let notes = plugin.shared.notes_for_rows();
        let note_numbers =
            |row: usize| -> Vec<usize> { notes[row].iter().map(|note| note.note_number).collect() };
        assert_eq!(note_numbers(0), vec![60]);
        assert_eq!(note_numbers(1), vec![64]);

        run(&mut plugin, 100, None, DEFAULT_SETTINGS);
        assert_eq!(plugin.shared.playing_steps(), [None; NUM_ROWS]);
    }

    #[test]
    fn empty_block_counts_as_stopped() {
        let mut plugin = plugin([&[true], &[false], &[false], &[false]]);
        note_on(&mut plugin, 60);
        assert_eq!(run(&mut plugin, 0, at_beat(0.0), DEFAULT_SETTINGS), vec![]);
    }

    #[test]
    fn grid_survives_saving_and_loading() {
        let saved = RhoParams::default();
        *saved.grid.lock().unwrap() = grid([
            &[true, false, true],
            &[false, true],
            &[true, true, false, true],
            &[false],
        ]);
        let fields = saved.serialize_fields();
        assert!(fields.contains_key("grid"));

        let loaded = RhoParams::default();
        loaded.deserialize_fields(&fields);

        let mut saved_grid = saved.grid.lock().unwrap();
        let mut loaded_grid = loaded.grid.lock().unwrap();
        for row in 0..NUM_ROWS {
            assert_eq!(loaded_grid.get_row(row), saved_grid.get_row(row));
        }
        assert_eq!(
            loaded_grid.get_normalized_density(),
            saved_grid.get_normalized_density()
        );

        // the hidden thresholds came too, so changing the density picks the same steps
        saved_grid.set_normalized_density(0.75);
        loaded_grid.set_normalized_density(0.75);
        for row in 0..NUM_ROWS {
            assert_eq!(loaded_grid.get_row(row), saved_grid.get_row(row));
        }
    }

    #[test]
    fn default_grid_plays_out_of_the_box() {
        let params = RhoParams::default();
        let grid = params.grid.lock().unwrap();
        assert_eq!(
            grid.get_normalized_density(),
            DEFAULT_DENSITY,
            "half the steps on"
        );
        for row in 0..NUM_ROWS {
            assert_eq!(grid.row_length(row), DEFAULT_ROW_LENGTH);
        }
    }
}
