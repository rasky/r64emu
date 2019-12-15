use emu::dbg;
use emu::gfx::{GfxBufferMutLE, Rgb888};
use emu::log;
use emu::snd::{SampleFormat, SndBufferMut};
use slog;
use slog::*;

struct FakeModel {
    curframe: i64,
    ram: Vec<u8>,
    rom: Vec<u8>,
}

impl dbg::MemoryView for FakeModel {
    fn name(&self) -> &str {
        return "Fake";
    }
    fn banks(&self) -> Vec<dbg::MemoryBank> {
        vec![
            dbg::MemoryBank::new("RAM", 0, 1024 * 1024 - 1, true),
            dbg::MemoryBank::new("ROM", 0xFFFF_0000, 0xFFFF_0000 + 64 * 1024 - 1, false),
        ]
    }

    fn mem_slice<'a>(&'a self, bank_idx: usize, start: u64, end: u64) -> &'a [u8] {
        match bank_idx {
            0 => &self.ram[start as usize..=end as usize],
            1 => &self.rom[(start - 0xFFFF_0000) as usize..=(end - 0xFFFF_0000) as usize],
            _ => unreachable!(),
        }
    }

    fn mem_slice_mut<'a>(&'a mut self, bank_idx: usize, start: u64, end: u64) -> &'a mut [u8] {
        match bank_idx {
            0 => &mut self.ram[start as usize..=end as usize],
            1 => &mut self.rom[(start - 0xFFFF_0000) as usize..=(end - 0xFFFF_0000) as usize],
            _ => unreachable!(),
        }
    }
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

    fn render_debug<'a, 'ui>(&mut self, dr: &dbg::DebuggerRenderer<'a, 'ui>) {
        dr.render_memoryview(self);
    }
}

fn fake_logging(logger: &slog::Logger, cnt: u32) {
    info!(logger, "test info"; "a" => "b", "cnt" => cnt, "@f" => cnt);
    warn!(logger, #"foo", "test warn first"; "a" => "b", "@f" => cnt);
    info!(logger, "test info"; "a" => "b", "@f" => cnt);
    warn!(logger, #"bar", "test warn second"; "a" => "b", "@f" => cnt);
    error!(logger, "test error 1"; "a" => "b", "@f" => cnt, "@pc" => "0x1234", "@sub" => "mips");
    warn!(logger, #"foo", "test warn third"; "a" => "b", "@f" => cnt);
    error!(logger, "test error 2"; "a" => "b", "@f" => cnt);
    info!(logger, #"foo", "test info"; "a" => "b", "@f" => cnt);
}

fn rand(state: &mut u64) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x as u32
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

    let mut model = FakeModel {
        curframe: 0,
        ram: Vec::new(),
        rom: Vec::new(),
    };

    model.ram.resize(1024 * 1024, 0);
    model.rom.resize(1024 * 1024, 0);
    let mut state = 0x12345678;
    for i in 4 * 1024..32 * 1024 {
        model.ram[i] = rand(&mut state) as u8;
    }
    for i in 0..1024 * 1024 {
        model.rom[i] = rand(&mut state) as u8;
    }

    let (logger, logpool) = log::new_pool_logger();

    let mut dbgui = dbg::DebuggerUI::new(video, &window, &mut model, logpool);
    let mut cnt = 0;
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

        for i in 0..100 {
            fake_logging(&logger, cnt);
        }
        cnt += 1;
        dbgui.render(&window, &event_pump, &mut model);
        window.gl_swap_window();
        ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 30));
    }
}
