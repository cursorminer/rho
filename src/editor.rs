use crate::grid_activations::GridActivations;
use crate::rho_config::NUM_ROWS;
use crate::shared_state::{RowNotes, SharedState};
use crate::step_switch::step_switch;
use crate::RhoParams;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, widgets, EguiState};
use std::sync::Arc;

pub fn default_state() -> Arc<EguiState> {
    EguiState::from_size(600, 340)
}

pub fn create(params: Arc<RhoParams>, shared: Arc<SharedState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        (),
        |_, _| {},
        move |ctx, setter, _| {
            let mut grid = params.grid.lock().unwrap();
            let playing_steps = shared.playing_steps();
            let notes_for_rows = shared.notes_for_rows();
            let mut rows_changed = false;

            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Rho");
                ui.add_space(10.0);

                // measured once, a row that overflows would widen the ones after it
                let width = ui.available_width();

                // row 0 gets the lowest note so it goes at the bottom
                for row in (0..NUM_ROWS).rev() {
                    rows_changed |= draw_row(
                        ui,
                        width,
                        &mut grid,
                        row,
                        playing_steps[row],
                        &notes_for_rows[row],
                    );
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let mut density = (grid.get_normalized_density() * 127.0) as usize;
                    if ui
                        .add(egui::Slider::new(&mut density, 0..=127).text("density"))
                        .changed()
                    {
                        grid.set_normalized_density(density as f32 / 127.0);
                        rows_changed = true;
                    }

                    if ui.button("New Dist").clicked() {
                        grid.create_new_distribution_given_active_steps();
                        rows_changed = true;
                    }

                    let mut hold = params.hold.value();
                    if ui.checkbox(&mut hold, "Hold").changed() {
                        setter.begin_set_parameter(&params.hold);
                        setter.set_parameter(&params.hold, hold);
                        setter.end_set_parameter(&params.hold);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Step Length");
                    ui.add(widgets::ParamSlider::for_param(&params.step_length, setter));
                    ui.label("Gate");
                    ui.add(widgets::ParamSlider::for_param(&params.gate, setter));
                });
            });

            if rows_changed {
                shared.publish_rows(&grid);
            }

            // keep the playing step moving
            ctx.request_repaint();
        },
    )
}

// a row of steps with its notes on the left and its length on the right. Returns true if the
// row changed.
fn draw_row(
    ui: &mut egui::Ui,
    width: f32,
    grid: &mut GridActivations,
    row: usize,
    playing_step: Option<usize>,
    notes: &RowNotes,
) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        let spacing = ui.spacing().item_spacing;
        let fixed_left_width = 100.0;
        let fixed_right_width = 40.0;

        let note_names: Vec<String> = notes.iter().map(|note| note.to_string()).collect();
        ui.add_sized(
            [fixed_left_width, 50.0],
            egui::Label::new(note_names.join(" ")),
        );

        let mut row_length = grid.row_length(row);
        let steps_width =
            width - fixed_left_width - fixed_right_width - spacing.x * (row_length + 1) as f32;
        let step_width = steps_width / row_length as f32;

        for step in 0..row_length {
            let mut active = grid.get(row, step);
            let is_playing = playing_step == Some(step);
            if ui
                .add_sized([step_width, 50.0], step_switch(&mut active, is_playing))
                .changed()
            {
                grid.set(row, step, active);
                changed = true;
            }
        }

        if ui
            .add_sized(
                [fixed_right_width, 50.0],
                egui::DragValue::new(&mut row_length).range(2..=8),
            )
            .on_hover_text("Row length")
            .changed()
        {
            grid.set_row_length(row, row_length);
            changed = true;
        }
    });

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 600.0;

    // draws a row headless for each batch of input events, returns whether any frame changed it
    fn draw_frames(grid: &mut GridActivations, frames: Vec<Vec<egui::Event>>) -> bool {
        let ctx = egui::Context::default();
        let mut changed = false;
        for events in frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(WIDTH, 100.0),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        changed |= draw_row(ui, WIDTH, grid, 0, None, &RowNotes::new());
                    });
            });
        }
        changed
    }

    fn click(x: f32, y: f32) -> Vec<Vec<egui::Event>> {
        let pos = egui::pos2(x, y);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ]
    }

    #[test]
    fn clicking_a_step_toggles_it() {
        let mut grid = GridActivations::new(NUM_ROWS, 4);

        // just right of the notes label, so the first step
        assert!(draw_frames(&mut grid, click(130.0, 25.0)));
        assert_eq!(grid.get_row(0), vec![true, false, false, false]);

        assert!(draw_frames(&mut grid, click(130.0, 25.0)));
        assert_eq!(grid.get_row(0), vec![false; 4]);
    }

    #[test]
    fn clicking_the_notes_label_changes_nothing() {
        let mut grid = GridActivations::new(NUM_ROWS, 4);
        assert!(!draw_frames(&mut grid, click(50.0, 25.0)));
        assert_eq!(grid.get_row(0), vec![false; 4]);
    }

    #[test]
    fn steps_fit_beside_the_length_box() {
        // the last step of the longest row is still clickable, nothing pushed off the edge
        let mut grid = GridActivations::new(NUM_ROWS, 4);
        grid.set_row_length(0, 8);
        assert!(draw_frames(&mut grid, click(WIDTH - 60.0, 25.0)));
        assert_eq!(grid.get_row(0)[7], true);
    }
}
