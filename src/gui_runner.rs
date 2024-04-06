// run the egui update function

use crate::grid_activations::GridActivations;
use crate::messages::*;
use crate::rho_config::NUM_ROWS;
use crate::step_switch::*;
use eframe::egui;
use midir::{MidiInput, MidiOutput};
use std::time::Duration;

struct UiState {
    // these vars are persistent across frames
    selected_in_port: usize,
    selected_out_port: usize,
    midi_in_channel: u8,
    midi_out_channel: u8,
    note_strings_for_rows: Vec<String>,
    hold_checkbox_enabled: bool,
    playing_steps_for_rows: [Option<usize>; NUM_ROWS],
    playing: bool,
    tempo: f32,
}

impl UiState {
    fn new() -> Self {
        Self {
            selected_in_port: 0,
            selected_out_port: 0,
            midi_in_channel: 0,
            midi_out_channel: 0,
            note_strings_for_rows: vec!["".to_string(); NUM_ROWS],
            hold_checkbox_enabled: false,
            playing_steps_for_rows: [None; NUM_ROWS],
            playing: false,
            tempo: 120.0,
        }
    }
}

// gui takes ownership of the grid
pub fn run_gui(
    rx: std::sync::mpsc::Receiver<MessageToGui>,
    tx: std::sync::mpsc::Sender<MessageGuiToRho>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        default_theme: eframe::Theme::Dark,
        follow_system_theme: false,
        ..Default::default()
    };

    let mut ui_state = UiState::new();

    // grid could go in UiState too
    let mut grid = GridActivations::new(4, 4);

    // TODO send all the intial gui state to Rho
    let _ = tx.send(MessageGuiToRho::SetTempo {
        tempo: ui_state.tempo,
    });

    let _ = tx.send(MessageGuiToRho::RowActivations {
        row_activations: grid.get_row_activations(),
    });

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        // these vars are reset each frame
        let mut do_send_row_activations = false;

        top_panel(ctx, &mut ui_state, &tx);

        egui::CentralPanel::default().show(ctx, |ui| {
            // first recieve messages from the clock thread
            match rx.try_recv() {
                Ok(MessageToGui::Tick { playing_steps }) => {
                    ui_state.playing_steps_for_rows = playing_steps;
                    ctx.request_repaint();
                }
                Ok(MessageToGui::NotesForRows { notes }) => {
                    // assign notes to the note_strings_for_rows
                    for i in 0..NUM_ROWS {
                        let mut note_str = String::new();
                        for note in notes[i].iter() {
                            note_str.push_str(&format!("{} ", note));
                        }
                        ui_state.note_strings_for_rows[i] = note_str.clone();
                        ctx.request_repaint();
                    }
                }
                _ => (),
            }

            let mut density: usize = (grid.get_normalized_density() * 127.0) as usize;

            for row in (0..NUM_ROWS).rev() {
                let playing_step = ui_state.playing_steps_for_rows[row];
                do_send_row_activations = do_send_row_activations
                    || draw_row(ui, &mut grid, &mut ui_state, row, playing_step);
            }

            ui.horizontal(|ui| {
                if ui
                    .add(egui::Slider::new(&mut density, 0..=127).text("density"))
                    .changed()
                {
                    let norm_density = density as f32 / 127.0;
                    grid.set_normalized_density(norm_density);
                    do_send_row_activations = true;
                }

                if ui.button("New Dist").clicked() {
                    grid.create_new_distribution_given_active_steps();
                    do_send_row_activations = true;
                }

                if ui
                    .checkbox(&mut ui_state.hold_checkbox_enabled, "Hold")
                    .changed()
                {
                    let _ = tx.send(MessageGuiToRho::HoldNotesEnabled {
                        enabled: ui_state.hold_checkbox_enabled,
                    });
                }
            });

            if do_send_row_activations {
                let _ = tx.send(MessageGuiToRho::RowActivations {
                    row_activations: grid.get_row_activations(),
                });
            }

            ctx.request_repaint_after(Duration::from_millis(100));
        });
    });
}

