use byteorder::{BigEndian, ByteOrder, LittleEndian, NativeEndian};
use num::PrimInt;
use num_traits::{WrappingAdd, WrappingSub};
use std::marker::PhantomData;
use std::ops::Shr;
use typenum;

/// A trait for a type that can be used to represent a single sample.
/// It is implemented for `u8`, `u16`, `i8`, `i16`. It is normally used as
/// part of [`SampleFormat`](trait.SampleFormat.html).
///
/// Signed integers assume that the center is at 0, while unsigned integers
/// have the center at 0x80 / 0x8000.
///
/// You can use [`SampleInt::sconv()`](trait.SampleInt.html#method.sconv) to
/// perform a semantically-correct conversion between different `SampleInt`.
pub trait SampleInt: PrimInt + WrappingAdd + WrappingSub + Send {
    /// The size of type in bytes
    const SIZE: usize;

    /// Signedness of the type
    const SIGNED: bool;

    /// Read the sample from the specified memory slice, in the specified byte order
    fn read<O: ByteOrder>(buf: &[u8]) -> Self;

    /// Write the sample to the specified memory slice, in the specified byte order
    fn write<O: ByteOrder>(buf: &mut [u8], v: Self);

    /// Convert the sample to u16. This is a helper to simplify writing code
    /// that is generic over the `SampleInt`: you can simply convert the sample
    /// to u16, do the required processing, and convert back using
    /// [`SampleInt::from_u16`](trait.SampleInt.html#method.from_u16).
    fn to_u16(self) -> u16;

    /// Convert from u16 into sample size. See
    /// [`SampleInt::to_u16`](trait.SampleInt.html#method.to_u16) for the
    /// rationale.
    fn from_u16(v: u16) -> Self;

    /// Convert to a different `SampleInt`. This conversion is semantically
    /// correct for an audio sample: signed/unsigned conversion is done by
    /// adjusting the center.
    ///
    /// ```rust
    /// use emu::snd::SampleInt;
    ///
    /// fn main() {
    /// 	let x: u8 = 128;
    /// 	let y: i16 = x.sconv();
    /// 	assert_eq!(y, 0i16);
    /// }
    /// ```
    fn sconv<S2: SampleInt>(self) -> S2 {
        S2::from_u16(self.to_u16())
    }
}

impl SampleInt for u8 {
    const SIZE: usize = 1;
    const SIGNED: bool = false;
    fn read<O: ByteOrder>(buf: &[u8]) -> Self {
        buf[0]
    }
    fn write<O: ByteOrder>(buf: &mut [u8], v: Self) {
        buf[0] = v;
    }
    fn to_u16(self) -> u16 {
        (self as u16) << 8
    }
    fn from_u16(v: u16) -> Self {
        (v >> 8) as u8
    }
}
impl SampleInt for i8 {
    const SIZE: usize = 1;
    const SIGNED: bool = true;
    fn read<O: ByteOrder>(buf: &[u8]) -> Self {
        buf[0] as i8
    }
    fn write<O: ByteOrder>(buf: &mut [u8], v: Self) {
        buf[0] = v as u8;
    }
    fn to_u16(self) -> u16 {
        #[allow(overflowing_literals)]
        return ((self ^ 0x80) as u8).to_u16();
    }
    fn from_u16(v: u16) -> Self {
        #[allow(overflowing_literals)]
        return (u8::from_u16(v) ^ 0x80) as i8;
    }
}
impl SampleInt for u16 {
    const SIZE: usize = 2;
    const SIGNED: bool = false;
    fn read<O: ByteOrder>(buf: &[u8]) -> Self {
        O::read_u16(buf)
    }
    fn write<O: ByteOrder>(buf: &mut [u8], v: Self) {
        O::write_u16(buf, v);
    }
    fn to_u16(self) -> u16 {
        self
    }
    fn from_u16(v: u16) -> Self {
        v
    }
}
impl SampleInt for i16 {
    const SIZE: usize = 2;
    const SIGNED: bool = true;
    fn read<O: ByteOrder>(buf: &[u8]) -> Self {
        O::read_i16(buf)
    }
    fn write<O: ByteOrder>(buf: &mut [u8], v: Self) {
        O::write_i16(buf, v);
    }
    fn to_u16(self) -> u16 {
        #[allow(overflowing_literals)]
        return (self ^ 0x8000) as u16;
    }
    fn from_u16(v: u16) -> Self {
        #[allow(overflowing_literals)]
        return (v ^ 0x8000) as i16;
    }
}

