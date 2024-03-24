// import my lib here
extern crate shitquencer;

use eframe::egui;
use shitquencer::Rho;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

use midir::{Ignore, MidiIO, MidiInput, MidiOutput};

fn main() {
    let rho = Arc::new(Mutex::new(Rho::new()));

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    match run(rho) {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

#[cfg(not(target_arch = "wasm32"))] // conn_out is not `Send` in Web MIDI, which means it cannot be passed to connect
fn run(rho: Arc<Mutex<Rho>>) -> Result<(), Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir forwarding input")?;
    midi_in.ignore(Ignore::None);
    let midi_out = MidiOutput::new("midir forwarding output")?;

    let in_port = select_port(&midi_in, "input")?;
    println!();
    let out_port = select_port(&midi_out, "output")?;

    println!("\nOpening connections");
    // let in_port_name = midi_in.port_name(&in_port)?;
    // let out_port_name = midi_out.port_name(&out_port)?;

    let mut conn_out = midi_out.connect(&out_port, "midir-forward")?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };

    let mut density: i32 = 0;

    let _ = eframe::run_simple_native("My egui App", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Define a new scope in which the closure `play_note` borrows conn_out, so it can be called easily
            let mut play_note = |note: u8, duration: u64| {
                const NOTE_ON_MSG: u8 = 0x90;
                const NOTE_OFF_MSG: u8 = 0x80;
                const VELOCITY: u8 = 0x64;
                // We're ignoring errors in here
                let _ = conn_out.send(&[NOTE_ON_MSG, note, VELOCITY]);
                sleep(Duration::from_millis(duration * 150));
                let _ = conn_out.send(&[NOTE_OFF_MSG, note, VELOCITY]);
            };

            ui.heading("My egui Application");
            ui.add(egui::Slider::new(&mut density, 0..=127).text("density"));
            if ui.button("Squanchrement").clicked() {
                // output a midi note
                play_note(69, 4);
            }

            let mut rho = rho.lock().unwrap();
            let norm_density = (density as f32) / 127.0;
            if norm_density != rho.get_density() {
                rho.set_density(norm_density);
            }
        });
    });

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let _conn_in = midi_in.connect(&in_port, "midir-forward", move |stamp, message, _| {}, ())?;

    let mut input = String::new();
    stdin().read_line(&mut input)?; // wait for next enter key press

    println!("Closing connections");
    Ok(())
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

#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), Box<dyn Error>> {
    println!("test_forward cannot run on Web MIDI");
    Ok(())
}
