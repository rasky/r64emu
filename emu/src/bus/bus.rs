use super::device::Device;
use super::mem::Mem;
use super::radix::RadixTree;
use super::regs::Reg;
use crate::memint::{AccessSize, ByteOrderCombiner, MemInt};
use crate::state::ArrayField;

use array_macro::array;
use byteorder::ByteOrder;
use enum_map::{enum_map, EnumMap};
use slog::*;
use static_assertions::assert_eq_size;
use std::result::Result; // explicit import to override slog::Result

use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;
use std::slice;

pub(crate) enum HwIoR {
    Mem(ArrayField<u8>, u32),
    Func(Rc<Fn(u32) -> u64>),
}

impl Clone for HwIoR {
    fn clone(&self) -> HwIoR {
        use self::HwIoR::*;
        match self {
            Mem(f, m) => Mem(unsafe { (*f).clone() }, *m),
            Func(f) => Func(f.clone()),
        }
    }
}

pub(crate) enum HwIoW {
    Mem(ArrayField<u8>, u32),
    Func(Rc<RefCell<FnMut(u32, u64)>>),
}

impl Clone for HwIoW {
    fn clone(&self) -> HwIoW {
        use self::HwIoW::*;
        match self {
            Mem(f, m) => Mem(unsafe { (*f).clone() }, *m),
            Func(f) => Func(f.clone()),
        }
    }
}

impl HwIoR {
    pub(crate) fn at<O: ByteOrder, U: MemInt>(&self, addr: u32) -> MemIoR<O, U> {
        MemIoR {
            hwio: self.clone(),
            addr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn read<O: ByteOrder, U: MemInt>(&self, addr: u32) -> U {
        match self {
            HwIoR::Mem(buf, mask) => U::endian_read_from::<O>(&buf[(addr & mask) as usize..]),
            HwIoR::Func(f) => U::truncate_from(f(addr)),
        }
    }
}

impl HwIoW {
    pub(crate) fn at<O: ByteOrder, U: MemInt>(&self, addr: u32) -> MemIoW<O, U> {
        MemIoW {
            hwio: self.clone(),
            addr,
            phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn write<O: ByteOrder, U: MemInt>(&mut self, addr: u32, val: U) {
        match self {
            HwIoW::Mem(ref mut buf, mask) => {
                U::endian_write_to::<O>(&mut buf[(addr & *mask) as usize..], val)
            }
            HwIoW::Func(ref mut f) => {
                let mut func = f.borrow_mut();
                (&mut *func)(addr, val.into());
            }
        }
    }
}

#[derive(Clone)]
pub struct MemIoR<O: ByteOrder, U: MemInt> {
    hwio: HwIoR,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

use std::iter;
pub type MemIoRIterator<'a, U> = iter::Map<slice::ChunksExact<'a, u8>, for<'r> fn(&'r [u8]) -> U>;

impl<O: ByteOrder, U: MemInt> MemIoR<O, U> {
    pub fn default() -> Self {
        MemIoR {
            hwio: HwIoR::Func(Rc::new(|_| 0)),
            addr: 0,
            phantom: PhantomData,
        }
    }

    pub fn is_mem(&self) -> bool {
        match &self.hwio {
            HwIoR::Mem(_, _) => true,
            HwIoR::Func(_) => false,
        }
    }

    pub fn read(&self) -> U {
        self.hwio.read::<O, U>(self.addr)
    }

    // If MemIoR points to a memory area, returns an iterator over it
    // that yields consecutive elements of type U.
    // Otherwise, returns None.
    pub fn iter(&self) -> Option<MemIoRIterator<U>> {
        //impl Iterator<Item = U>> {
        match &self.hwio {
            HwIoR::Mem(buf, mask) => Some(
                buf[(self.addr & *mask) as usize..]
                    .chunks_exact(U::SIZE)
                    .map(U::endian_read_from::<O>),
            ),
            HwIoR::Func(_) => None,
        }
    }
}

impl<O: ByteOrder> MemIoR<O, u8> {
    pub fn mem<'s, 'r: 's>(&'s self) -> Option<&'r [u8]> {
        match &self.hwio {
            HwIoR::Mem(buf, mask) => {
                Some(&unsafe { buf.as_slice() }[(self.addr & mask) as usize..])
            }
            HwIoR::Func(_) => None,
        }
    }
}

impl<O: ByteOrder, U: MemInt> io::Read for MemIoR<O, U> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        match self.hwio.clone() {
            HwIoR::Mem(buf, mask) => {
                (&buf[(self.addr & mask) as usize..])
                    .read(out)
                    .and_then(|sz| {
                        self.addr += sz as u32;
                        Ok(sz)
                    })
            }
            HwIoR::Func(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "memory area is not a linearly mapped",
            )),
        }
    }
}

