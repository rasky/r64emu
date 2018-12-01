extern crate imgui;
extern crate imgui_opengl_renderer;
extern crate imgui_sdl2;
extern crate sdl2;

use self::imgui::*;
use self::imgui_opengl_renderer::Renderer;
use self::imgui_sdl2::ImguiSdl2;

mod regview;
pub use self::regview::*;

use std::cell::RefCell;
use std::rc::Rc;

pub trait DebuggerModel {
    fn render_debug<'a, 'ui>(&mut self, dr: DebuggerRenderer<'a, 'ui>);
}

pub struct Debugger {
    imgui: Rc<RefCell<ImGui>>,
    imgui_sdl2: ImguiSdl2,
    backend: Renderer,
    hidpi_factor: f32,
}

impl Debugger {
    pub(crate) fn new(video: sdl2::VideoSubsystem) -> Debugger {
        let hidpi_factor = 1.0;

        let mut imgui = ImGui::init();
        imgui.set_ini_filename(None);

        let imgui_sdl2 = ImguiSdl2::new(&mut imgui);
        let backend = Renderer::new(&mut imgui, move |s| video.gl_get_proc_address(s) as _);

        Debugger {
            imgui: Rc::new(RefCell::new(imgui)),
            imgui_sdl2,
            backend,
            hidpi_factor,
        }
    }

    pub(crate) fn handle_event(&mut self, event: &sdl2::event::Event) {
        let imgui = self.imgui.clone();
        let mut imgui = imgui.borrow_mut();
        self.imgui_sdl2.handle_event(&mut imgui, &event);
    }

    pub(crate) fn render_frame<T: DebuggerModel>(
        &mut self,
        window: &sdl2::video::Window,
        event_pump: &sdl2::EventPump,
        model: &mut T,
    ) {
        let imgui = self.imgui.clone();
        let mut imgui = imgui.borrow_mut();

        let ui = self.imgui_sdl2.frame(&window, &mut imgui, &event_pump);

        {
            let dr = DebuggerRenderer { ui: &ui };
            model.render_debug(dr);
        }

        self.backend.render(ui);
    }

    fn draw_frame<'ui>(&mut self, ui: &Ui<'ui>) {
        ui.window(im_str!("Hello world"))
            .size((300.0, 100.0), ImGuiCond::FirstUseEver)
            .build(|| {
                ui.text(im_str!("Hello world!"));
                //ui.text(im_str!("こんにちは世界！"));
                ui.text(im_str!("This...is...imgui-rs!"));
                ui.separator();
                let mouse_pos = ui.imgui().mouse_pos();
                ui.text(im_str!(
                    "Mouse Position: ({:.1},{:.1})",
                    mouse_pos.0,
                    mouse_pos.1
                ));
            });
    }
}

pub struct DebuggerRenderer<'a, 'ui> {
    ui: &'a Ui<'ui>,
}

impl<'a, 'ui> DebuggerRenderer<'a, 'ui> {
    pub fn render_regview<RV: RegisterView>(&self, v: &mut RV) {
        render_regview(self.ui, v)
    }
}
