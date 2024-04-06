// midi helper functions

use crate::messages::*;
use midir::{Ignore, MidiIO, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::mpsc::Sender;

pub fn set_up_midi_in_connection(
    tx: Sender<MidiInMessage>,
) -> Result<MidiInputConnection<Sender<MidiInMessage>>, Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir input")?;
    midi_in.ignore(Ignore::None);
    let in_port = select_port(&midi_in, "input", 0)?;

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
pub fn on_midi_in(tx: &mut std::sync::mpsc::Sender<MidiInMessage>, _stamp: u64, message: &[u8]) {
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

pub fn get_midi_out_connection(port_index: usize) -> Result<MidiOutputConnection, Box<dyn Error>> {
    let midi_out = MidiOutput::new("midir output")?;

    println!();
    let out_port = select_port(&midi_out, "output", port_index)?;

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

pub fn select_port<T: MidiIO>(
    midi_io: &T,
    descr: &str,
    port_index: usize,
) -> Result<T::Port, Box<dyn Error>> {
    let midi_ports = midi_io.ports();
    let port = midi_ports.get(port_index).ok_or("Invalid port number")?;
    Ok(port.clone())
}