#[derive(Clone)]
pub struct MemIoW<O: ByteOrder, U: MemInt> {
    hwio: HwIoW,
    addr: u32,
    phantom: PhantomData<(O, U)>,
}

impl<O: ByteOrder, U: MemInt> MemIoW<O, U> {
    pub fn is_mem(&self) -> bool {
        match &self.hwio {
            HwIoW::Mem(_, _) => true,
            HwIoW::Func(_) => false,
        }
    }
    pub fn write(&mut self, val: U) {
        self.hwio.write::<O, U>(self.addr, val);
    }
}

impl<O: ByteOrder> MemIoW<O, u8> {
    pub fn mem<'s, 'r: 's>(&'s mut self) -> Option<&'r mut [u8]> {
        match self.hwio {
            HwIoW::Mem(ref mut buf, mask) => {
                Some(&mut unsafe { buf.as_slice_mut() }[(self.addr & mask) as usize..])
            }
            HwIoW::Func(_) => None,
        }
    }
}

/// BusFill specifies the behavior of the bus when requested to map a bus object
/// into a virtual range longer than its physical range; eg. a
/// [`Mem`](struct.Mem.html) object with physical size of 4Kb might be mapped
/// over a 16Mb virtual range, by passing a 16Mb address range in the call to
/// [`Bus::map_mem()`](struct.Bus.html#method.map_mem).
///
/// BusFill implements the `Default` trait with its `None` variant.
#[derive(Debug, Copy, Clone)]
pub enum BusFill {
    /// `None` means that no behavior is specified because the mapping should
    /// not exceed the physical size. If this is not true, a panic will be raised.
    None,

    /// `Mirror` means that the bus should mirror the bus object over the virtual
    /// size. If the virtual size is not an exact multiple of the physical size,
    /// the mapping function will panic.
    Mirror,

    /// `Fixed` means that the object will be mapped at the start of the
    /// specified virtual range. Any read access beyond the physical size
    /// will return the specified fixed value. Any write access beyond the
    /// physical size will be logged as an error and will be ignored.
    Fixed(u8),
}

impl Default for BusFill {
    fn default() -> Self {
        BusFill::None
    }
}

pub(crate) fn unmapped_area_r() -> HwIoR {
    thread_local!(
        static FN: Rc<Fn(u32)->u64> = Rc::new(|_| {
            return 0xffff_ffff_ffff_ffff;
        })
    );
    HwIoR::Func(FN.with(|c| c.clone()))
}

pub(crate) fn unmapped_area_w() -> HwIoW {
    thread_local!(
        static FN: Rc<RefCell<FnMut(u32,u64)>> = Rc::new(RefCell::new(|_,_| {}))
    );
    HwIoW::Func(FN.with(|c| c.clone()))
}

pub struct MemoryDesc {
    pub name: String,
    pub begin: u64,
    pub end: u64,
    pub readable: bool,
    pub writeable: bool,
}

pub struct Bus<Order: ByteOrderCombiner> {
    reads: EnumMap<AccessSize, Box<RadixTree<HwIoR>>>,
    writes: EnumMap<AccessSize, Box<RadixTree<HwIoW>>>,

    fillers: [ArrayField<u8>; 256],
    unmap_r: HwIoR,
    unmap_w: HwIoW,

    logger: slog::Logger,
    mems: Vec<MemoryDesc>, // List of mapped memory areas (for debugging)

    phantom: PhantomData<Order>,
}

