// module with the function that runs the clock thread

use crate::clock::Clock;
use crate::messages::*;
use crate::midi_helpers::*;
use crate::note_assigner::Note;
use crate::rho_config::NUM_ROWS;
use crate::Rho;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn run_clock(
    tx: std::sync::mpsc::Sender<MessageToGui>,
    running: Arc<AtomicBool>,
    rx_midi_in: std::sync::mpsc::Receiver<MidiInMessage>,
    rx_gui: std::sync::mpsc::Receiver<MessageGuiToRho>,
) -> thread::JoinHandle<()> {
    let mut rho = Rho::new();

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
                    tx.send(MessageToGui::Tick {
                        playing_steps: rho.get_playing_steps(),
                    })
                    .unwrap();
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
