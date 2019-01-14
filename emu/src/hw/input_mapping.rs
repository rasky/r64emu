use crate::input::{InputDeviceKind, InputEvent, InputKind, InputManager};

use sdl2;
use sdl2::keyboard::{Keycode, Scancode};
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

/// PhysicalDevice describes how a device was mapped.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
enum PhysicalDevice {
    Keyboard,
    Joystick(String),
}

#[derive(Serialize, Deserialize)]
struct InputDeviceConfig {
    phys: PhysicalDevice,
    mapping: HashMap<String, String>, // input name = key/joy
}

fn default_scancode_for_kind(kind: InputKind) -> Option<Scancode> {
    use self::InputKind::*;
    match kind {
        Start => Some(Scancode::Return),
        Select => Some(Scancode::Backspace),
        Up => Some(Scancode::Up),
        Down => Some(Scancode::Down),
        Left => Some(Scancode::Left),
        Right => Some(Scancode::Right),
        Button1 => Some(Scancode::Z),
        Button2 => Some(Scancode::X),
        Button3 => Some(Scancode::C),
        Button4 => Some(Scancode::V),
        _ => None,
    }
}

#[derive(Serialize, Deserialize)]
pub struct InputConfig {
    devices: HashMap<String, InputDeviceConfig>, // device name => mapped device
}

impl InputConfig {
    pub fn default(im: &InputManager) -> InputConfig {
        let mut devices = HashMap::new();
        let mut first_joystick = true;

        im.visit(|dev| {
            let mut mapping = HashMap::new();
            if first_joystick && dev.kind() == InputDeviceKind::Joystick {
                dev.visit(|inp| {
                    if let Some(scan) = default_scancode_for_kind(inp.kind()) {
                        let key_name = Keycode::from_scancode(scan).unwrap().name();
                        mapping.insert(inp.name().to_owned(), key_name);
                    }
                });
                first_joystick = false;
            }

            devices.insert(
                dev.name().to_owned(),
                InputDeviceConfig {
                    phys: PhysicalDevice::Keyboard,
                    mapping: mapping,
                },
            );
        });

        InputConfig { devices }
    }

    fn all_keys(&self) -> HashMap<Scancode, (String, String)> {
        self.devices
            .iter()
            .filter(|(_, d)| d.phys == PhysicalDevice::Keyboard)
            .map(|(dev_name, d)| {
                d.mapping.iter().map(move |(inp_name, key_name)| {
                    let scan =
                        Scancode::from_keycode(Keycode::from_name(key_name).unwrap()).unwrap();
                    (scan, (dev_name.clone(), inp_name.clone()))
                })
            })
            .flatten()
            .collect()
    }
}

pub struct InputMapping {
    cfg: InputConfig,
    key_lookup: HashMap<Scancode, (String, String)>,
}

impl InputMapping {
    pub fn new(cfg: InputConfig) -> Self {
        let key_lookup = cfg.all_keys();
        Self { cfg, key_lookup }
    }

    pub fn map_event(&self, event: &sdl2::event::Event) -> Option<InputEvent> {
        use sdl2::event::Event::*;
        match event {
            KeyDown {
                scancode: Some(scode),
                ..
            } => match self.key_lookup.get(scode) {
                Some((dev, inp)) => {
                    Some(InputEvent::Digital(dev.to_string(), inp.to_string(), true))
                }
                None => None,
            },

            KeyUp {
                scancode: Some(scode),
                ..
            } => match self.key_lookup.get(scode) {
                Some((dev, inp)) => {
                    Some(InputEvent::Digital(dev.to_string(), inp.to_string(), false))
                }
                None => None,
            },

            _ => None,
        }
    }
}