impl<'a: 'b, 'b, 's: 'b, Order> Bus<Order>
where
    Order: ByteOrderCombiner + 'static,
{
    pub fn new(logger: slog::Logger) -> Box<Bus<Order>> {
        assert_eq_size!(HwIoR, [u8; 24]);
        assert_eq_size!(HwIoW, [u8; 24]);

        Box::new(Bus {
            reads: enum_map! {
                AccessSize::Size8 => RadixTree::new(),
                AccessSize::Size16 => RadixTree::new(),
                AccessSize::Size32 => RadixTree::new(),
                AccessSize::Size64 => RadixTree::new(),
            },
            writes: enum_map! {
                AccessSize::Size8 => RadixTree::new(),
                AccessSize::Size16 => RadixTree::new(),
                AccessSize::Size32 => RadixTree::new(),
                AccessSize::Size64 => RadixTree::new(),
            },
            fillers: array![|idx| ArrayField::internal_new(&format!("Bus::filler{}", idx), idx as u8, 64, false); 256],
            unmap_r: unmapped_area_r(),
            unmap_w: unmapped_area_w(),
            logger: logger,
            mems: Vec::new(),
            phantom: PhantomData,
        })
    }

    pub fn read<U: MemInt + 'a>(&self, addr: u32) -> U {
        self.internal_fetch_read::<U>(addr, true)
            .read::<Order, U>(addr)
    }

    pub fn write<U: MemInt + 'a>(&mut self, addr: u32, val: U) {
        self.internal_fetch_write::<U>(addr, true)
            .write::<Order, U>(addr, val);
    }

    #[inline(never)]
    pub fn fetch_read<U: MemInt + 'a>(&self, addr: u32) -> MemIoR<Order, U> {
        self.internal_fetch_read::<U>(addr, true).at(addr)
    }

    #[inline(never)]
    pub fn fetch_write<U: MemInt + 'a>(&mut self, addr: u32) -> MemIoW<Order, U> {
        self.internal_fetch_write::<U>(addr, true).at(addr)
    }

    #[inline(never)]
    pub fn fetch_read_nolog<U: MemInt + 'a>(&self, addr: u32) -> MemIoR<Order, U> {
        self.internal_fetch_read::<U>(addr, false).at(addr)
    }

    #[inline(never)]
    pub fn fetch_write_nolog<U: MemInt + 'a>(&mut self, addr: u32) -> MemIoW<Order, U> {
        self.internal_fetch_write::<U>(addr, false).at(addr)
    }

    #[inline(always)]
    fn internal_fetch_read<U: MemInt + 'a>(&'b self, addr: u32, unmapped_log: bool) -> &'b HwIoR {
        self.reads[U::ACCESS_SIZE]
            .lookup(addr)
            .or_else(|| {
                if unmapped_log {
                    error!(self.logger, "unmapped bus read"; o!("addr" => format!("0x{:x}", addr), "size" => U::SIZE));
                }
                Some(&self.unmap_r)
            })
            .unwrap()
    }

    #[inline(always)]
    fn internal_fetch_write<U: MemInt + 'a>(
        &'b mut self,
        addr: u32,
        unmapped_log: bool,
    ) -> &'b mut HwIoW {
        if let Some(hwio) = self.writes[U::ACCESS_SIZE].lookup_mut(addr) {
            return hwio;
        }
        if unmapped_log {
            error!(self.logger, "unmapped bus write"; o!("addr" => format!("0x{:x}", addr), "size" => U::SIZE));
        }
        &mut self.unmap_w
    }

    fn mapreg_partial<U: 'static, S>(
        &mut self,
        addr: u32,
        reg: &Reg<Order, U>,
    ) -> Result<(), &'static str>
    where
        U: MemInt,
        S: MemInt + Into<U>,
    {
        self.reads[S::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            reg.hwio_r::<S>(),
            false,
        )?;
        self.writes[S::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            reg.hwio_w::<S>(),
            false,
        )?;
        Ok(())
    }

    pub fn map_reg<U>(&mut self, addr: u32, reg: &Reg<Order, U>) -> Result<(), &'static str>
    where
        U: MemInt,
        Reg<Order, U>: MappedReg<Order = Order>,
    {
        reg.map_into(self, addr)
    }

    /// Map a [`Mem`](struct.Mem.html) object into the bus. `begin`/`end` is the
    /// **inclusive** virtual address range onto which the memory will be
    /// mapped. `fill` specifies how to behave when the specified virtual
    /// address is longer than the physical size of `mem` (see
    /// [`BusFill`](enum.BusFill.html) documentation for more information),
    ///
    /// ```
    /// use emu::bus::le::{Bus, Mem, MemFlags, BusFill};
    /// use slog;
    ///
    /// fn main() {
    ///     let logger = slog::Logger::root(slog::Discard, slog::o!());
    ///     let mut bus = Bus::new(logger);
    ///
    ///     // Create a 1Kb RAM object
    ///     let ram1 = Mem::new("mem", 1024, MemFlags::default(), None);
    ///
    ///     // Map the memory over a 32Mb range of virtual address starting at
    ///     // 0x0400_0000, using mirroring.
    ///     bus.map_mem(0x0400_0000, 0x05FF_FFFF, &ram1, BusFill::Mirror).unwrap();
    ///
    ///     // Write a 32-bit value within the RAM
    ///     bus.write::<u32>(0x0400_0123, 0xaabbccdd);
    ///
    ///     // Verify that it can be read back from the same virtual address
    ///     // and from other mirrors, with different access sizes.
    ///     assert_eq!(bus.read::<u32>(0x0400_0123), 0xaabbccdd);
    ///     assert_eq!(bus.read::<u32>(0x05B8_A123), 0xaabbccdd);
    ///     assert_eq!(bus.read::<u8>(0x04BB_B125), 0xbb);
    /// }
    /// ```
    pub fn map_mem(
        &'b mut self,
        begin: u32,
        mut end: u32,
        mem: &'b Mem,
        fill: BusFill,
    ) -> Result<(), &'s str> {
        use self::AccessSize::*;
        use self::BusFill::*;

        if end < begin {
            return Err("Bus::map_mem: invalid arguments: end must be bigger than begin");
        }

        let vsize = end - begin + 1;
        let psize = mem.len() as u32;

        match fill {
            None => {
                if vsize > psize {
                    return Err("BusFill::None requires virtual size not to exceed physical size");
                }
            }
            Mirror => {
                if vsize % psize != 0 {
                    return Err(
                        "BusFill::Mirror requires virtual size to be a multiple of physical size",
                    );
                }
            }
            Fixed(val) => {
                // If there's virtual size beyond the physical size, map the
                // fixed-value filler in it.
                let pend = begin.saturating_add(psize - 1).min(end);
                if pend < end {
                    let fstart = pend.saturating_add(1);
                    let mask = self.fillers[val as usize].len().next_power_of_two() - 1;
                    let hwr =
                        unsafe { HwIoR::Mem(self.fillers[val as usize].clone(), mask as u32) };
                    self.reads[Size8].insert_range(fstart, end, hwr.clone(), false)?;
                    self.reads[Size16].insert_range(fstart, end, hwr.clone(), false)?;
                    self.reads[Size32].insert_range(fstart, end, hwr.clone(), false)?;
                    self.reads[Size64].insert_range(fstart, end, hwr.clone(), false)?;

                    let hww = unmapped_area_w();
                    self.writes[Size8].insert_range(fstart, end, hww.clone(), false)?;
                    self.writes[Size16].insert_range(fstart, end, hww.clone(), false)?;
                    self.writes[Size32].insert_range(fstart, end, hww.clone(), false)?;
                    self.writes[Size64].insert_range(fstart, end, hww.clone(), false)?;
                }

                // Fallthrough to normal mapping of Mem, but stops after
                // physical size finishes.
                end = pend;
            }
        }

        self.reads[Size8].insert_range(begin, end, mem.hwio_r::<u8>(), false)?;
        self.reads[Size16].insert_range(begin, end, mem.hwio_r::<u16>(), false)?;
        self.reads[Size32].insert_range(begin, end, mem.hwio_r::<u32>(), false)?;
        self.reads[Size64].insert_range(begin, end, mem.hwio_r::<u64>(), false)?;

        self.writes[Size8].insert_range(begin, end, mem.hwio_w::<Order, u8>(), false)?;
        self.writes[Size16].insert_range(begin, end, mem.hwio_w::<Order, u16>(), false)?;
        self.writes[Size32].insert_range(begin, end, mem.hwio_w::<Order, u32>(), false)?;
        self.writes[Size64].insert_range(begin, end, mem.hwio_w::<Order, u64>(), false)?;

        // Keep a cache of descriptions to implement mapped_mems().
        self.mems.push(MemoryDesc {
            name: mem.name().to_owned(),
            begin: begin as u64,
            end: end as u64,
            readable: mem.can_read(),
            writeable: mem.can_write(),
        });

        return Ok(());
    }

    pub fn map_device<T>(&'b mut self, base: u32, device: &T, bank: usize) -> Result<(), &'s str>
    where
        T: Device<Order = Order>,
    {
        device.dev_map(self, bank, base)
    }

    // Add a memory map for a "combiner": that is, an internal function that combines two
    // half-sized memory accesses into a larger word. For instance, if two reg16 are mapped
    // at addresses 0x8 and 0xA, we want a 32-bit read at 0x8 to combine both registers
    // into a single 32-bit word.
    // NOTE: the order of memory access *matters*, especially if the access size is
    // larger than the physical bus size. For instance, calling a 64-bit read in a bus
    // connected to a 32-bit CPU actually simulates two physical 32-bit accesses.
    // This function guarantees that accesses happens in address order irrespective
    // of the endianess (that is, in the above example, 0x8 is read before 0xA).
    fn map_combine<U: MemInt + 'static>(&mut self, addr: u32) -> Result<(), &'static str> {
        let before = self.fetch_read_nolog::<U::Half>(addr);
        let after = self.fetch_read_nolog::<U::Half>(addr + (mem::size_of::<U>() as u32) / 2);

        self.reads[U::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            HwIoR::Func(Rc::new(move |_| {
                let val_before = before.read();
                let val_after = after.read();
                U::from_halves::<Order>(val_before, val_after).into()
            })),
            true, // a combiner might overwrite if already existing
        )?;

        let off: u32 = (mem::size_of::<U>() as u32) / 2;
        let mut before = self.fetch_write_nolog::<U::Half>(addr);
        let mut after = self.fetch_write_nolog::<U::Half>(addr + off);

        self.writes[U::ACCESS_SIZE].insert_range(
            addr,
            addr + U::SIZE as u32 - 1,
            HwIoW::Func(Rc::new(RefCell::new(move |_, val64| {
                let (mask1, shift1) = Order::subint_mask::<U, U::Half>(0);
                let (mask2, shift2) = Order::subint_mask::<U, U::Half>(off as usize);
                let val_before = (val64 & mask1.into()) >> shift1;
                let val_after = (val64 & mask2.into()) >> shift2;
                before.write(U::Half::truncate_from(val_before));
                after.write(U::Half::truncate_from(val_after));
            }))),
            true, // a combiner might overwrite if already existing
        )?;

        Ok(())
    }

    /// Return a description of all memory areas that have been mapped to this bus.
    /// This can be useful for inspection and debugging.
    pub fn mapped_mems(&self) -> &Vec<MemoryDesc> {
        &self.mems
    }
}

