use imgui::*;

pub(crate) fn render_help(ui: &Ui<'_>) -> ImString {
    let title = ImString::new("Keyboard shortcuts");
    ui.popup_modal(&title).resizable(false).build(|| {
        ui.text("General:");
        ui.separator();

        ui.bullet_text(im_str!("ESC"));
        ui.same_line(90.0);
        ui.text("Enter/exit debugger");

        ui.bullet_text(im_str!("SPACE"));
        ui.same_line(90.0);
        ui.text("Start/stop emulation");

        ui.spacing();
        ui.spacing();
        ui.text("Disasm:");
        ui.separator();

        ui.bullet_text(im_str!("C"));
        ui.same_line(90.0);
        ui.text("Center view");

        ui.bullet_text(im_str!("S"));
        ui.same_line(90.0);
        ui.text("Step into");

        ui.bullet_text(im_str!("UP/DOWN"));
        ui.same_line(90.0);
        ui.text("Move selection");

        ui.bullet_text(im_str!("ENTER"));
        ui.same_line(90.0);
        ui.text("Run to selection");

        ui.spacing();
        ui.spacing();
        if ui.button(im_str!("Close"), (80.0, 30.0)) {
            ui.close_current_popup();
        }
    });
    title
}