// draw a single row: todo make it stretchy
fn draw_row(
    ui: &mut egui::Ui,
    grid: &mut GridActivations,
    ui_state: &mut UiState,
    row: usize,
    playing_step: Option<usize>,
) -> bool {
    let mut do_send_row_activations = false;

    // todo stretch this to fill the width of the panel

    ui.horizontal(|ui| {
        let spacing = ui.spacing().item_spacing;

        let fixed_left_width = 100.0;
        let fixed_right_width = 200.0;

        // a text display of the note for this row
        ui.add_sized(
            [fixed_left_width, 50.0],
            egui::Label::new(&ui_state.note_strings_for_rows[row]),
        );

        // draw the row of steps
        let mut row_length = grid.row_length(row);

        let steps_width = ui.available_size().x
            - fixed_left_width
            - fixed_right_width
            - spacing.x * (row_length - 1) as f32;

        let step_width = steps_width / row_length as f32;
        for step in 0..row_length {
            let mut active = grid.get(row, step);
            let is_playing = playing_step == Some(step);

            // set the size on this step switch
            if ui
                .add_sized([step_width, 50.0], step_switch(&mut active, is_playing))
                .changed()
            {
                grid.set(row, step, active);
                do_send_row_activations = true;
            }
        }

        // todo replace with +- buttons
        if ui
            .add(egui::Slider::new(&mut row_length, 2..=8).text("Row Length"))
            .changed()
        {
            grid.set_row_length(row, row_length);
            do_send_row_activations = true;
        }
    });

    do_send_row_activations
}

fn top_panel(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    tx: &std::sync::mpsc::Sender<MessageGuiToRho>,
) {
    // set up midi list here TODO this happens every frame! Might be slow
    // could instead use a popup window to set midi ports and if they come and go then we don't care
    let midi_in = MidiInput::new("midir input").unwrap();
    let in_ports = midi_in.ports();
    let mut in_port_names: Vec<String> = in_ports
        .iter()
        .map(|port| midi_in.port_name(port).unwrap())
        .collect();

    if in_port_names.len() == 0 {
        in_port_names.push("No Midi In Ports".to_string());
    }

    let midi_out = MidiOutput::new("midir output").unwrap();
    let out_ports = midi_out.ports();
    // let in_port_name = midi_in.port_name(&in_port)?;
    let mut out_port_names: Vec<String> = out_ports
        .iter()
        .map(|port| midi_out.port_name(port).unwrap())
        .collect();

    if out_port_names.len() == 0 {
        out_port_names.push("No Midi Out Ports".to_string());
    }

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        ui.heading("Rho Sequencer");

        ui.horizontal(|ui| {
            let response = egui::ComboBox::from_label("Midi In Port")
                .selected_text(format!("{:?}", in_port_names[ui_state.selected_in_port]))
                .show_ui(ui, |ui| {
                    let mut i = 0;
                    for port in in_port_names.iter() {
                        ui.selectable_value(&mut ui_state.selected_in_port, i, port);
                        i += 1;
                    }
                });

            // if the midi port selection was changed, send a message to the clock thread
            if response.response.changed() {
                let _ = tx.send(MessageGuiToRho::SetMidiInPort {
                    port: ui_state.selected_in_port,
                });
            }

            if ui
                .add(egui::DragValue::new(&mut ui_state.midi_in_channel).clamp_range(0..=15))
                .changed()
            {
                let _ = tx.send(MessageGuiToRho::SetMidiChannelIn {
                    channel: ui_state.midi_in_channel,
                });
            }

            let response = egui::ComboBox::from_label("Midi Out Port")
                .selected_text(format!("{:?}", out_port_names[ui_state.selected_out_port]))
                .show_ui(ui, |ui| {
                    let mut i = 0;
                    for port in out_port_names.iter() {
                        ui.selectable_value(&mut ui_state.selected_out_port, i, port);
                        i += 1;
                    }
                });

            if response.response.changed() {
                let _ = tx.send(MessageGuiToRho::SetMidiOutPort {
                    port: ui_state.selected_out_port,
                });
            }

            if ui
                .add(egui::DragValue::new(&mut ui_state.midi_out_channel).clamp_range(0..=15))
                .changed()
            {
                let _ = tx.send(MessageGuiToRho::SetMidiChannelOut {
                    channel: ui_state.midi_out_channel,
                });
            }
        });

        ui.add_space(10.0);

        // add transport controls
        ui.horizontal(|ui| {
            if ui.checkbox(&mut ui_state.playing, "Play").clicked() {
                let _ = tx.send(MessageGuiToRho::SetPlaying { playing: true });
            }

            if ui
                .add(egui::Slider::new(&mut ui_state.tempo, 40.0..=1000.0).text("Tempo"))
                .changed()
            {
                let _ = tx.send(MessageGuiToRho::SetTempo {
                    tempo: ui_state.tempo,
                });
            }
        });

        ui.add_space(10.0);
    });
}
