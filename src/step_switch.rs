//! Source code example of how to create your own widget.
//! This is meant to be read as a tutorial, hence the plethora of comments.
use eframe::egui;
use egui::Color32;

/// iOS-style toggle switch:
///
/// ``` text
///      _____________
///     /       /.....\
///    |       |.......|
///     \_______\_____/
/// ```
///
/// ## Example:
/// ``` ignore
/// toggle_ui(ui, &mut my_bool);
/// ```
pub fn step_switch_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    // Widget code can be broken up in four steps:
    //  1. Decide a size for the widget
    //  2. Allocate space for it
    //  3. Handle interactions with the widget (if any)
    //  4. Paint the widget

    // 1. Deciding widget size:
    // You can query the `ui` how much space is available,
    // but in this example we have a fixed size widget based on the height of a standard button:
    let desired_height = ui.spacing().interact_size.y * 2.0;
    // use all available width
    let desired_width = ui.available_width();
    let desired_size = egui::vec2(desired_width, desired_height);

    // 2. Allocating space:
    // This is where we get a region of the screen assigned.
    // We also tell the Ui to sense clicks in the allocated region.
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    // 3. Interact: Time to check for clicks!
    if response.clicked() {
        *on = !*on;
        response.mark_changed(); // report back that the value changed
    }

    // Attach some meta-data to the response which can be used by screen readers:
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

    // 4. Paint!
    // Make sure we need to paint:
    if ui.is_rect_visible(rect) {
        // Let's ask for a simple animation from egui.
        // egui keeps track of changes in the boolean associated with the id and
        // returns an animated value in the 0-1 range for how much "on" we are.
        let how_on = ui.ctx().animate_bool(response.id, *on);
        // We will follow the current style by asking
        // "how should something that is being interacted with be painted?".
        // This will, for instance, give us different colors when the widget is hovered or clicked.
        let visuals = ui.style().interact_selectable(&response, *on);
        // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
        let rect = rect.expand(visuals.expansion);
        let radius = 0.1 * rect.height();
        let on_color = Color32::YELLOW;
        let off_color = Color32::BLACK;

        let fill_color = egui::Color32::from_rgb(
            egui::lerp((off_color.r() as f32)..=(on_color.r() as f32), how_on) as u8,
            egui::lerp((off_color.g() as f32)..=(on_color.g() as f32), how_on) as u8,
            egui::lerp((off_color.b() as f32)..=(on_color.b() as f32), how_on) as u8,
        );
        ui.painter()
            .rect(rect, radius, fill_color, visuals.bg_stroke);
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
pub fn step_switch(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| step_switch_ui(ui, on)
}

pub fn url_to_file_source_code() -> String {
    format!("https://github.com/emilk/egui/blob/master/{}", file!())
}
