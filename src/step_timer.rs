// Works out when steps start and when their notes stop within a block, from the host's song
// position in beats. Doesn't know about MIDI or samples so it can be tested on its own.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepEvent {
    // stop the notes from the previous step
    NotesOff { beat: f64 },
    // play the step with this index, steps are counted from the start of the song
    Step { index: i64, beat: f64 },
}

// how far the song position can drift between blocks before we treat it as a jump
const JUMP_TOLERANCE_BEATS: f64 = 1.0e-3;
// absorbs rounding in the host's song position so a step on a block boundary isn't missed
const BEAT_EPSILON: f64 = 1.0e-6;

#[derive(Debug, Default)]
pub struct StepTimer {
    // when the notes from the last step should stop
    notes_off_beat: Option<f64>,
    // the beat of the last step played, so a step on a block boundary isn't played twice
    last_step_beat: Option<f64>,
    // where we expect the next block to start
    expected_start_beat: Option<f64>,
}

impl StepTimer {
    // emits the events for the block from start_beat up to (not including) end_beat, in order.
    // gate is the fraction of a step that notes last for, from 0 to 1.
    pub fn process_block(
        &mut self,
        start_beat: f64,
        end_beat: f64,
        step_length: f64,
        gate: f64,
        mut emit: impl FnMut(StepEvent),
    ) {
        let jumped = self.expected_start_beat.map_or(false, |expected| {
            (start_beat - expected).abs() > JUMP_TOLERANCE_BEATS
        });
        self.expected_start_beat = Some(end_beat);

        if jumped {
            self.last_step_beat = None;
            // the pending note off might now be in the past or far in the future
            if self.notes_off_beat.take().is_some() {
                emit(StepEvent::NotesOff { beat: start_beat });
            }
        }

        let mut next_index = (start_beat / step_length - BEAT_EPSILON).ceil() as i64;
        loop {
            let step_beat = next_index as f64 * step_length;

            if let Some(off_beat) = self.notes_off_beat {
                if off_beat < end_beat && off_beat <= step_beat {
                    emit(StepEvent::NotesOff { beat: off_beat });
                    self.notes_off_beat = None;
                    continue;
                }
            }

            if step_beat >= end_beat {
                break;
            }

            let already_played = self
                .last_step_beat
                .map_or(false, |last| step_beat <= last + BEAT_EPSILON);
            if !already_played {
                // with a long gate the previous notes might still be on
                if self.notes_off_beat.take().is_some() {
                    emit(StepEvent::NotesOff { beat: step_beat });
                }
                emit(StepEvent::Step {
                    index: next_index,
                    beat: step_beat,
                });
                self.notes_off_beat = Some(step_beat + step_length * gate);
                self.last_step_beat = Some(step_beat);
            }
            next_index += 1;
        }
    }

    // the transport stopped, emits a note off if notes are still on
    pub fn stop(&mut self, mut emit: impl FnMut(StepEvent)) {
        if self.notes_off_beat.take().is_some() {
            emit(StepEvent::NotesOff { beat: 0.0 });
        }
        self.last_step_beat = None;
        self.expected_start_beat = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use StepEvent::*;

    fn run(
        timer: &mut StepTimer,
        start: f64,
        end: f64,
        step_length: f64,
        gate: f64,
    ) -> Vec<StepEvent> {
        let mut events = vec![];
        timer.process_block(start, end, step_length, gate, |e| events.push(e));
        events
    }

    #[test]
    fn steps_and_note_offs_in_one_block() {
        let mut timer = StepTimer::default();
        let events = run(&mut timer, 0.0, 1.0, 0.25, 0.5);
        assert_eq!(
            events,
            vec![
                Step {
                    index: 0,
                    beat: 0.0
                },
                NotesOff { beat: 0.125 },
                Step {
                    index: 1,
                    beat: 0.25
                },
                NotesOff { beat: 0.375 },
                Step {
                    index: 2,
                    beat: 0.5
                },
                NotesOff { beat: 0.625 },
                Step {
                    index: 3,
                    beat: 0.75
                },
                NotesOff { beat: 0.875 },
            ]
        );
    }

    #[test]
    fn note_off_carries_over_to_next_block() {
        let mut timer = StepTimer::default();
        assert_eq!(
            run(&mut timer, 0.0, 0.1, 0.25, 0.5),
            vec![Step {
                index: 0,
                beat: 0.0
            }]
        );
        assert_eq!(
            run(&mut timer, 0.1, 0.2, 0.25, 0.5),
            vec![NotesOff { beat: 0.125 }]
        );
        assert_eq!(
            run(&mut timer, 0.2, 0.3, 0.25, 0.5),
            vec![Step {
                index: 1,
                beat: 0.25
            }]
        );
    }

    #[test]
    fn step_on_block_boundary_plays_once_despite_rounding() {
        let mut timer = StepTimer::default();
        // this block's end lands just past the step
        assert_eq!(
            run(&mut timer, 0.0, 0.2500000001, 0.25, 0.1),
            vec![
                Step {
                    index: 0,
                    beat: 0.0
                },
                NotesOff { beat: 0.025 },
                Step {
                    index: 1,
                    beat: 0.25
                },
            ]
        );
        // and the host says the next block starts just before it
        assert_eq!(
            run(&mut timer, 0.2499999999, 0.3, 0.25, 0.1),
            vec![NotesOff { beat: 0.275 }]
        );
    }

    #[test]
    fn step_just_before_block_start_is_not_missed() {
        let mut timer = StepTimer::default();
        run(&mut timer, 0.0, 0.2499999999, 0.25, 0.5);
        // the host's position is a hair past the step
        assert_eq!(
            run(&mut timer, 0.2500000001, 0.3, 0.25, 0.5),
            vec![Step {
                index: 1,
                beat: 0.25
            }]
        );
    }

    #[test]
    fn full_gate_turns_notes_off_before_next_step() {
        let mut timer = StepTimer::default();
        assert_eq!(
            run(&mut timer, 0.0, 0.5, 0.25, 1.0),
            vec![
                Step {
                    index: 0,
                    beat: 0.0
                },
                NotesOff { beat: 0.25 },
                Step {
                    index: 1,
                    beat: 0.25
                },
            ]
        );
    }

    #[test]
    fn jump_back_turns_notes_off_and_replays_steps() {
        let mut timer = StepTimer::default();
        run(&mut timer, 3.9, 4.1, 0.25, 0.5);
        // loop back to the start
        assert_eq!(
            run(&mut timer, 0.0, 0.1, 0.25, 0.5),
            vec![
                NotesOff { beat: 0.0 },
                Step {
                    index: 0,
                    beat: 0.0
                },
            ]
        );
    }

    #[test]
    fn stop_turns_notes_off_once() {
        let mut timer = StepTimer::default();
        run(&mut timer, 0.0, 0.1, 0.25, 0.5);

        let mut events = vec![];
        timer.stop(|e| events.push(e));
        timer.stop(|e| events.push(e));
        assert_eq!(events, vec![NotesOff { beat: 0.0 }]);

        // starting again plays the step at the start position
        assert_eq!(
            run(&mut timer, 0.0, 0.1, 0.25, 0.5),
            vec![Step {
                index: 0,
                beat: 0.0
            }]
        );
    }

    #[test]
    fn negative_song_position() {
        let mut timer = StepTimer::default();
        assert_eq!(
            run(&mut timer, -0.3, -0.2, 0.25, 0.5),
            vec![Step {
                index: -1,
                beat: -0.25
            }]
        );
    }
}
