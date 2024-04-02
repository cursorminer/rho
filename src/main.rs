// import my lib here
extern crate rho;

use eframe::egui;
use midir::MidiOutputConnection;
use rho::clock::Clock;
use rho::grid_activations::GridActivations;
use rho::messages::*;
use rho::note_assigner::Note;
use rho::rho_config::NUM_ROWS;
use rho::step_switch::*;
use rho::Rho;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput};

const NOTE_ON_MSG: u8 = 0x90;
const NOTE_OFF_MSG: u8 = 0x80;

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // make into ref counted pointer
    let rho = Rho::new();
    let grid = GridActivations::new(4, 4);

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // channel from clock to gui
    let (tx, rx) = mpsc::channel();

    // channel from midi in to rho
    let (tx_midi_in, rx_midi_in) = mpsc::channel();

    // channel from gui to rho
    let (tx_gui, rx_gui) = mpsc::channel();

    // set up midi in connection
    let _conn_in = set_up_midi_in_connection(tx_midi_in);

    let clock_thread_handle = run_clock(tx, running, rho, rx_midi_in, rx_gui);

    // run gui in the main thread, it has a recieve channel
    run_gui(rx, tx_gui, grid);

    // when gui stops, we stop the clock thread via this atomic bool
    r.store(false, Ordering::SeqCst);
    clock_thread_handle.join().unwrap();
}

fn run_clock(
    tx: std::sync::mpsc::Sender<MessageToGui>,
    running: Arc<AtomicBool>,
    mut rho: Rho,
    rx_midi_in: std::sync::mpsc::Receiver<MidiInMessage>,
    rx_gui: std::sync::mpsc::Receiver<MessageGuiToRho>,
) -> thread::JoinHandle<()> {
    let clock_arc = Arc::new(Mutex::new(Clock::new()));
    let sample_rate = 32.0;
    let period_ms = (1000.0 / sample_rate) as u64;
    let mut sent_notes_for_rows: [Vec<Note>; NUM_ROWS] = Default::default();
    let mut midi_out_channel: u8 = 0;

    // run a clock in another thread.
    let handle = thread::spawn(move || {
        // open a midi out connection
        let midi_out_conn = get_midi_out_connection();
        let mut midi_out_conn = match midi_out_conn {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("Error: {}", e);
                return;
            }
        };

        while running.load(Ordering::SeqCst) {
            // check to see if there are any messages from the midi in
            match rx_midi_in.try_recv() {
                Ok(MidiInMessage::NoteOn(note, velocity)) => {
                    rho.note_on(note.into(), velocity.into());
                }
                Ok(MidiInMessage::NoteOff(note)) => {
                    rho.note_off(note.into());
                }
                _ => (),
            }

            match rx_gui.try_recv() {
                Ok(MessageGuiToRho::RowActivations { row_activations }) => {
                    rho.set_row_activations(row_activations);
                }
                Ok(MessageGuiToRho::HoldNotesEnabled { enabled }) => {
                    rho.set_hold_notes_enabled(enabled);
                }
                Ok(MessageGuiToRho::SetMidiChannelOut { channel }) => {
                    midi_out_channel = channel;
                }
                _ => (),
            }

            let mut clock = clock_arc.lock().unwrap();
            clock.set_rate(8.0, sample_rate);

            let clock_out = clock.tick();
            if let Some(c) = clock_out {
                if c {
                    // now get the notes to play
                    let notes_to_play = rho.on_clock_high();

                    for note in notes_to_play {
                        print!("----------clock------------- OUTPUT note on {}\n", note);
                        // send midi note on
                        midi_out_conn
                            .send(&[NOTE_ON_MSG + midi_out_channel, note.note_number as u8, 0x64])
                            .unwrap();
                    }
                    tx.send(MessageToGui::Tick { high: true }).unwrap();
                } else {
                    // send midi off for all notes
                    let notes_to_stop = rho.on_clock_low();
                    for note in notes_to_stop {
                        print!("----------clock------------- OUTPUT note off {}\n", note);
                        // send midi note off
                        midi_out_conn
                            .send(&[
                                NOTE_OFF_MSG + midi_out_channel,
                                note.note_number as u8,
                                0x64,
                            ])
                            .unwrap();
                    }
                    tx.send(MessageToGui::Tick { high: false }).unwrap();
                }
            }

            let new_notes_for_rows = rho.get_notes_for_rows();
            if new_notes_for_rows != sent_notes_for_rows {
                sent_notes_for_rows = new_notes_for_rows.clone();
                let _ = tx.send(MessageToGui::NotesForRows {
                    notes: new_notes_for_rows,
                });
            }

            thread::sleep(Duration::from_millis(period_ms));
        }
    });
    // TODO stop playing midi notes!
    handle
}

