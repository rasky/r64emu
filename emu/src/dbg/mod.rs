extern crate gl;
extern crate imgui;
extern crate imgui_opengl_renderer;
extern crate imgui_sdl2;
extern crate imgui_sys;
extern crate sdl2;
use super::gfx::GfxBufferLE;
use super::hw::glutils::{ColorForTexture, Texture};

use self::imgui::*;
use self::imgui_opengl_renderer::Renderer;
use self::imgui_sdl2::ImguiSdl2;
mod uisupport;

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
    tex_screen: Texture,
}

impl Debugger {
    pub(crate) fn new(video: sdl2::VideoSubsystem) -> Debugger {
        let hidpi_factor = 1.0;

        let mut imgui = ImGui::init();
        imgui.set_ini_filename(Some(im_str!("debug.ini").to_owned()));

        let imgui_sdl2 = ImguiSdl2::new(&mut imgui);
        let backend = Renderer::new(&mut imgui, move |s| video.gl_get_proc_address(s) as _);

        Debugger {
            imgui: Rc::new(RefCell::new(imgui)),
            imgui_sdl2,
            backend,
            hidpi_factor,
            tex_screen: Texture::new(),
        }
    }

    pub(crate) fn handle_event(&mut self, event: &sdl2::event::Event) {
        let imgui = self.imgui.clone();
        let mut imgui = imgui.borrow_mut();
        self.imgui_sdl2.handle_event(&mut imgui, &event);
    }

    pub(crate) fn render_frame<T: DebuggerModel, CF: ColorForTexture>(
        &mut self,
        window: &sdl2::video::Window,
        event_pump: &sdl2::EventPump,
        model: &mut T,
        screen: &GfxBufferLE<CF>,
        tex_id: usize,
    ) {
        let imgui = self.imgui.clone();
        let mut imgui = imgui.borrow_mut();
        let ui = self.imgui_sdl2.frame(&window, &mut imgui, &event_pump);

        self.render_main(&ui, screen, tex_id);
        //ui.show_demo_window(&mut true);

        {
            let dr = DebuggerRenderer { ui: &ui };
            model.render_debug(dr);
        }

        // Actually flush commands batched in imgui to OpenGL
        unsafe {
            gl::ClearColor(0.45, 0.55, 0.60, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        self.backend.render(ui);
    }

    fn render_main<'ui, CF: ColorForTexture>(
        &mut self,
        ui: &Ui<'ui>,
        screen: &GfxBufferLE<CF>,
        tex_id: usize,
    ) {
        ui.main_menu_bar(|| {
            ui.menu(im_str!("Emulation")).build(|| {
                ui.menu_item(im_str!("Reset")).build();
            })
        });

        self.tex_screen.copy_from_buffer(screen);
        ui.window(im_str!("Screen"))
            .size((320.0, 240.0), ImGuiCond::FirstUseEver)
            .build(|| {
                let tsid = self.tex_screen.id();
                let reg = ui.get_content_region_avail();
                let image = uisupport::Image::new(ui, tsid.into(), reg);
                image.build();
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
