use emu::dbg;
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::snd::{SampleFormat, SndBufferMut};
use slog;
use slog::*;

struct FakeModel {
    curframe: i64,
}

impl dbg::DebuggerModel for FakeModel {
    fn all_cpus(&self) -> Vec<String> {
        Vec::new()
    }
    fn cycles(&self) -> i64 {
        0
    }
    fn frames(&self) -> i64 {
        self.curframe
    }
    fn trace_frame<SF: SampleFormat>(
        &mut self,
        screen: &mut GfxBufferMutLE<Rgb888>,
        sound: &mut SndBufferMut<SF>,
        tracer: &dbg::Tracer,
    ) -> dbg::Result<()> {
        self.curframe += 1;
        Ok(())
    }
    fn trace_step(&mut self, cpu_name: &str, tracer: &dbg::Tracer) -> dbg::Result<()> {
        self.curframe += 1;
        Ok(())
    }
    fn reset(&mut self, hard: bool) {}
    fn render_debug<'a, 'ui>(&mut self, dr: &dbg::DebuggerRenderer<'a, 'ui>) {}
}

fn fake_logging(logger: &slog::Logger) {
    info!(logger, "test info"; "a" => "b");
    warn!(logger, "test warn first"; "a" => "b");
    info!(logger, "test info"; "a" => "b");
    warn!(logger, "test warn second"; "a" => "b");
    error!(logger, "test error 1"; "a" => "b");
    warn!(logger, "test warn third"; "a" => "b");
    error!(logger, "test error 2"; "a" => "b");
    info!(logger, "test info"; "a" => "b");
}

fn main() {
    let sdl_context = sdl2::init().unwrap();
    let video = sdl_context.video().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();

    {
        let gl_attr = video.gl_attr();
        gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
        gl_attr.set_context_version(3, 0);
    }

    let window = video
        .window("logview-demo", 1000, 1000)
        .position_centered()
        .resizable()
        .opengl()
        .allow_highdpi()
        .build()
        .unwrap();

    let _gl_context = window
        .gl_create_context()
        .expect("Couldn't create GL context");
    gl::load_with(|s| video.gl_get_proc_address(s) as _);

    let mut model = FakeModel { curframe: 0 };

    let (logger, logpool) = dbg::new_debugger_logger();

    let mut dbgui = dbg::DebuggerUI::new(video, &window, &mut model, logpool);

    'running: loop {
        use sdl2::event::Event;
        use sdl2::keyboard::Keycode;

        for event in event_pump.poll_iter() {
            if dbgui.handle_event(&event) {
                continue;
            }

            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        fake_logging(&logger);
        dbgui.render(&window, &event_pump, &mut model);
        window.gl_swap_window();
        ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 30));
    }
}
