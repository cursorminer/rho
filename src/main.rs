// import my lib here
extern crate rho;

use rho::clock_runner::*;
use rho::gui_runner::*;
use rho::midi_helpers::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    // channel from clock to gui
    let (tx, rx) = mpsc::channel();

    // channel from midi in to rho
    let (tx_midi_in, rx_midi_in) = mpsc::channel();

    // channel from gui to rho
    let (tx_gui, rx_gui) = mpsc::channel();

    // set up midi in connection
    let _conn_in = set_up_midi_in_connection(tx_midi_in);

    let clock_thread_handle = run_clock(tx, running, rx_midi_in, rx_gui);

    // run gui in the main thread, it has a recieve channel from the clock
    run_gui(rx, tx_gui);

    // when gui stops, we stop the clock thread via this atomic bool
    r.store(false, Ordering::SeqCst);
    clock_thread_handle.join().unwrap();
}
