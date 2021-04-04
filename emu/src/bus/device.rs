use super::bus::Bus;
use crate::memint::ByteOrderCombiner;
use hashbrown::HashMap;
use std::any::Any;
use std::cell::RefCell;
use std::marker::Unpin;
use std::pin::Pin;

pub trait Device: Sized {
    type Order: ByteOrderCombiner;

    fn register(self: Box<Self>);

    fn dev_map(
        &self,
        bus: &mut Bus<Self::Order>,
        bank: usize,
        base: u32,
    ) -> Result<(), &'static str>;

    fn tag() -> &'static str;

    fn get() -> &'static Self {
        CurrentDeviceMap().get::<Self>().unwrap()
    }
    fn get_mut() -> &'static mut Self {
        CurrentDeviceMap().get_mut::<Self>().unwrap()
    }
}

type PinnedDevice = Pin<Box<dyn Any + Unpin>>;

#[derive(Default)]
pub struct DeviceMap {
    devices: HashMap<&'static str, PinnedDevice>,
}

impl DeviceMap {
    pub fn register<D: 'static + Device + Unpin>(&mut self, o: Pin<Box<D>>) {
        self.devices.insert(D::tag(), o);
    }

    pub fn get_by_tag<D: 'static + Device>(&self, tag: &'static str) -> Option<&D> {
        self.devices
            .get(tag)
            .map(|v| (Pin::get_ref(v.as_ref()) as &Any).downcast_ref().unwrap())
    }
    pub fn get_mut_by_tag<D: 'static + Device>(&mut self, tag: &'static str) -> Option<&mut D> {
        self.devices.get_mut(tag).map(|v| {
            (Pin::get_mut(v.as_mut()) as &mut dyn Any)
                .downcast_mut()
                .unwrap()
        })
    }

    pub fn get<D: 'static + Device>(&self) -> Option<&D> {
        self.get_by_tag(D::tag())
    }
    pub fn get_mut<D: 'static + Device>(&mut self) -> Option<&mut D> {
        self.get_mut_by_tag(D::tag())
    }
}

thread_local!(
    static DEVICE_MAP: RefCell<DeviceMap> = RefCell::new(DeviceMap::default())
);

#[allow(non_snake_case)]
pub fn CurrentDeviceMap() -> &'static mut DeviceMap {
    let s: *const DeviceMap = DEVICE_MAP.with(|s| &(*s.borrow()) as _);
    let s: *mut DeviceMap = s as *mut DeviceMap;
    unsafe { &mut *s }
}
