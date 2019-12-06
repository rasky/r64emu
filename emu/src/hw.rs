pub(crate) mod glutils;
mod input_mapping;

use self::glutils::SurfaceRenderer;
use self::input_mapping::{InputConfig, InputMapping};

use crate::dbg::{DebuggerModel, DebuggerUI};
use crate::gfx::{GfxBufferLE, GfxBufferMutLE, OwnedGfxBufferLE, Rgb888};
use crate::input::{InputEvent, InputManager};
use crate::snd::{OwnedSndBuffer, SampleFormat, SampleInt, SndBuffer, SndBufferMut};

use byteorder::NativeEndian;
use sdl2::audio::{AudioFormatNum, AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{AudioSubsystem, VideoSubsystem};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use std::path::Path;

pub struct VideoConfig {
    pub window_title: String,
    pub width: isize,
    pub height: isize,
    pub fps: isize,
}

pub struct AudioConfig {
    pub frequency: isize,
}

struct Video {
    video: VideoSubsystem,
    window: Window,
    renderer: SurfaceRenderer,
    _gl_context: GLContext,

    cfg: Rc<VideoConfig>,
    fps_clock: Instant,
    fps_counter: isize,
}

impl Video {
    fn new(cfg: Rc<VideoConfig>, context: &sdl2::Sdl) -> Result<Video, String> {
        let video = context
            .video()
            .or_else(|e| Err(format!("error creating video subsystem: {:?}", e)))?;

        // Request OpenGL Core profile (for GL 3.2 extensions, required by imgui-opengl-renderer).
        {
            let gl_attr = video.gl_attr();
            gl_attr.set_context_profile(GLProfile::Core);
            gl_attr.set_context_version(3, 0);
        }

        let window = video
            .window(&cfg.window_title, 640 * 2, 480 * 2)
            .resizable()
            .position_centered()
            .opengl()
            .allow_highdpi()
            .build()
            .or_else(|e| Err(format!("error creating window: {:?}", e)))?;

        let gl_context = window
            .gl_create_context()
            .expect("couldn't create GL context");

        let video2 = video.clone();
        let renderer = SurfaceRenderer::new(move |s| video2.gl_get_proc_address(s) as _);

        Ok(Video {
            cfg,
            video,
            window,
            renderer,
            _gl_context: gl_context,
            fps_clock: Instant::now(),
            fps_counter: 0,
        })
    }

    fn render_frame(&mut self, frame: &GfxBufferLE<Rgb888>) {
        self.renderer.render(frame);
    }

    fn update_fps(&mut self) {
        self.fps_counter += 1;
        if self.fps_clock.elapsed() >= Duration::new(1, 0) {
            self.window
                .set_title(&format!(
                    "{} - {} FPS",
                    &self.cfg.window_title, self.fps_counter
                ))
                .unwrap();
            self.fps_counter = 0;
            self.fps_clock += Duration::new(1, 0);
        }
    }
}

struct Audio<SI: SampleInt + AudioFormatNum, SF: SampleFormat<ORDER = NativeEndian, SAMPLE = SI>> {
    audio: AudioSubsystem,
    queue: AudioQueue<SI>,
    frame_size: usize,
    phantom: PhantomData<SF>,
}

impl<SI, SF> Audio<SI, SF>
where
    SI: SampleInt + AudioFormatNum,
    SF: SampleFormat<ORDER = NativeEndian, SAMPLE = SI>,
{
    fn new(context: &sdl2::Sdl, fps: isize, acfg: Rc<AudioConfig>) -> Self {
        let audio = context
            .audio()
            .or_else(|e| Err(format!("error creating audio subsystem: {:?}", e)))
            .unwrap();

        if acfg.frequency % fps != 0 {
            // We need to generate the exact number of samples per frame, so for
            // now only allows exact multiples. This is not impossible to make it
            // work more generally (we should request a possible different amount
            // of samples each frame), but let's punt for now.
            panic!("audio frequency not a perfect multiple of framerate");
        }

        let nsamples_per_frame = (acfg.frequency / fps) as usize;
        let spec = AudioSpecDesired {
            freq: Some(acfg.frequency as i32),
            channels: Some(SF::CHANNELS as u8),
            samples: Some(nsamples_per_frame as u16),
        };
        let queue = audio.open_queue(None, &spec).unwrap();
        queue.resume();

        Self {
            audio,
            queue,
            frame_size: nsamples_per_frame * SF::frame_size(),
            phantom: PhantomData,
        }
    }

    fn samples_per_frame(&self) -> usize {
        self.frame_size / SF::frame_size()
    }

    fn render_frame(&mut self, buf: &SndBuffer<SF>, throttle: bool) {
        if throttle {
            // Wait until the queue is less than one frame small. This
            // crates one buffer worth of lag, but should keep the audio
            // playing with no cracks.
            while self.queue.size() > self.frame_size as u32 * 2 {
                std::thread::sleep(Duration::from_micros(100));
            }
            self.queue.queue(buf.as_ref());
        } else {
            // If we're not throttling there are two possibilities:
            // we're either running too slow (in which case, there would be
            // audio cracks), or too fast; in the latter case, we want to skip
            // some audio frames to avoid desyncing audio and video.
            if self.queue.size() < self.frame_size as u32 {
                self.queue.queue(buf.as_ref());
            }
        }
    }
}

/// OutputProducer is a trait that allows an emulator to interface with
/// [`Output`](struct.Output.html) to produce audio and video on the host
/// computer.
///
/// To use [`Output`](struct.Output.html), the emulator must implement this
/// trait which exposes the static configuration of the output, and the methods
/// to run the emulator.
pub trait OutputProducer {
    /// Sample format of the audio produced by the emulator. Supported types are
    /// those that implement the
    /// [`emu::snd::SampleFormat`](../snd/trait.SampleFormat.html) trait, but
    /// using the host byte order (eg: `LittleEndian` on x86).
    type AudioSampleFormat: SampleFormat<ORDER = NativeEndian>;

    /// Return a mutable reference to an InputManager instance. If None,
    /// input events are not generated. Returning None can also be useful in case
    /// input events come from another source (gg: while playbacking).
    fn input_manager(&mut self) -> Option<&mut InputManager>;

    fn render_frame(
        &mut self,
        video: &mut GfxBufferMutLE<Rgb888>,
        audio: &mut SndBufferMut<Self::AudioSampleFormat>,
    );
}

pub struct Output {
    vcfg: Rc<VideoConfig>,
    acfg: Rc<AudioConfig>,
    context: sdl2::Sdl,
    video: Option<Video>,
    audio: bool,
    debug: bool,
    quit: bool,
    framecount: i64,
}

impl Output {
    pub fn new(vcfg: VideoConfig, acfg: AudioConfig) -> Result<Output, String> {
        Ok(Output {
            vcfg: Rc::new(vcfg),
            acfg: Rc::new(acfg),
            context: sdl2::init()?,
            video: None,
            audio: false,
            debug: true,
            quit: false,
            framecount: 0,
        })
    }

    pub fn enable_video(&mut self) -> Result<(), String> {
        self.video = Some(Video::new(self.vcfg.clone(), &self.context)?);
        Ok(())
    }

    pub fn enable_audio(&mut self) -> Result<(), String> {
        self.audio = true;
        Ok(())
    }

    fn process_event(&mut self, event: &Event) {
        match event {
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => {
                // Toggle debugger activation
                self.debug = !self.debug
            }
            Event::Quit { .. } => {
                self.quit = true;
            }
            _ => {}
        }
    }

    pub fn run_and_debug<SI, SF, P>(&mut self, producer: &mut P, dbg_conf_filename: &Path)
    where
        SI: SampleInt + AudioFormatNum,
        SF: SampleFormat<SAMPLE = SI, ORDER = NativeEndian>,
        P: OutputProducer<AudioSampleFormat = SF> + DebuggerModel,
    {
        let width = self.vcfg.width as usize;
        let height = self.vcfg.height as usize;
        assert_eq!(self.video.is_some(), true); // TODO: debugger could work without video as well

        let video = self.video.as_ref().unwrap();
        let mut dbg_ui = DebuggerUI::new(video.video.clone(), &video.window, producer);
        if dbg_conf_filename.exists() {
            dbg_ui.load_conf(dbg_conf_filename);
        }

        let mut audio = Audio::<SI, SF>::new(&self.context, self.vcfg.fps, self.acfg.clone());
        let mut audio_buf = OwnedSndBuffer::with_capacity(audio.samples_per_frame());

        let mut event_pump = self.context.event_pump().unwrap();
        let mut screen = OwnedGfxBufferLE::<Rgb888>::new(width, height);

        let input = match producer.input_manager() {
            Some(im) => Some(InputMapping::new(InputConfig::default(im))),
            None => None,
        };

        while !self.quit {
            for event in event_pump.poll_iter() {
                dbg_ui.handle_event(&event);
                self.process_event(&event);

                if let Some(map) = input.as_ref() {
                    if let Some(im) = producer.input_manager() {
                        match map.map_event(&event) {
                            Some(evt) => {
                                im.process_event(evt);
                            }
                            None => {}
                        };
                    }
                }
            }

            let v = self.video.as_mut().unwrap();
            if !self.debug {
                producer.render_frame(&mut screen.buf_mut(), &mut audio_buf.buf_mut());
                v.render_frame(&screen.buf());
                audio.render_frame(&audio_buf.buf(), true);
                v.update_fps();
            } else {
                if dbg_ui.trace(producer, &mut screen.buf_mut(), &mut audio_buf.buf_mut()) {
                    v.update_fps();
                }
                dbg_ui.render(&v.window, &event_pump, producer);
            }

            v.window.gl_swap_window();

            self.framecount += 1;
        }

        dbg_ui.save_conf(dbg_conf_filename);
    }

    /// Run a blocking loop in which output is produced by a OutputProducer,
    /// until the producer exits by itself, or the user closes the window.
    /// The OutputProducer is run in a background thread, so to parallelize
    /// display visualization and vsync with actual output generation.
    ///
    /// create is a FnOnce callback that creates a OutputProducer, and is invoked
    /// in the background thread so that OutputProducer needs not to implement
    /// Send.
    pub fn run_threaded<F, P, SI, SF>(&mut self, create: F)
    where
        SI: SampleInt + AudioFormatNum,
        SF: SampleFormat<SAMPLE = SI, ORDER = NativeEndian>,
        P: OutputProducer<AudioSampleFormat = SF>,
        F: FnOnce() -> Result<Box<P>, String> + Send + 'static,
    {
        let width = self.vcfg.width as usize;
        let height = self.vcfg.height as usize;
        let (tx_frame, rx_frame) = mpsc::sync_channel(3);
        let (tx_event, rx_event) = mpsc::sync_channel::<Vec<InputEvent>>(3);
        let (tx_input, rx_input) = mpsc::sync_channel(1);

        let mut audio = Audio::new(&self.context, self.vcfg.fps, self.acfg.clone());
        let audio_frame_size = audio.samples_per_frame();

        let mut event_pump = self.context.event_pump().unwrap();

        thread::spawn(move || {
            let mut producer = create().unwrap();

            // Send a clone of the input manager to the main thread,
            // for input mapping initialization.
            tx_input.send(producer.input_manager().map(|im| im.clone()));

            loop {
                let mut sound = OwnedSndBuffer::with_capacity(audio_frame_size);
                let mut screen = OwnedGfxBufferLE::<Rgb888>::new(width, height);
                producer.render_frame(&mut screen.buf_mut(), &mut sound.buf_mut());

                if !tx_frame.send((screen, sound)).is_ok() {
                    return;
                }

                // If we received any input event from the main thread, process
                // them through the input manager.
                if let Ok(evts) = rx_event.try_recv() {
                    if let Some(im) = producer.input_manager() {
                        for e in evts.iter() {
                            im.process_event(e.clone());
                        }
                    }
                }
            }
        });

        // Initialize input mapping, using the default config for the
        // current input manager. TODO: add load/save of input mapping.
        let input = match rx_input.recv() {
            Ok(Some(im)) => Some(InputMapping::new(InputConfig::default(&im))),
            Ok(None) => None,
            Err(_) => panic!("error while receiving input manager?"),
        };

        let polling_interval = Duration::from_millis(20);
        while !self.quit {
            let mut events = Vec::new();
            for event in event_pump.poll_iter() {
                self.process_event(&event);

                // Try to pass the even through the input mapping.
                // If it's mapped to an emulator input, accumulate
                // to send it
                if let Some(map) = input.as_ref() {
                    if let Some(evt) = map.map_event(&event) {
                        events.push(evt);
                    }
                }
            }
            if events.len() > 0 {
                tx_event.send(events);
            }

            match rx_frame.recv_timeout(polling_interval) {
                Ok((ref screen, ref sound)) => {
                    self.render_frame(&screen.buf());
                    audio.render_frame(&sound.buf(), true);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Render a single frame to the video output.
    pub fn render_frame(&mut self, screen: &GfxBufferLE<Rgb888>) {
        if let Some(v) = self.video.as_mut() {
            v.render_frame(&screen);
            v.window.gl_swap_window();
            v.update_fps();
        }
    }
}