pub trait MappedReg {
    type Order: ByteOrderCombiner;
    fn map_into(&self, bus: &mut Bus<Self::Order>, addr: u32) -> Result<(), &'static str>;
}

impl<O: ByteOrderCombiner + 'static> MappedReg for Reg<O, u8> {
    type Order = O;
    fn map_into(&self, bus: &mut Bus<Self::Order>, addr: u32) -> Result<(), &'static str> {
        bus.mapreg_partial::<u8, u8>(addr, self)?;
        bus.map_combine::<u16>(addr & !1)?;
        bus.map_combine::<u32>(addr & !3)?;
        bus.map_combine::<u64>(addr & !7)?;
        Ok(())
    }
}

impl<O: ByteOrderCombiner + 'static> MappedReg for Reg<O, u16> {
    type Order = O;
    fn map_into(&self, bus: &mut Bus<Self::Order>, addr: u32) -> Result<(), &'static str> {
        bus.mapreg_partial::<u16, u8>(addr, self)?;
        bus.mapreg_partial::<u16, u16>(addr, self)?;
        bus.map_combine::<u32>(addr & !3)?;
        bus.map_combine::<u64>(addr & !7)?;
        Ok(())
    }
}

impl<O: ByteOrderCombiner + 'static> MappedReg for Reg<O, u32> {
    type Order = O;
    fn map_into(&self, bus: &mut Bus<Self::Order>, addr: u32) -> Result<(), &'static str> {
        bus.mapreg_partial::<u32, u8>(addr, self)?;
        bus.mapreg_partial::<u32, u16>(addr, self)?;
        bus.mapreg_partial::<u32, u32>(addr, self)?;
        bus.map_combine::<u64>(addr & !7)?;
        Ok(())
    }
}

