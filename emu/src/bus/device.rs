use super::bus::Bus;
use crate::memint::ByteOrderCombiner;
use hashbrown::HashMap;
use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::pin::{Pin, Unpin};
use std::rc::Rc;

pub trait Device {
    type Order: ByteOrderCombiner;

    fn dev_init(&mut self, wself: Rc<RefCell<Self>>);
    fn dev_map(
        &self,
        bus: &mut Bus<Self::Order>,
        bank: usize,
        base: u32,
    ) -> Result<(), &'static str>;
}

pub struct DevPtr<T: Device> {
    dev: Rc<RefCell<T>>,
}

impl<'b, T> DevPtr<T>
where
    T: Device,
{
    pub fn new(d: T) -> DevPtr<T> {
        let d = DevPtr {
            dev: Rc::new(RefCell::new(d)),
        };

        d.dev.borrow_mut().dev_init(d.dev.clone());
        return d;
    }

    pub fn clone(&self) -> DevPtr<T> {
        DevPtr {
            dev: self.dev.clone(),
        }
    }

    pub fn borrow(&self) -> Ref<T> {
        self.dev.borrow()
    }

    pub fn borrow_mut(&mut self) -> RefMut<T> {
        self.dev.borrow_mut()
    }

    pub fn unwrap(self) -> Rc<RefCell<T>> {
        self.dev
    }
}

pub type PinnedDevice = Pin<Box<dyn Any + Unpin>>;

pub trait DeviceWithTag {
    fn tag() -> &'static str;
}

pub trait DeviceGetter {
    type Dev: DeviceWithTag;
    fn get() -> &'static Self::Dev;
    fn get_mut() -> &'static mut Self::Dev;
}

impl<D: DeviceWithTag> DeviceGetter for D {
    type Dev = D;
    fn get() -> &'static Self::Dev {
        CurrentDeviceMap().get::<Self::Dev>().unwrap()
    }
    fn get_mut() -> &'static mut Self::Dev {
        CurrentDeviceMap().get_mut::<Self::Dev>().unwrap()
    }
}

#[derive(Default)]
pub struct DeviceMap {
    devices: HashMap<&'static str, PinnedDevice>,
}

impl DeviceMap {
    pub fn register<D: 'static + DeviceWithTag + Unpin>(&mut self, o: Pin<Box<D>>) {
        self.devices.insert(D::tag(), o);
    }

    pub fn get_by_tag<D: 'static + DeviceWithTag>(&self, tag: &'static str) -> Option<&D> {
        self.devices
            .get(tag)
            .map(|v| (Pin::get_ref(v.as_ref()) as &Any).downcast_ref().unwrap())
    }
    pub fn get_mut_by_tag<D: 'static + DeviceWithTag>(
        &mut self,
        tag: &'static str,
    ) -> Option<&mut D> {
        self.devices.get_mut(tag).map(|v| {
            (Pin::get_mut(v.as_mut()) as &mut Any)
                .downcast_mut()
                .unwrap()
        })
    }

    pub fn get<D: 'static + DeviceWithTag>(&self) -> Option<&D> {
        self.get_by_tag(D::tag())
    }
    pub fn get_mut<D: 'static + DeviceWithTag>(&mut self) -> Option<&mut D> {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Cpu {
        val: u64,
    }
    struct Gpu {
        val: u64,
    }

    impl DeviceWithTag for Cpu {
        fn tag() -> &'static str {
            "CPU"
        }
    }
    impl DeviceWithTag for Gpu {
        fn tag() -> &'static str {
            "GPU"
        }
    }

    #[test]
    fn basic() {
        let devmap = CurrentDeviceMap();
        devmap.register(Pin::new(Box::new(Cpu { val: 4 })));
        devmap.register(Pin::new(Box::new(Gpu { val: 8 })));

        assert_eq!(Cpu::get().val, 4);
        assert_eq!(Gpu::get().val, 8);

        Cpu::get_mut().val = 10;
        assert_eq!(Cpu::get().val, 10);
    }
}
