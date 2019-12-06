use super::UiCtx;
use imgui::*;
use imgui_sys::*;
use std::time::Duration;

// Rendere the help tooltip showing keyboard shortcuts
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
        if ui.button(&im_str!("Close"), [80.0, 30.0]) {
            ui.close_current_popup();
        }
    });
    title
}

// Render the flash messages
pub(crate) fn render_flash_msgs(ui: &Ui<'_>, ctx: &mut UiCtx) {
    if ctx.flash_msg.is_none() {
        return;
    }
    let (msg, when) = ctx.flash_msg.as_ref().unwrap();
    const CORNER: usize = 1; // top-right
    const DISTANCE_X: f32 = 10.0;
    const DISTANCE_Y: f32 = 25.0;

    let disp_size = ui.io().display_size;
    let wpos_x = if CORNER & 1 != 0 {
        disp_size[0] - DISTANCE_X
    } else {
        DISTANCE_X
    };
    let wpos_y = if CORNER & 2 != 0 {
        disp_size[1] - DISTANCE_Y
    } else {
        DISTANCE_Y
    };
    let pivot_x = if CORNER & 1 != 0 { 1.0 } else { 0.0 };
    let pivot_y = if CORNER & 2 != 0 { 1.0 } else { 0.0 };

    unsafe {
        igSetNextWindowPos(
            (wpos_x, wpos_y).into(),
            ImGuiCond_Always as i32,
            (pivot_x, pivot_y).into(),
        );
        igSetNextWindowBgAlpha(0.5);
    }
    Window::new(&im_str!(""))
        .resizable(false)
        .movable(false)
        .collapsible(false)
        .title_bar(false)
        .save_settings(false)
        .no_nav()
        .mouse_inputs(false)
        .build(ui, || {
            ui.text(&im_str!("{}", msg));
        });

    if when.elapsed() > Duration::from_secs(4) {
        ctx.flash_msg = None;
    }
}