impl<O: ByteOrderCombiner + 'static> MappedReg for Reg<O, u64> {
    type Order = O;
    fn map_into(&self, bus: &mut Bus<Self::Order>, addr: u32) -> Result<(), &'static str> {
        bus.mapreg_partial::<u64, u8>(addr, self)?;
        bus.mapreg_partial::<u64, u16>(addr, self)?;
        bus.mapreg_partial::<u64, u32>(addr, self)?;
        bus.mapreg_partial::<u64, u64>(addr, self)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::le::{Reg16, Reg32, Reg8, RegFlags};
    use crate::log::new_console_logger;

    extern crate byteorder;
    use self::byteorder::{BigEndian, LittleEndian};
    use super::super::mem::MemFlags;

    fn logger() -> slog::Logger {
        new_console_logger()
    }

    #[test]
    fn basic_mem_mirror() {
        let ram1 = Mem::new("mem", 1024, MemFlags::default(), None);
        let mut bus = Bus::<LittleEndian>::new(logger());

        // Mapping a vsize which is not a multiple of psize returns an error
        assert_eq!(
            bus.map_mem(0x0400_0000, 0x0600_0000, &ram1, BusFill::Mirror)
                .is_ok(),
            false
        );

        // Correct mirror mapping
        assert_eq!(
            bus.map_mem(0x0400_0000, 0x05FF_FFFF, &ram1, BusFill::Mirror)
                .is_ok(),
            true
        );
        bus.write::<u32>(0x04000123, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x04000123), 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x05aaa123), 0xaabbccdd);
        assert_eq!(bus.read::<u8>(0x04bbb125), 0xbb);
    }

    #[test]
    fn basic_mem_fillnone() {
        let ram1 = Mem::new("mem", 1024, MemFlags::default(), None);
        let mut bus = Bus::<LittleEndian>::new(logger());

        assert_eq!(
            bus.map_mem(0x0400_0000, 0x0600_0000, &ram1, BusFill::None)
                .is_ok(),
            false
        );

        // Smaller size is OK
        assert_eq!(
            bus.map_mem(0x0400_0000, 0x0400_00FF, &ram1, BusFill::None)
                .is_ok(),
            true
        );

        bus.write::<u32>(0x0400_0020, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0400_0020), 0xaabb_ccdd);
        bus.write::<u32>(0x0400_0100, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0400_0100), 0xffff_ffff); // open bus

        // Correct size
        assert_eq!(
            bus.map_mem(0x0500_0000, 0x0500_03FF, &ram1, BusFill::None)
                .is_ok(),
            true
        );

        bus.write::<u32>(0x0500_0300, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x05000300), 0xaabbccdd);
    }

    #[test]
    fn basic_mem_fillfixed() {
        let ram1 = Mem::new("mem", 1024, MemFlags::default(), None);
        let mut bus = Bus::<LittleEndian>::new(logger());

        // Shorter mapping
        assert_eq!(
            bus.map_mem(0x0400_0000, 0x0400_000F, &ram1, BusFill::None)
                .is_ok(),
            true
        );

        bus.write::<u32>(0x0400_0008, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0400_0008), 0xaabb_ccdd);
        assert_eq!(bus.read::<u32>(0x0400_0010), 0xffff_ffff);
        bus.write::<u32>(0x0400_0010, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0400_0010), 0xffff_ffff);

        // Longer mapping
        assert_eq!(
            bus.map_mem(0x0500_0000, 0x05ff_ffff, &ram1, BusFill::Fixed(0x6C))
                .is_ok(),
            true
        );

        bus.write::<u32>(0x0500_0100, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0500_0100), 0xaabb_ccdd);
        assert_eq!(bus.read::<u32>(0x0500_1000), 0x6c6c_6c6c);
        bus.write::<u32>(0x0500_1000, 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0x0500_1000), 0x6c6c_6c6c);
    }

    #[test]
    fn basic_reg() {
        let mut reg1 = Reg32::new_basic("reg1");
        reg1.set(0x12345678);

        let reg2 = Reg32::new(
            "reg2",
            0x12345678,
            0xffff0000,
            RegFlags::default(),
            None,
            Some(Rc::new(box |x| x | 0xf0)),
        );

        let mut bus = Bus::<LittleEndian>::new(logger());
        assert_eq!(bus.map_reg(0x04000120, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg(0x04000124, &reg2).is_ok(), true);

        assert_eq!(bus.fetch_read::<u32>(0x04000120).is_mem(), true);
        assert_eq!(bus.fetch_read::<u16>(0x04000122).is_mem(), true);
        assert_eq!(bus.fetch_read::<u8>(0x04000121).is_mem(), true);
        assert_eq!(bus.fetch_read::<u32>(0x04000124).is_mem(), false); // callback
        assert_eq!(bus.fetch_read::<u8>(0x04000125).is_mem(), false); // callback

        assert_eq!(bus.read::<u32>(0x04000120), 0x12345678);
        assert_eq!(bus.read::<u16>(0x04000122), 0x1234);
        assert_eq!(bus.read::<u8>(0x04000121), 0x56);

        assert_eq!(bus.read::<u32>(0x04000124), 0x123456f8);
        bus.write::<u8>(0x04000124, 0x00);
        bus.write::<u16>(0x04000126, 0xaabb);
        assert_eq!(bus.read::<u32>(0x04000124), 0xaabb56f8);

        assert_eq!(bus.read::<u64>(0x04000120), 0xaabb56f812345678);
        bus.write::<u64>(0x04000120, 0x0);

        assert_eq!(bus.read::<u32>(0x04000120), 0x00000000);
        assert_eq!(bus.read::<u32>(0x04000124), 0x000056f8);
    }

    #[test]
    fn combiner_le() {
        let reg1 = Reg32::new_basic("reg1");
        let reg2 = Reg16::new_basic("reg2");
        let reg3 = Reg8::new_basic("reg3");
        let reg4 = Reg8::new_basic("reg4");

        let mut bus = Bus::<LittleEndian>::new(logger());
        assert_eq!(bus.map_reg(0xFF000000, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000004, &reg2).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000006, &reg3).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000007, &reg4).is_ok(), true);

        bus.write::<u64>(0xFF000000, 0xaabbccdd11223344);
        assert_eq!(reg1.get(), 0x11223344);
        assert_eq!(reg2.get(), 0xccdd);
        assert_eq!(reg3.get(), 0xbb);
        assert_eq!(reg4.get(), 0xaa);
        assert_eq!(bus.read::<u64>(0xFF000000), 0xaabbccdd11223344);
        assert_eq!(bus.read::<u32>(0xFF000000), 0x11223344);
        assert_eq!(bus.read::<u32>(0xFF000004), 0xaabbccdd);
        assert_eq!(bus.read::<u16>(0xFF000004), 0xccdd);
        assert_eq!(bus.read::<u16>(0xFF000006), 0xaabb);
        assert_eq!(bus.read::<u8>(0xFF000006), 0xbb);
        assert_eq!(bus.read::<u8>(0xFF000007), 0xaa);

        bus.write::<u32>(0xFF000004, 0x66778899);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x66778899);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x8899);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x6677);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x77);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x66);

        bus.write::<u16>(0xFF000006, 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x1122);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x22);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x11);
    }

    #[test]
    fn combiner_be() {
        use crate::bus::be::{Reg16, Reg32, Reg8};
        let reg1 = Reg32::new_basic("reg1");
        let reg2 = Reg16::new_basic("reg2");
        let reg3 = Reg8::new_basic("reg3");
        let reg4 = Reg8::new_basic("reg4");

        let mut bus = Bus::<BigEndian>::new(logger());

        assert_eq!(bus.map_reg(0xFF000000, &reg1).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000004, &reg2).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000006, &reg3).is_ok(), true);
        assert_eq!(bus.map_reg(0xFF000007, &reg4).is_ok(), true);

        bus.write::<u64>(0xFF000000, 0xaabbccdd11223344);
        assert_eq!(reg1.get(), 0xaabbccdd);
        assert_eq!(reg2.get(), 0x1122);
        assert_eq!(reg3.get(), 0x33);
        assert_eq!(reg4.get(), 0x44);
        assert_eq!(bus.read::<u64>(0xFF000000), 0xaabbccdd11223344);
        assert_eq!(bus.read::<u32>(0xFF000000), 0xaabbccdd);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x11223344);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x3344);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x33);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x44);

        bus.write::<u32>(0xFF000004, 0x66778899);
        assert_eq!(bus.read::<u32>(0xFF000004), 0x66778899);
        assert_eq!(bus.read::<u16>(0xFF000004), 0x6677);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x8899);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x88);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x99);

        bus.write::<u16>(0xFF000006, 0x1122);
        assert_eq!(bus.read::<u16>(0xFF000006), 0x1122);
        assert_eq!(bus.read::<u8>(0xFF000006), 0x11);
        assert_eq!(bus.read::<u8>(0xFF000007), 0x22);
    }
}
