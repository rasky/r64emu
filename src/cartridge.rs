use crate::emu::bus::be::{Mem, MemFlags, Reg32};
use crate::emu::int::Numerics;
use crate::errors::*;

use crc::crc32;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str;

#[derive(DeviceBE)]
pub struct Cartridge {
    #[mem(bank = 0, offset = 0, vsize = 0x03FF_0000, fill = "Fixed(0x00)")]
    rom: Mem,

    #[reg(bank = 0, offset = 0x03FF_0014, wcb)]
    isviewer_len: Reg32,

    #[mem(bank = 0, offset = 0x03FF_0020, size = 0x800, vsize = 0x800)]
    isviewer_buffer: Mem,

    #[reg(bank = 1, offset = 0x200)]
    drive64_status: Reg32,

    #[reg(bank = 1, offset = 0x208)]
    drive64_cmd: Reg32,

    logger: slog::Logger,
    isv_accum: Vec<u8>,
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
    pub fn new(logger: slog::Logger, romfn: &Path) -> Result<Box<Cartridge>> {
        let mut file = File::open(romfn)?;
        let mut contents = vec![];
        file.read_to_end(&mut contents)?;

        if !contents.len().is_power_of_two() {
            let newsize = contents.len().next_power_of_two();
            contents.resize(newsize, 0xff);
        }

        Ok(Box::new(Cartridge {
            logger,
            isv_accum: Vec::new(),
            drive64_status: Reg32::default(),
            drive64_cmd: Reg32::default(),
            isviewer_len: Reg32::default(),
            isviewer_buffer: Mem::default(),
            rom: Mem::from_buffer("rom", romswap(contents), MemFlags::READACCESS),
        }))
    }

    pub fn cb_write_isviewer_len(&mut self, _old: u32, len: u32) {
        info!(self.logger, "ISViewer Buffer"; o!("0" => self.isviewer_buffer[0].hex(), "1" => self.isviewer_buffer[1].hex()));
        self.isv_accum
            .extend_from_slice(&self.isviewer_buffer[..len as usize]);
        let mut v: Vec<&[u8]> = self.isv_accum.split(|ch| *ch == b'\n').collect();
        info!(self.logger, "ISViewer len write"; o!("len" => len, "buf" => self.isv_accum.len(), "iv0" => self.isviewer_buffer[0].hex(), "lines" => v.len()));
        for line in &v[0..v.len() - 1] {
            match str::from_utf8(line) {
                Ok(s) => info!(self.logger, "ISViewer debugging"; o!("line" => s)),
                Err(_) => info!(self.logger, "ISViewer debugging"; o!("line" => "[NON UTF8]")),
            };
        }
        self.isv_accum = v.pop().unwrap().to_vec();
    }

    // Detect the CIC model by checksumming the header of the ROM.
    pub fn detect_cic_model(&self) -> Result<CicModel> {
        match crc32::checksum_ieee(&self.rom[0x40..0x1000]) {
            0x6170A4A1 => Ok(CicModel::Cic6101),
            0x90BB6CB5 => Ok(CicModel::Cic6102),
            0x0B050EE0 => Ok(CicModel::Cic6103),
            0x98BC2C86 => Ok(CicModel::Cic6105),
            0xACC8580A => Ok(CicModel::Cic6106),
            chk => bail!("cannot detect CIC model in ROM (chk = {:08x})", chk),
        }
    }
}
