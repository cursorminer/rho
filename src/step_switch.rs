use eframe::egui;
use egui::Color32;

pub fn step_switch_ui(ui: &mut egui::Ui, on: &mut bool, is_playing: bool) -> egui::Response {
    let desired_height = ui.spacing().interact_size.y * 2.0;
    // use all available width
    let desired_width = ui.available_width();
    let desired_size = egui::vec2(desired_width, desired_height);

    // 2. Allocating space:
    // This is where we get a region of the screen assigned.
    // We also tell the Ui to sense clicks in the allocated region.
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed(); // report back that the value changed
    }

    // Attach some meta-data to the response which can be used by screen readers:
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

    // 4. Paint!
    // Make sure we need to paint:
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);

        // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
        let rect = rect.expand(visuals.expansion);
        let radius = 0.1 * rect.height();
        let playing_color = if *on {
            Color32::WHITE
        } else {
            Color32::from_rgb(113, 175, 175)
        };
        let on_color = Color32::from_rgb(240, 186, 113);
        let off_color = Color32::from_rgb(58, 58, 58);

        let fill_color = Color32::from_rgb(
            egui::lerp((off_color.r() as f32)..=(on_color.r() as f32), how_on) as u8,
            egui::lerp((off_color.g() as f32)..=(on_color.g() as f32), how_on) as u8,
            egui::lerp((off_color.b() as f32)..=(on_color.b() as f32), how_on) as u8,
        );
        // if playing, paint a border around the switch
        if is_playing {
            ui.painter()
                .rect(rect, radius, playing_color, visuals.bg_stroke);
            let inner_rect = rect.expand(-radius / 2.0); // make room for the border
            ui.painter()
                .rect(inner_rect, radius, fill_color, visuals.bg_stroke);
        } else {
            ui.painter()
                .rect(rect, radius, fill_color, visuals.bg_stroke);
        }
    }

    // All done! Return the interaction response so the user can check what happened
    // (hovered, clicked, ...) and maybe show a tooltip:
    response
}

// A wrapper that allows the more idiomatic usage pattern: `ui.add(toggle(&mut my_bool))`
/// iOS-style toggle switch.
///
/// ## Example:
/// ``` ignore
/// ui.add(toggle(&mut my_bool));
/// ```
pub fn step_switch(on: &mut bool, is_playing: bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| step_switch_ui(ui, on, is_playing)
}

pub fn url_to_file_source_code() -> String {
    format!("https://github.com/emilk/egui/blob/master/{}", file!())
}
