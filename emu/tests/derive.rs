#![feature(pin)]

#[cfg(test)]
mod tests {
    use byteorder::LittleEndian;
    use emu::bus::{Bus, Device, Mem, Reg};
    use emu_derive::DeviceLE;

    use slog::Drain;
    use slog::*;
    use slog_term;
    use std;

    fn logger() -> slog::Logger {
        let decorator = slog_term::PlainSyncDecorator::new(std::io::stdout());
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        slog::Logger::root(drain, o!())
    }

    #[derive(Default, DeviceLE)]
    struct Gpu {
        #[mem(
            bank = 1,
            offset = 0x0,
            size = 4194304,
            vsize = 0x0200_0000,
            fill = "Mirror"
        )]
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

    #[test]
    fn basic_device() {
        Box::new(Gpu::default()).register();

        let mut bus = Bus::<LittleEndian>::new(logger());
        {
            let gpu = Gpu::get();
            bus.map_device(0x04000000, gpu, 0).expect("map error");
            bus.map_device(0x08000000, gpu, 1).expect("map error");
        }

        bus.write::<u32>(0x08000123, 456);
        assert_eq!(bus.read::<u32>(0x09000123), 456);

        {
            let gpu = Gpu::get_mut();
            gpu.k1 = 0x80;
            gpu.k2 = 0x1;
        }
        bus.write::<u32>(0x0400000C, 0xaaaaaaaa);
        assert_eq!(bus.read::<u32>(0x0400000C), 0xaaaa0081);
    }
}