/// `SampleFormat` is a trait that represents the format of frames within a
/// sound buffer. It is used as generic parameter for instantiating a
/// [`SndBuffer`](struct.SndBuffer.html).
///
/// Implementations of this trait are used purely as a type, they are never
/// instantiated.
///
/// There are several implementations of this trait for common
/// sample formats, see [`SndBuffer documentation`](struct.SndBuffer.html).
/// User code should not implement this trait, and only use provided
/// implementations.
pub trait SampleFormat: Sized + Send + 'static {
    /// The integer type of a sample (eg: u8).
    type SAMPLE: SampleInt;

    /// The byteorder in which samples are stored. This is meaningless for
    /// 8-bit samples.
    type ORDER: ByteOrder + Send;

    /// The number of channels that a frame is composed of. The only supported
    /// values„ for this constant are 1 and 2.
    const CHANNELS: usize;

    /// The size in bytes of an audio frame (computed as size of a sample
    /// multiplied by number of channels).
    fn frame_size() -> usize {
        Self::SAMPLE::SIZE * Self::CHANNELS
    }
}

#[derive(Clone)]
pub struct OwnedSndBuffer<SF: SampleFormat> {
    buf: Vec<u8>,
    phantom: PhantomData<SF>,
}

pub struct SndBuffer<'a, SF: SampleFormat> {
    buf: &'a [u8],
    phantom: PhantomData<SF>,
}

pub struct SndBufferMut<'a, SF: SampleFormat> {
    buf: &'a mut [u8],
    phantom: PhantomData<SF>,
}

impl<SF: SampleFormat> OwnedSndBuffer<SF> {
    pub fn new(buf: Vec<u8>) -> Result<Self, &'static str> {
        if buf.len() % SF::frame_size() != 0 {
            return Err("invalid sound buffer size (not multiple of frame size");
        }
        return Ok(Self {
            buf,
            phantom: PhantomData,
        });
    }

    pub fn with_capacity(nframes: usize) -> Self {
        let mut v = Vec::new();
        v.resize(nframes * SF::frame_size(), 0u8);
        Self {
            buf: v,
            phantom: PhantomData,
        }
    }

    /// Return the number of frames in this buffer
    pub fn count(&self) -> usize {
        self.buf.len() / SF::frame_size()
    }

    /// Convert into a different buffer format
    pub fn sconv<SF2: SampleFormat>(&self) -> OwnedSndBuffer<SF2> {
        let mut dst = OwnedSndBuffer::with_capacity(self.count());
        self.buf().sconv_into(&mut dst.buf_mut()).unwrap();
        dst
    }

    pub fn buf<'a>(&'a self) -> SndBuffer<'a, SF> {
        SndBuffer::new(&self.buf[..])
    }
    pub fn buf_mut<'a>(&'a mut self) -> SndBufferMut<'a, SF> {
        SndBufferMut::new(&mut self.buf[..])
    }
}

impl<'a, SF: SampleFormat> SndBuffer<'a, SF> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            phantom: PhantomData,
        }
    }
    pub fn count(&self) -> usize {
        self.buf.len() / SF::frame_size()
    }
    pub fn get_sample(&self, nframe: usize, nchan: usize) -> SF::SAMPLE {
        let off = nframe * SF::frame_size() + nchan * SF::SAMPLE::SIZE;
        SF::SAMPLE::read::<SF::ORDER>(&self.buf[off..off + SF::SAMPLE::SIZE])
    }
    pub fn sconv_into<SF2: SampleFormat>(&self, dst: &mut SndBufferMut<SF2>) -> Result<(), String> {
        let nframes = self.count();
        if dst.count() != nframes {
            return Err("SndBuffer::sconv_into: found buffers of different size".to_string());
        }

        for i in 0..nframes {
            match (SF::CHANNELS, SF2::CHANNELS) {
                (1, 1) => {
                    let s = self.get_sample(i, 0).sconv();
                    dst.set_sample(i, 0, s);
                }
                (2, 2) => {
                    let s1 = self.get_sample(i, 0).sconv();
                    let s2 = self.get_sample(i, 1).sconv();
                    dst.set_sample(i, 0, s1);
                    dst.set_sample(i, 1, s2);
                }
                (1, 2) => {
                    let s = self.get_sample(i, 0).sconv();
                    dst.set_sample(i, 0, s);
                    dst.set_sample(i, 1, s);
                }
                (2, 1) => {
                    let s1 = self.get_sample(i, 0);
                    let s2 = self.get_sample(i, 1);
                    let s = (s1.shr(1)).wrapping_add(&s2.shr(1));
                    dst.set_sample(i, 0, s.sconv());
                }
                _ => unimplemented!(),
            }
        }

        Ok(())
    }
}

