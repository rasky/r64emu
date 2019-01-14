use indexmap::map::IndexMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Other,
    Button1,
    Button2,
    Button3,
    Button4,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
    Coin,
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDeviceKind {
    Joystick,
    Mouse,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub enum InputValue {
    /// Digital input (eg: a button). True is asserted (pressed), False is the
    /// default state (released).
    Digital(bool),

    /// Analog input (eg: a joystick). 0 is the default state.
    Analog(i16),

    /// Absolute coordinate input (eg: a mouse). 0x7FFF is the default state.
    Coordinate(u16),
}

#[derive(Clone, Debug)]
pub struct Input {
    name: String,
    kind: InputKind,
    value: InputValue,
    prev: InputValue,
    custom_id: usize,
}

impl Input {
    pub fn new_digital(name: &str, kind: InputKind, custom_id: usize) -> Input {
        Input {
            name: name.into(),
            kind: kind,
            value: InputValue::Digital(false),
            prev: InputValue::Digital(false),
            custom_id,
        }
    }

    pub fn new_analog(name: &str, kind: InputKind, custom_id: usize) -> Input {
        Input {
            name: name.into(),
            kind: kind,
            value: InputValue::Analog(0),
            prev: InputValue::Analog(0),
            custom_id,
        }
    }

    pub fn new_coordinate(name: &str, kind: InputKind, custom_id: usize) -> Input {
        Input {
            name: name.into(),
            kind: kind,
            value: InputValue::Coordinate(0x7FFF),
            prev: InputValue::Coordinate(0x7FFF),
            custom_id,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> InputKind {
        self.kind
    }

    pub fn value(&self) -> InputValue {
        self.value
    }

    pub fn custom_id(&self) -> usize {
        self.custom_id
    }

    pub fn digital(&self) -> Option<bool> {
        match self.value {
            InputValue::Digital(val) => Some(val),
            _ => None,
        }
    }

    pub fn analog(&self) -> Option<i16> {
        match self.value {
            InputValue::Analog(val) => Some(val),
            _ => None,
        }
    }

    pub fn coordinate(&self) -> Option<u16> {
        match self.value {
            InputValue::Coordinate(val) => Some(val),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct InputDevice {
    name: String,
    kind: InputDeviceKind,
    inputs: IndexMap<String, Input>,
    active: bool,
}

impl InputDevice {
    pub fn new(name: &str, kind: InputDeviceKind, inputs: Vec<Input>) -> InputDevice {
        InputDevice {
            name: name.into(),
            kind,
            inputs: inputs.iter().map(|i| (i.name.clone(), i.clone())).collect(),
            active: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> InputDeviceKind {
        self.kind
    }

    pub fn dup(&self, name: &str) -> InputDevice {
        let mut i = self.clone();
        i.name = name.into();
        i
    }

    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.get(name)
    }

    pub fn visit<F: FnMut(&Input)>(&self, mut cb: F) {
        for val in self.inputs.values() {
            cb(val);
        }
    }
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    Digital(String, String, bool),
    Analog(String, String, i16),
    Coordinate(String, String, u16),
}

#[derive(Clone)]
pub struct InputManager {
    // Devices defined in this input manager. NOTE: it's using
    // indexmap::HashMap so that insertion order is preserved
    // while iterating.
    devices: IndexMap<String, InputDevice>,
    events: Vec<(usize, InputEvent)>,
    curframe: usize,
}

impl InputManager {
    pub fn new(devices: Vec<InputDevice>) -> InputManager {
        InputManager {
            devices: devices
                .iter()
                .map(|d| (d.name.clone(), d.clone()))
                .collect(),
            events: Vec::with_capacity(256),
            curframe: 0,
        }
    }

    pub fn begin_frame(&mut self) {}

    pub fn process_event(&mut self, event: InputEvent) {
        match &event {
            InputEvent::Digital(dev, inp, val) => {
                let inp = &mut self
                    .devices
                    .get_mut(dev)
                    .unwrap()
                    .inputs
                    .get_mut(inp)
                    .unwrap();
                inp.prev = inp.value;
                inp.value = InputValue::Digital(*val);
            }
            InputEvent::Analog(dev, inp, val) => {
                let inp = &mut self
                    .devices
                    .get_mut(dev)
                    .unwrap()
                    .inputs
                    .get_mut(inp)
                    .unwrap();
                inp.prev = inp.value;
                inp.value = InputValue::Analog(*val);
            }
            InputEvent::Coordinate(dev, inp, val) => {
                let inp = &mut self
                    .devices
                    .get_mut(dev)
                    .unwrap()
                    .inputs
                    .get_mut(inp)
                    .unwrap();
                inp.prev = inp.value;
                inp.value = InputValue::Coordinate(*val);
            }
        };
        self.events.push((self.curframe, event));
    }

    pub fn end_frame(&mut self) {
        self.curframe += 1;
    }

    /// Get a reference to an [InputDevice](struct.InputDevice.html)
    /// by name (if it exists).
    pub fn device(&self, name: &str) -> Option<&InputDevice> {
        self.devices.get(name)
    }

    /// Visit all the defined [InputDevice](struct.InputDevice.html)
    /// instances (in insertion order).
    pub fn visit<F: FnMut(&InputDevice)>(&self, mut f: F) {
        for d in self.devices.values() {
            f(d);
        }
    }
}
