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
        self.shared.publish_rows(&self.params.grid.lock().unwrap());
        self.applied_rows = [ROWS_NOT_APPLIED; NUM_ROWS];
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

        for (row, applied) in self.applied_rows.iter_mut().enumerate() {
            let packed = self.shared.packed_row(row);
            if packed != *applied {
                self.rho.set_row(row, &unpack_row(packed));
                *applied = packed;
            }
        }

        // held notes go to the arpeggiator, everything else is dropped for now.
        // these are applied for the whole block rather than at their exact time.
        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => self
                    .rho
                    .note_on(note as usize, (velocity * 127.0).round() as usize),
                NoteEvent::NoteOff { note, .. } => self.rho.note_off(note as usize),
                _ => (),
            }
        }

        let hold = self.params.hold.value();
        if hold != self.hold_enabled {
            self.hold_enabled = hold;
            self.rho.set_hold_notes_enabled(hold);
        }

        let num_samples = buffer.samples();
        let transport = context.transport();
        let position = match (transport.playing, transport.tempo, transport.pos_beats()) {
            (true, Some(tempo), Some(start_beat)) if num_samples > 0 => Some((tempo, start_beat)),
            _ => None,
        };

        let rho = &mut self.rho;
        let sounding_notes = &mut self.sounding_notes;

        let Some((tempo, start_beat)) = position else {
            self.step_timer
                .stop(|_| send_note_offs(sounding_notes, context, 0));
            rho.stop();
            self.publish_display();
            return ProcessStatus::Normal;
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
            self.params.step_length.value().beats(),
            self.params.gate.value() as f64,
            |event| match event {
                StepEvent::NotesOff { beat } => {
                    send_note_offs(sounding_notes, context, to_timing(beat))
                }
                StepEvent::Step { index, beat } => {
                    for note in rho.on_step(index) {
                        context.send_event(NoteEvent::NoteOn {
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
        ProcessStatus::Normal
    }
}

impl RhoPlugin {
    // let the gui show what rho is doing
    fn publish_display(&self) {
        self.shared.set_playing_steps(self.rho.get_playing_steps());
        self.shared
            .try_set_notes_for_rows(|row| self.rho.notes_for_row(row));
    }
}

fn send_note_offs(
    sounding_notes: &mut ArrayVec<Note, NUM_ROWS>,
    context: &mut impl ProcessContext<RhoPlugin>,
    timing: u32,
) {
    for note in sounding_notes.drain(..) {
        context.send_event(NoteEvent::NoteOff {
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