// gui takes ownership of the grid
fn run_gui(
    rx: std::sync::mpsc::Receiver<MessageToGui>,
    tx: std::sync::mpsc::Sender<MessageGuiToRho>,
    mut grid: GridActivations,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };

    // these vars are persistent across frames
    let mut selected_in_port = 0;
    let mut selected_out_port = 0;
    let mut midi_in_channel: u8 = 0;
    let mut midi_out_channel: u8 = 0;
    let mut note_strings_for_rows = vec!["C#".to_string(); NUM_ROWS];
    let mut hold_checkbox_state = false;

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
                    .selected_text(format!("{:?}", in_port_names[selected_in_port]))
                    .show_ui(ui, |ui| {
                        let mut i = 0;
                        for port in in_port_names.iter() {
                            ui.selectable_value(&mut selected_in_port, i, port);
                            i += 1;
                        }
                    });

                // if the midi port selection was changed, send a message to the clock thread
                if response.response.changed() {
                    let _ = tx.send(MessageGuiToRho::SetMidiInPort {
                        port: selected_in_port,
                    });
                }

                if ui
                    .add(egui::DragValue::new(&mut midi_in_channel).clamp_range(0..=15))
                    .changed()
                {
                    let _ = tx.send(MessageGuiToRho::SetMidiChannelIn {
                        channel: midi_in_channel,
                    });
                }
            });

            ui.horizontal(|ui| {
                let response = egui::ComboBox::from_label("Midi Out Port")
                    .selected_text(format!("{:?}", out_port_names[selected_out_port]))
                    .show_ui(ui, |ui| {
                        let mut i = 0;
                        for port in out_port_names.iter() {
                            ui.selectable_value(&mut selected_out_port, i, port);
                            i += 1;
                        }
                    });

                if response.response.changed() {
                    let _ = tx.send(MessageGuiToRho::SetMidiInPort {
                        port: selected_out_port,
                    });
                }

                if ui
                    .add(egui::DragValue::new(&mut midi_out_channel).clamp_range(0..=15))
                    .changed()
                {
                    let _ = tx.send(MessageGuiToRho::SetMidiChannelOut {
                        channel: midi_out_channel,
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

            if ui.checkbox(&mut hold_checkbox_state, "Hold").changed() {
                let _ = tx.send(MessageGuiToRho::HoldNotesEnabled {
                    enabled: hold_checkbox_state,
                });
            }

            for row in (0..NUM_ROWS).rev() {
                ui.horizontal(|ui| {
                    // a text display of the note for this row

                    ui.add_sized([100.0, 50.0], egui::Label::new(&note_strings_for_rows[row]));

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
                Ok(MessageToGui::Tick { high }) => {
                    ctx.request_repaint();
                }
                Ok(MessageToGui::NotesForRows { notes }) => {
                    // assign notes to the note_strings_for_rows
                    for i in 0..NUM_ROWS {
                        let mut note_str = String::new();
                        for note in notes[i].iter() {
                            note_str.push_str(&format!("{} ", note));
                        }
                        note_strings_for_rows[i] = note_str.clone();
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

fn set_up_midi_in_connection(
    tx: Sender<MidiInMessage>,
) -> Result<MidiInputConnection<Sender<MidiInMessage>>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir input")?;
    midi_in.ignore(Ignore::None);
    let in_port = select_port(&midi_in, "input")?;

    let conn_in = midi_in.connect(
        &in_port,
        "midir-input",
        move |stamp, message, tx| {
            on_midi_in(tx, stamp, message);
        },
        tx.clone(),
    )?;
    Ok(conn_in)
}

// when a midi in message is recieved, we call this function
fn on_midi_in(tx: &mut std::sync::mpsc::Sender<MidiInMessage>, _stamp: u64, message: &[u8]) {
    //println!("{}: {:?} (len = {})", stamp, message, message.len());

    const MSG_NOTE: u8 = 144;
    const MSG_NOTE_2: u8 = 145;
    const MSG_NOTE_OFF: u8 = 129;

    let status = message[0];
    let note = message[1];
    let velocity = message[2];

    if status == MSG_NOTE || status == MSG_NOTE_2 {
        if velocity > 0 {
            print!("sending note on {:?}\n", note);
            tx.send(MidiInMessage::NoteOn(note, velocity)).unwrap(); // TODO this can panic!
        } else {
            tx.send(MidiInMessage::NoteOff(note)).unwrap();
        }
    } else if status == MSG_NOTE_OFF {
        tx.send(MidiInMessage::NoteOff(note)).unwrap();
    }
}

fn get_midi_out_connection() -> Result<MidiOutputConnection, Box<dyn Error>> {
    let midi_out = MidiOutput::new("midir output")?;

    println!();
    let out_port = select_port(&midi_out, "output")?;

    // let in_port_name = midi_in.port_name(&in_port)?;
    let out_port_name = midi_out.port_name(&out_port)?;

    let conn_out = midi_out.connect(&out_port, &out_port_name)?;

    // how you use the connection
    // const NOTE_ON_MSG: u8 = 0x90;
    // const NOTE_OFF_MSG: u8 = 0x80;
    // const VELOCITY: u8 = 0x64;
    // // We're ignoring errors in here
    // let _ = conn_out.send(&[NOTE_ON_MSG, note, VELOCITY]);

    Ok(conn_out)
}

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port, Box<dyn Error>> {
    println!("Available {} ports:", descr);
    let midi_ports = midi_io.ports();
    for (i, p) in midi_ports.iter().enumerate() {
        println!("{}: {}", i, midi_io.port_name(p)?);
    }
    print!("Please select {} port: ", descr);
    stdout().flush()?;
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let port = midi_ports
        .get(input.trim().parse::<usize>()?)
        .ok_or("Invalid port number")?;

    // to force set port
    //let port = midi_ports.get(1).ok_or("Invalid port number")?;
    Ok(port.clone())
}