impl<'a, SF: SampleFormat> SndBufferMut<'a, SF> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            phantom: PhantomData,
        }
    }

    fn buf(&'a self) -> SndBuffer<'a, SF> {
        SndBuffer::new(self.buf)
    }

    pub fn count(&self) -> usize {
        self.buf().count()
    }
    pub fn get_sample(&self, nframe: usize, nchan: usize) -> SF::SAMPLE {
        self.buf().get_sample(nframe, nchan)
    }
    pub fn sconv_into<SF2: SampleFormat>(&self, dst: &mut SndBufferMut<SF2>) -> Result<(), String> {
        self.buf().sconv_into(dst)
    }

    pub fn set_sample(&mut self, nframe: usize, nchan: usize, val: SF::SAMPLE) {
        let off = nframe * SF::frame_size() + nchan * SF::SAMPLE::SIZE;
        SF::SAMPLE::write::<SF::ORDER>(&mut self.buf[off..off + SF::SAMPLE::SIZE], val);
    }
}

impl<'a, SF> AsRef<[SF::SAMPLE]> for SndBuffer<'a, SF>
where
    SF: SampleFormat<ORDER = NativeEndian>,
{
    // Return the raw memory buffer, as a slice of the correct sample type.
    //
    // Notice that this method is only available for buffers whose
    // `SampleFormat` has the same byte order of the host system (eg:
    // `LittleEndian` on x86). If you need to be more generic, use the sample
    // accessors instead
    // ([`get_sample`](struct.SndBuffer.html#method.get_sample) and
    // [`set_sample`](struct.SndBuffer.html#method.set_sample)).
    fn as_ref(&self) -> &[SF::SAMPLE] {
        unsafe { ::std::mem::transmute(&*self.buf) }
    }
}

impl<'a, SF> AsMut<[SF::SAMPLE]> for SndBufferMut<'a, SF>
where
    SF: SampleFormat<ORDER = NativeEndian>,
{
    // Return the raw memory buffer, as a mutable slice of the correct sample
    // type.
    //
    // See [`as_ref`](struct.SndBuffer.html#method.as_ref) for more information.
    fn as_mut(&mut self) -> &mut [SF::SAMPLE] {
        unsafe { ::std::mem::transmute(&mut *self.buf) }
    }
}

#[allow(non_camel_case_types)]
pub struct sf<S: SampleInt, O: ByteOrder, C: typenum::Unsigned> {
    phantom: PhantomData<(S, O, C)>,
}

impl<C, S, O> SampleFormat for sf<S, O, C>
where
    S: SampleInt + Send + 'static,
    O: ByteOrder + Send + 'static,
    C: typenum::Unsigned + Send + 'static,
{
    type SAMPLE = S;
    type ORDER = O;
    const CHANNELS: usize = C::USIZE;
}

#[allow(non_camel_case_types)]
pub type U8_MONO = sf<u8, LittleEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type U8_STEREO = sf<u8, LittleEndian, typenum::U2>;
#[allow(non_camel_case_types)]
pub type S8_MONO = sf<i8, LittleEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type S8_STEREO = sf<i8, LittleEndian, typenum::U2>;
#[allow(non_camel_case_types)]
pub type U16LE_MONO = sf<u16, LittleEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type U16LE_STEREO = sf<u16, LittleEndian, typenum::U2>;
#[allow(non_camel_case_types)]
pub type U16BE_MONO = sf<u16, BigEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type U16BE_STEREO = sf<u16, BigEndian, typenum::U2>;
#[allow(non_camel_case_types)]
pub type S16LE_MONO = sf<i16, LittleEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type S16LE_STEREO = sf<i16, LittleEndian, typenum::U2>;
#[allow(non_camel_case_types)]
pub type S16BE_MONO = sf<i16, BigEndian, typenum::U1>;
#[allow(non_camel_case_types)]
pub type S16BE_STEREO = sf<i16, BigEndian, typenum::U2>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let mut obuf = OwnedSndBuffer::<U8_MONO>::with_capacity(4);

        let mut buf = obuf.buf_mut();
        buf.set_sample(0, 0, 0x11);
        buf.set_sample(1, 0, 0x88);
        buf.set_sample(2, 0, 0xCC);
        buf.set_sample(3, 0, 0xFF);

        let odst = obuf.sconv::<S16BE_STEREO>();
        let dst = odst.buf();
        assert_eq!(dst.count(), 4);
        assert_eq!(dst.get_sample(0, 0) as u16, 0x9100);
        assert_eq!(dst.get_sample(0, 1) as u16, 0x9100);
        assert_eq!(dst.get_sample(1, 0) as u16, 0x0800);
        assert_eq!(dst.get_sample(1, 1) as u16, 0x0800);
        assert_eq!(dst.get_sample(2, 0) as u16, 0x4C00);
        assert_eq!(dst.get_sample(2, 1) as u16, 0x4C00);
        assert_eq!(dst.get_sample(3, 0) as u16, 0x7F00);
        assert_eq!(dst.get_sample(3, 1) as u16, 0x7F00);
    }
}
