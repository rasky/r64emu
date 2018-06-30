extern crate sdl2;

use self::sdl2::event::Event;
use self::sdl2::keyboard::Keycode;
use self::sdl2::pixels::PixelFormatEnum;
use self::sdl2::render::{Texture, TextureCreator, WindowCanvas};
use self::sdl2::video::WindowContext;
use self::sdl2::{Sdl, VideoSubsystem};

use std::time::{Duration, SystemTime};

pub struct OutputConfig {
    pub title: String,
    pub width: isize,
    pub height: isize,
    pub fps: isize,
    pub enforce_speed: bool,
}

struct Video {
    sub: VideoSubsystem,
    canvas: WindowCanvas,
    creator: TextureCreator<WindowContext>,

    window_title: String,
    fps_clock: SystemTime,
    fps_counter: isize,
}

impl Video {
    fn new(context: &sdl2::Sdl, window_title: &str) -> Result<Video, String> {
        let sub = context
            .video()
            .or_else(|e| Err(format!("error creating video subsystem: {:?}", e)))?;
        let window = sub
            .window(window_title, 800, 600)
            .resizable()
            .position_centered()
            .opengl()
            .build()
            .or_else(|e| Err(format!("error creating window: {:?}", e)))?;
        let canvas = window
            .into_canvas()
            .software()
            .build()
            .or_else(|e| Err(format!("error creating canvas: {:?}", e)))?;
        let creator = canvas.texture_creator();

        Ok(Video {
            window_title: window_title.into(),
            sub,
            canvas,
            creator,
            fps_clock: SystemTime::now(),
            fps_counter: 0,
        })
    }

    fn present(&mut self) {
        self.canvas.clear();
        self.canvas.present();

        self.fps_counter += 1;
        let one_second = Duration::new(1, 0);
        match self.fps_clock.elapsed() {
            Ok(elapsed) if elapsed >= one_second => {
                self.canvas
                    .window_mut()
                    .set_title(&format!("{} - {} FPS", self.window_title, self.fps_counter));
                self.fps_counter = 0;
                self.fps_clock += one_second;
            }
            _ => {}
        }
    }
}

pub struct Output {
    cfg: OutputConfig,
    context: sdl2::Sdl,
    video: Option<Video>,
}

impl Output {
    pub fn new(cfg: OutputConfig) -> Result<Output, String> {
        Ok(Output {
            cfg,
            context: sdl2::init()?,
            video: None,
        })
    }

    pub fn enable_video(&mut self) -> Result<(), String> {
        self.video = Some(Video::new(&self.context, &self.cfg.title)?);
        Ok(())
    }

    pub fn poll(&self) -> bool {
        for event in self.context.event_pump().unwrap().poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::Quit { .. } => return false,
                _ => {}
            }
        }
        true
    }

    pub fn present(&mut self) {
        if let Some(ref mut video) = self.video {
            video.present();
        }
    }
}
