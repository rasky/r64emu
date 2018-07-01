extern crate sdl2;

use self::sdl2::event::Event;
use self::sdl2::keyboard::Keycode;
use self::sdl2::pixels::PixelFormatEnum;
use self::sdl2::render::{Texture, TextureCreator, WindowCanvas};
use self::sdl2::video::WindowContext;
use self::sdl2::{Sdl, VideoSubsystem};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

pub struct OutputConfig {
    pub window_title: String,
    pub width: isize,
    pub height: isize,
    pub fps: isize,
    pub enforce_speed: bool,
}

struct Video {
    sub: VideoSubsystem,
    canvas: WindowCanvas,
    creator: TextureCreator<WindowContext>,

    cfg: Rc<OutputConfig>,
    fps_clock: SystemTime,
    fps_counter: isize,
}

impl Video {
    fn new(cfg: Rc<OutputConfig>, context: &sdl2::Sdl) -> Result<Video, String> {
        let sub = context
            .video()
            .or_else(|e| Err(format!("error creating video subsystem: {:?}", e)))?;
        let window = sub
            .window(&cfg.window_title, 800, 600)
            .resizable()
            .position_centered()
            .opengl()
            .build()
            .or_else(|e| Err(format!("error creating window: {:?}", e)))?;
        let mut canvas = window
            .into_canvas()
            .software()
            .build()
            .or_else(|e| Err(format!("error creating canvas: {:?}", e)))?;
        let creator = canvas.texture_creator();

        canvas.set_logical_size(cfg.width as u32, cfg.height as u32);

        Ok(Video {
            cfg,
            sub,
            canvas,
            creator,
            fps_clock: SystemTime::now(),
            fps_counter: 0,
        })
    }

    fn render_frame(&mut self, frame: (&[u8], usize)) {
        self.draw(frame);
        self.update_fps();
    }

    fn draw(&mut self, frame: (&[u8], usize)) {
        let mut tex = self
            .creator
            .create_texture_target(
                PixelFormatEnum::ABGR8888,
                self.cfg.width as u32,
                self.cfg.height as u32,
            )
            .unwrap();
        tex.update(None, frame.0, frame.1);
        self.canvas.copy(&tex, None, None);
        self.canvas.present();
    }

    fn update_fps(&mut self) {
        self.fps_counter += 1;
        let one_second = Duration::new(1, 0);
        match self.fps_clock.elapsed() {
            Ok(elapsed) if elapsed >= one_second => {
                self.canvas.window_mut().set_title(&format!(
                    "{} - {} FPS",
                    &self.cfg.window_title, self.fps_counter
                ));
                self.fps_counter = 0;
                self.fps_clock += one_second;
            }
            _ => {}
        }
    }
}

pub trait OutputProducer {
    fn render_frame(&mut self, screen: &mut [u8], pitch: usize);
    fn finish(&mut self);
}

pub struct Output {
    cfg: Rc<OutputConfig>,
    context: sdl2::Sdl,
    video: Option<Video>,
}

impl Output {
    pub fn new(cfg: OutputConfig) -> Result<Output, String> {
        Ok(Output {
            cfg: Rc::new(cfg),
            context: sdl2::init()?,
            video: None,
        })
    }

    pub fn enable_video(&mut self) -> Result<(), String> {
        self.video = Some(Video::new(self.cfg.clone(), &self.context)?);
        Ok(())
    }

    pub fn run<F: 'static + Send + FnOnce() -> Result<Box<OutputProducer>, String>>(
        &mut self,
        create: F,
    ) {
        let width = self.cfg.width as usize;
        let height = self.cfg.height as usize;
        let (tx, rx) = mpsc::sync_channel(3);

        thread::spawn(move || {
            let mut producer = create().unwrap();
            loop {
                let mut screen = Vec::<u8>::new();

                screen.resize(width * height * 4, 0x00);
                producer.render_frame(&mut screen, width * 4);

                tx.send(screen).unwrap();
            }
        });

        loop {
            for event in self.context.event_pump().unwrap().poll_iter() {
                match event {
                    Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    }
                    | Event::Quit { .. } => return,
                    _ => {}
                }
            }

            let screen = rx.recv().unwrap();
            self.render_frame((&screen, width * 4));
        }
    }

    pub fn render_frame(&mut self, video: (&[u8], usize)) {
        self.video.as_mut().map(|v| v.render_frame(video));
    }
}
