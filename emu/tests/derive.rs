#![feature(attr_literals)]
extern crate byteorder;
extern crate emu;

#[macro_use]
extern crate emu_derive;

#[cfg(test)]
mod tests {
    use super::byteorder::LittleEndian;
    use super::emu::bus::{Bus, DevPtr, Mem, Reg};

    #[derive(Default, DeviceLE)]
    struct Gpu {
        #[mem(bank = 1, offset = 0x0, size = 4_194_304, vsize = 0x0200_0000)]
        ram: Mem,

        #[reg(bank = 0, offset = 0xC, rwmask = 0xffff0000, rcb, wcb)]
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

    /*
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

        fn dev_map(
            &mut self,
            bus: &mut Bus<Self::Order>,
            bank: usize,
            base: u32,
        ) -> Result<(), &'static str> {
            bus.map_mem(0x08000000, 0x09FFFFFF, &self.ram)?;
            bus.map_reg32(0x0400000C, &self.reg1)?;
            Ok(())
        }
    }
    */

    #[test]
    fn basic_device() {
        let mut gpu = DevPtr::new(Gpu::default());

        let mut bus = Bus::<LittleEndian>::new();
        bus.map_device(0x04000000, &mut gpu, 0).expect("map error");
        bus.map_device(0x08000000, &mut gpu, 1).expect("map error");

        bus.write::<u32>(0x08000123, 456);
        assert_eq!(bus.read::<u32>(0x09000123), 456);

        gpu.borrow_mut().k1 = 0x80;
        gpu.borrow_mut().k2 = 0x1;
        bus.write::<u32>(0x0400000C, 0xaaaaaaaa);
        assert_eq!(bus.read::<u32>(0x0400000C), 0xaaaa0081);
    }
}
