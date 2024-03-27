// import my lib here
extern crate shitquencer;

use eframe::egui;
use midir::MidiOutputConnection;
use shitquencer::clock::Clock;
use shitquencer::note_assigner::Note;
use shitquencer::Rho;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput};

enum MessageToRho {
    SetDensity { density: f32 },
    SetRowLength { row: usize, length: usize },
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let rho = Arc::new(Mutex::new(Rho::new()));

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // run gui in the main thread, it has a transmission channel
    let (tx, rx) = mpsc::channel();

    let clock_thread_handle = run_clock(rx, rho, running);

    run_gui(tx);

    // when gui stops, we stop the clock thread via this atomic bool
    r.store(false, Ordering::SeqCst);
    clock_thread_handle.join().unwrap();
}

fn run_clock(
    rx: std::sync::mpsc::Receiver<MessageToRho>,
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
        rho.set_density(0.9);

        while running.load(Ordering::SeqCst) {
            let mut clock = clock_arc.lock().unwrap();
            match rx.try_recv() {
                Ok(MessageToRho::SetDensity { density }) => {
                    print!("recieved density {}\n", density);
                    rho.set_density(density);
                }
                Ok(MessageToRho::SetRowLength { row, length }) => {
                    print!("recieved row {}, length {}\n", row, length);
                    rho.set_row_length(row, length);
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
            thread::sleep(Duration::from_millis(period_ms));
        }
    });
    handle
}

fn run_gui(tx: std::sync::mpsc::Sender<MessageToRho>) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };

    let mut density: i32 = 0;
    let mut row_length: usize = 0;

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("My egui Application");
            if ui
                .add(egui::Slider::new(&mut density, 0..=127).text("density"))
                .changed()
            {
                let norm_density = density as f32 / 127.0;
                let _ = tx.send(MessageToRho::SetDensity {
                    density: norm_density,
                });
            }

            if ui
                .add(egui::Slider::new(&mut row_length, 2..=8).text("Row Length"))
                .changed()
            {
                let _ = tx.send(MessageToRho::SetRowLength {
                    row: (1),
                    length: row_length,
                });
            }
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
