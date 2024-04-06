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

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        // set up midi list here TODO this happens every frame! Might be slow
        let midi_in = MidiInput::new("midir input").unwrap();
        let in_ports = midi_in.ports();
        let in_port_names: Vec<String> = in_ports
            .iter()
            .map(|port| midi_in.port_name(port).unwrap())
            .collect();

        let midi_out = MidiOutput::new("midir output").unwrap();
        let out_ports = midi_out.ports();
        // let in_port_name = midi_in.port_name(&in_port)?;
        let out_port_names: Vec<String> = out_ports
            .iter()
            .map(|port| midi_out.port_name(port).unwrap())
            .collect();

        // these vars are reset each frame
        let mut do_send_row_activations = false;

        egui::CentralPanel::default().show(ctx, |ui| {
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
            });

            ui.horizontal(|ui| {
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
                    let _ = tx.send(MessageGuiToRho::SetMidiInPort {
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

            let mut density: usize = (grid.get_normalized_density() * 127.0) as usize;
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

            for row in (0..NUM_ROWS).rev() {
                ui.horizontal(|ui| {
                    // a text display of the note for this row

                    ui.add_sized(
                        [100.0, 50.0],
                        egui::Label::new(&ui_state.note_strings_for_rows[row]),
                    );

                    let mut row_length = grid.row_length(row);
                    if ui
                        .add(egui::Slider::new(&mut row_length, 2..=8).text("Row Length"))
                        .changed()
                    {
                        grid.set_row_length(row, row_length);
                        do_send_row_activations = true;
                    }
                    for step in 0..row_length {
                        let mut active = grid.get(row, step);
                        if toggle_ui(ui, &mut active).changed() {
                            grid.set(row, step, active);
                            do_send_row_activations = true;
                        }
                    }
                });
            }

            match rx.try_recv() {
                Ok(MessageToGui::Tick { .. }) => {
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

            if do_send_row_activations {
                let _ = tx.send(MessageGuiToRho::RowActivations {
                    row_activations: grid.get_row_activations(),
                });
            }

            ctx.request_repaint_after(Duration::from_millis(100));
        });
    });
}
