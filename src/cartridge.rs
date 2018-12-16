extern crate crc;
extern crate emu;

use self::crc::crc32;
use crate::errors::*;
use emu::bus::be::{Mem, MemFlags, Reg32};
use std::fs::File;
use std::io::Read;

#[derive(DeviceBE)]
pub struct Cartridge {
    #[mem(offset = 0, vsize = 0x07C0_0000)]
    rom: Mem,

    #[reg(bank = 1, offset = 0x200)]
    drive64_status: Reg32,

    #[reg(bank = 1, offset = 0x208)]
    drive64_cmd: Reg32,
}

pub enum CicModel {
    Cic6101 = 6101,
    Cic6102 = 6102,
    Cic6103 = 6103,
    Cic6105 = 6105,
    Cic6106 = 6106,
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

        if !contents.len().is_power_of_two() {
            let newsize = contents.len().next_power_of_two();
            contents.resize(newsize, 0xff);
        }

        Ok(Cartridge {
            drive64_status: Reg32::default(),
            drive64_cmd: Reg32::default(),
            rom: Mem::from_buffer(romswap(contents), MemFlags::READACCESS),
        })
    }

    // Detect the CIC model by checksumming the header of the ROM.
    pub fn detect_cic_model(&self) -> Result<CicModel> {
        let rom = self.rom.buf();
        match crc32::checksum_ieee(&rom[0x40..0x1000]) {
            0x6170A4A1 => Ok(CicModel::Cic6101),
            0x90BB6CB5 => Ok(CicModel::Cic6102),
            0x0B050EE0 => Ok(CicModel::Cic6103),
            0x98BC2C86 => Ok(CicModel::Cic6105),
            0xACC8580A => Ok(CicModel::Cic6106),
            chk => bail!("cannot detect CIC model in ROM (chk = {:08x})", chk),
        }
    }
}
