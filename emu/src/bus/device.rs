use super::bus::Bus;
use super::memint::{ByteOrderCombiner, MemInt};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};

pub trait Device {
    type Order: ByteOrderCombiner;

    fn dev_init(&mut self, wself: Rc<RefCell<Self>>);
    fn dev_map(&mut self, bus: &mut Bus<Self::Order>) -> Result<(), &'static str>;
}

#[derive(Clone)]
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

    pub fn borrow(&mut self) -> Ref<T> {
        self.dev.borrow()
    }

    pub fn borrow_mut(&mut self) -> RefMut<T> {
        self.dev.borrow_mut()
    }

    pub fn map_into(&mut self, bus: &mut Bus<T::Order>) -> Result<(), &'static str> {
        self.dev.borrow_mut().dev_map(bus)
    }
}

#[cfg(test)]
mod tests {
    use super::super::mem::{Mem, MemFlags};
    use super::super::regs::{Reg, RegFlags};
    use super::*;
    extern crate byteorder;
    use self::byteorder::LittleEndian;

    #[derive(Default)]
    struct Gpu {
        ram: Mem,
        reg1: Reg<LittleEndian, u32>,
        k1: u32,
        k2: u32,
    }

    impl Gpu {
        fn cb_write_reg1(&mut self, _old: u32, val: u32) {
            self.reg1.set(val | self.k1);
        }

        fn cb_read_reg1(&self, val: u32) -> u32 {
            val | self.k2
        }
    }

    impl Device for Gpu {
        type Order = LittleEndian;

        fn dev_init(&mut self, wself: Rc<RefCell<Self>>) {
            self.ram = Mem::new(1024, MemFlags::default());

            let wdevr = Rc::downgrade(&wself);
            let wdevw = Rc::downgrade(&wself);
            self.reg1 = Reg::new(
                0,
                0xffff0000,
                RegFlags::default(),
                Some(Rc::new(box move |old, val| {
                    let dev = wdevw.upgrade().unwrap();
                    dev.borrow_mut().cb_write_reg1(old, val);
                })),
                Some(Rc::new(box move |val| {
                    let dev = wdevr.upgrade().unwrap();
                    let res = dev.borrow().cb_read_reg1(val);
                    drop(dev);
                    res
                })),
            );
        }

        fn dev_map(&mut self, bus: &mut Bus<LittleEndian>) -> Result<(), &'static str> {
            bus.map_mem(0x08000000, 0x09FFFFFF, &self.ram)?;
            bus.map_reg32(0x0400000C, &self.reg1)?;
            Ok(())
        }
    }

    #[test]
    fn basic_device() {
        let mut gpu = DevPtr::new(Gpu::default());

        let mut bus = Bus::<LittleEndian>::new();
        bus.map_device(&mut gpu);

        bus.write::<u32>(0x08000123, 456);
        assert_eq!(bus.read::<u32>(0x09000123), 456);

        gpu.borrow_mut().k1 = 0x80;
        gpu.borrow_mut().k2 = 0x1;
        bus.write::<u32>(0x0400000C, 0xaaaaaaaa);
        assert_eq!(bus.read::<u32>(0x0400000C), 0xaaaa0081);
    }
}
