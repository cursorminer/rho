// import my lib here
extern crate shitquencer;

use eframe::egui;
use midir::MidiOutputConnection;
use shitquencer::clock::Clock;
use shitquencer::grid_activations::GridActivations;
use shitquencer::note_assigner::Note;
use shitquencer::rho_config::NUM_ROWS;
use shitquencer::Rho;
use shitquencer::Rows;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput};

// the gui sends messages to the clock thread
// it needs to send: row activations so that the steps are updated
enum MessageToRho {
    SetRowActivations { rows: [Vec<bool>; NUM_ROWS] },
}

// the clock thread sends messages to the gui
// it needs to send: note assignments, which steps are playing, so the gui can display playing state
enum MessageToGui {
    SetNotesForRows { notes: [Vec<usize>; NUM_ROWS] },
    SetStepPlayCounter { steps: [usize; NUM_ROWS] },
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    let rho = Arc::new(Mutex::new(Rho::new()));
    let grid = Arc::new(Mutex::new(GridActivations::new(4, 4)));

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // run gui in the main thread, it has a transmission channel
    let (txFromGuiToRho, rxToRhoFromGui) = mpsc::channel();
    let (txFromRhoToGui, rxToGuiFromRho) = mpsc::channel();

    let clock_thread_handle = run_clock(rxToRhoFromGui, txFromRhoToGui, rho, running);

    run_gui(txFromGuiToRho, rxToGuiFromRho, grid);

    // when gui stops, we stop the clock thread via this atomic bool
    r.store(false, Ordering::SeqCst);
    clock_thread_handle.join().unwrap();
}

fn run_clock(
    rx: std::sync::mpsc::Receiver<MessageToRho>,
    tx: std::sync::mpsc::Sender<MessageToGui>,
    rho: Arc<Mutex<Rho>>,
    running: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let clock_arc = Arc::new(Mutex::new(Clock::new()));
    let sample_rate = 32.0;
    let period_ms = (1000.0 / sample_rate) as u64;

    let midi_out_conn = get_midi_out_connection();
    if let Err(err) = midi_out_conn {
        println!("Error: {}", err);
        return thread::spawn(|| {});
    }
    let mut midi_out_conn = midi_out_conn.unwrap();

    // run a clock in another thread. This is equivalent of Audio
    // rho will be passed  in here
    let handle = thread::spawn(move || {
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

        // fake note on and trigger setting

        let mut rho = rho.lock().unwrap();
        rho.note_on(60, 100);
        rho.note_on(69, 100);

        while running.load(Ordering::SeqCst) {
            let mut clock = clock_arc.lock().unwrap();

            // process messages from GUI
            match rx.try_recv() {
                Ok(MessageToRho::SetRowActivations { rows }) => {
                    print!("recieved rows\n");
                }
                _ => (),
            }

            clock.set_rate(2.0, sample_rate);
            let clock_out = clock.tick();
            if let Some(c) = clock_out {
                if c {
                    // clock high
                    // process messages from UI

                    // this doesn't work because the above will keep processing messages till the end...?
                    let starting_notes = rho.on_clock_high();
                    // play midi notes here
                    for note in starting_notes {
                        send_midi(note, true);
                    }
                } else {
                    // clock low
                    let finishing_notes = rho.on_clock_low();
                    // stop midi notes here
                    for note in finishing_notes {
                        send_midi(note, false);
                    }
                }
            }
            // send messages back to GUI if needed
            // let rows = rho.get_cloned_row_loopers();
            // let _ = tx.send(MessageToGui::SetRows { row_states: rows });

            thread::sleep(Duration::from_millis(period_ms));
        }
    });
    // TODO stop playing midi notes!
    handle
}

fn run_gui(
    tx: std::sync::mpsc::Sender<MessageToRho>,
    rx: std::sync::mpsc::Receiver<MessageToGui>,
    grid: Arc<Mutex<GridActivations>>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };

    let mut density: i32 = 0;
    let mut row_length: usize = 0;

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut grid = grid.lock().unwrap();

            // process messages from Rho
            match rx.try_recv() {
                Ok(MessageToGui::SetNotesForRows { notes }) => {
                    print!("recieved notes {:?}\n", notes);
                }
                _ => (),
            }

            ui.heading("My egui Application");
            if ui
                .add(egui::Slider::new(&mut density, 0..=127).text("density"))
                .changed()
            {
                let norm_density = density as f32 / 127.0;
                grid.set_normalized_density(norm_density);
            }

            if ui
                .add(egui::Slider::new(&mut row_length, 2..=8).text("Row Length"))
                .changed()
            {}

            let row_length = 8;

            // add a horizontal row of checkboxes
            ui.horizontal(|ui| {
                for j in 0..row_length {
                    ui.checkbox(&mut false, "");
                }
            });

            let _ = tx.send(MessageToRho::SetRowActivations {
                rows: grid.get_row_activations(),
            });
        });
    });
}

fn get_midi_in_connection() -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir input")?;
    midi_in.ignore(Ignore::None);
    let in_port = select_port(&midi_in, "input")?;

    let conn_in = midi_in.connect(&in_port, "midir-input", move |_stamp, _message, _| {}, ())?;
    Ok(conn_in)
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

fn on_midi_in(rho: &mut Rho, _stamp: u64, message: &[u8]) {
    //println!("{}: {:?} (len = {})", stamp, message, message.len());

    const MSG_NOTE: u8 = 144;
    const MSG_NOTE_2: u8 = 145;
    const MSG_NOTE_OFF: u8 = 129;

    let status = message[0];
    let note = message[1];
    let velocity = message[2];

    if status == MSG_NOTE || status == MSG_NOTE_2 {
        if velocity > 0 {
            rho.note_on(note.into(), velocity.into());
        } else {
            rho.note_off(note.into());
        }
    } else if status == MSG_NOTE_OFF {
        rho.note_off(note.into());
    }
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
