extern crate emu;

use emu::bus::be::{Mem, MemFlags};
use errors::*;
use std::fs::File;
use std::io::Read;

#[derive(DeviceBE)]
pub struct Cartridge {
    #[mem(offset = 0, vsize = 0x0FC0_0000)]
    rom: Mem,
}

pub fn romswap(rom: Vec<u8>) -> Vec<u8> {
    if rom[0] == 0x80 {
        // ROM is big-endian: nothing to do
        return rom;
    } else if rom[1] == 0x80 {
        // ROM is byteswapped
        return rom
            .iter()
            .enumerate()
            .map(|(idx, _)| rom[idx ^ 1])
            .collect();
    } else {
        panic!("unsupported ROM format")
    }
}

impl Cartridge {
    pub fn new(romfn: &str) -> Result<Cartridge> {
        let mut file = File::open(romfn)?;
        let mut contents = vec![];
        file.read_to_end(&mut contents)?;

        Ok(Cartridge {
            rom: Mem::from_buffer(romswap(contents), MemFlags::READACCESS),
        })
    }
}
