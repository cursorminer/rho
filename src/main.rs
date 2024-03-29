// import my lib here
extern crate shitquencer;

use eframe::egui;
use midir::MidiOutputConnection;
use shitquencer::clock::Clock;
use shitquencer::grid_activations::GridActivations;
use shitquencer::note_assigner::Note;
use shitquencer::rho_config::NUM_ROWS;
use shitquencer::step_switch::*;
use shitquencer::Rho;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput};

enum MidiInMessage {
    NoteOn(u8, u8),
    NoteOff(u8),
}

struct Tick {
    high: bool,
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // make into ref counted pointer
    let mut rho = Rho::new();
    let mut grid = GridActivations::new(4, 4);

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // channel from clock to gui
    let (tx, rx) = mpsc::channel();

    // channel from midi in to rho
    let (tx_midi_in, rx_midi_in) = mpsc::channel();

    // set up midi in connection
    let _conn_in = set_up_midi_in_connection(tx_midi_in);

    let clock_thread_handle = run_clock(tx, running, rho, rx_midi_in);

    // run gui in the main thread, it has a recieve channel
    run_gui(rx, grid);

    // when gui stops, we stop the clock thread via this atomic bool
    r.store(false, Ordering::SeqCst);
    clock_thread_handle.join().unwrap();
}

fn set_up_midi_out(rho: &mut Rho) {
    let midi_out_conn = get_midi_out_connection();
    let mut midi_out_conn = match midi_out_conn {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let mut send_midi = |note: Note, on| {
        const NOTE_ON_MSG: u8 = 0x90;
        const NOTE_OFF_MSG: u8 = 0x80;
        const VELOCITY: u8 = 0x64;
        // We're ignoring errors in here
        if on {
            print!("starting note {}\n", note.note_number as u8);
            let _ = midi_out_conn.send(&[NOTE_ON_MSG, note.note_number as u8, VELOCITY]);
        } else {
            print!("stopin note {}\n", note.note_number as u8);
            let _ = midi_out_conn.send(&[NOTE_OFF_MSG, note.note_number as u8, VELOCITY]);
        }
    };
}

fn run_clock(
    tx: std::sync::mpsc::Sender<Tick>,
    running: Arc<AtomicBool>,
    mut rho: Rho,
    rx_midi_in: std::sync::mpsc::Receiver<MidiInMessage>,
) -> thread::JoinHandle<()> {
    let clock_arc = Arc::new(Mutex::new(Clock::new()));
    let sample_rate = 32.0;
    let period_ms = (1000.0 / sample_rate) as u64;

    // run a clock in another thread.
    let handle = thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            // check to see if there are any messages from the midi in
            match rx_midi_in.try_recv() {
                Ok(MidiInMessage::NoteOn(note, velocity)) => {
                    print!("note on {:?}\n", note);
                    rho.note_on(note.into(), velocity.into());
                }
                Ok(MidiInMessage::NoteOff(note)) => {
                    print!("note off {:?}\n", note);
                    rho.note_off(note.into());
                }
                _ => (),
            }

            let mut clock = clock_arc.lock().unwrap();

            clock.set_rate(2.0, sample_rate);
            let clock_out = clock.tick();
            if let Some(c) = clock_out {
                if c {
                    tx.send(Tick { high: true }).unwrap();
                } else {
                    tx.send(Tick { high: false }).unwrap();
                }
            }

            thread::sleep(Duration::from_millis(period_ms));
        }
    });
    // TODO stop playing midi notes!
    handle
}

// gui takes ownership of the grid
fn run_gui(rx: std::sync::mpsc::Receiver<Tick>, mut grid: GridActivations) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };
    let mut selected_in_port = 0;

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        // set up midi list here
        let midi_in = MidiInput::new("midir input").unwrap();
        let in_ports = midi_in.ports();
        let in_port_names: Vec<String> = in_ports
            .iter()
            .map(|port| midi_in.port_name(port).unwrap())
            .collect();

        egui::CentralPanel::default().show(ctx, |ui| {
            match rx.try_recv() {
                Ok(Tick { high }) => {
                    print!("TICK {:?}\n", high);
                }
                _ => (),
            }

            ui.heading("Rho Sequencer");

            egui::ComboBox::from_label("Midi In Port")
                .selected_text(format!("{:?}", in_port_names[selected_in_port]))
                .show_ui(ui, |ui| {
                    let mut i = 0;
                    for port in in_port_names.iter() {
                        ui.selectable_value(&mut selected_in_port, i, port);
                        i += 1;
                    }
                });

            let mut density: usize = (grid.get_normalized_density() * 127.0) as usize;
            if ui
                .add(egui::Slider::new(&mut density, 0..=127).text("density"))
                .changed()
            {
                let norm_density = density as f32 / 127.0;
                grid.set_normalized_density(norm_density);
            }

            if ui.button("New Dist").clicked() {
                grid.create_new_distribution_given_active_steps();
            }

            for row in 0..NUM_ROWS {
                ui.horizontal(|ui| {
                    let mut row_length = grid.row_length(row);
                    if ui
                        .add(egui::Slider::new(&mut row_length, 2..=8).text("Row Length"))
                        .changed()
                    {
                        grid.set_row_length(row, row_length);
                    }
                    for step in 0..row_length {
                        let mut active = grid.get(row, step);
                        if toggle_ui(ui, &mut active).changed() {
                            grid.set(row, step, active);
                        }
                    }
                });
            }
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
            tx.send(MidiInMessage::NoteOn(note, velocity)).unwrap();
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
    Ok(port.clone())
}
