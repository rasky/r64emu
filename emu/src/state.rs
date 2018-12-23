//! A module that allows to implement save states in emulators.
//!
//! ## Design
//!
//! This module implements the concept of a global thread-local `State`, from
//! which specific variables called `Field` can be allocated. You can think
//! of a `State` like an arena allocator, and a `Field` is a pointer to an object
//! allocated within it. When `State` is replaced with a different instance,
//! all `Fields` objects are transparently updated with the new value.
//!
//! Each `Field` must also have a unique name, that is used as a key while
//! serializing the state. Failure to use unique names will result in runtime
//! panics (hopefully at startup).
//!
//! `emu::Bus::Mem` and `emu::bus::Reg` internally use `Field` to store their
//! contents, so all memory areas and hardware registers defined in the emulator
//! are already part of the `State`.
//!
//! ## Fields
//!
//! There are three different kind of fields that can be used:
//!
//! * `Field`: can be used for objects that implement Copy and Serialize, eg:
//!   `Field<u64>`, `Field<MyStruct(u32, u64)>`. It implements `Deref` and
//!   `DerefMut`, thus behaving like a smart pointer.
//! * `ArrayField`: a fixed-size array of Fields. It derefs as a slice.
//! * `EndianField`: can be used for integers that must be saved in a specific
//!   endianess, maybe because they need to be accessed at the byte level. It
//!   exposes a `into_array_field` method to access the byte-level representation.
//!
//! Notice that all fields implement `Default` but their default state is invalid
//! and will panic if accessed. It should be used only as a placeholder in structs
//! in case delayed initialization is required.
//!
//!

use crate::bus::{ByteOrderCombiner, MemInt};

use futures::*;
use lz4;
use serde::Serialize;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::thread;

/// A `Field` is an object that is part of the emulator state. It is a lightweight
/// pointer into the current state, and can be used to mutate the state itself.
///
/// The type of the object pointed by `Field` (`F`) must implement the Copy
/// trait (because its contents will be copied around when snapshotting the state),
/// and the Serialize trait (to allow for long term persistence).
///
/// Cloning a Field creates another field pointing to the same content.
#[derive(Clone)]
pub struct Field<F: Copy + Serialize> {
    offset: usize,
    phantom: PhantomData<F>,
}

// A field refers implicitly to the current thread's State, thus we cannot
// Send it across threads.
impl<F> !Send for Field<F> {}

impl<F: Copy + Serialize> Field<F> {
    /// Create a new Field with the specified name and initial value.
    ///
    /// # Panics
    ///
    /// This function will panic if the name
    pub fn new(name: &'static str, f: F) -> Self {
        CurrentState().new_field(name, f)
    }
}

impl<F: Copy + Serialize> Default for Field<F> {
    // Default returns an invalid Field, that will cause a panic when used.
    // It can be used as placeholder in structs until proper initialization
    // is performed.
    fn default() -> Self {
        Self {
            offset: usize::max_value(),
            phantom: PhantomData,
        }
    }
}

impl<F: Copy + Serialize> Deref for Field<F> {
    type Target = F;

    fn deref(&self) -> &F {
        let state = CurrentState();
        let data = &state.data[self.offset];
        unsafe { mem::transmute(data) }
    }
}

impl<F: Copy + Serialize> DerefMut for Field<F> {
    fn deref_mut(&mut self) -> &mut F {
        let state = CurrentState();
        let data = &mut state.data[self.offset];
        unsafe { mem::transmute(data) }
    }
}

/// A Field which is an integer, stored with the specified endianess.
#[derive(Clone)]
pub struct EndianField<F: Copy + Serialize + MemInt, O: ByteOrderCombiner> {
    offset: usize,
    phantom: PhantomData<(F, O)>,
}

impl<F, O> !Send for EndianField<F, O> {}

impl<F: Copy + Serialize + MemInt, O: ByteOrderCombiner> EndianField<F, O> {
    pub fn new(name: &'static str, f: F) -> Self {
        CurrentState().new_endian_field(name, f)
    }
    pub fn into_array_field(self) -> ArrayField<u8> {
        ArrayField {
            offset: self.offset,
            len: mem::size_of::<F>(),
            phantom: PhantomData,
        }
    }

    fn deref(&self) -> &F {
        let state = CurrentState();
        let data = &state.data[self.offset];
        unsafe { mem::transmute(data) }
    }

    fn deref_mut(&mut self) -> &mut F {
        let state = CurrentState();
        let data = &mut state.data[self.offset];
        unsafe { mem::transmute(data) }
    }

    pub fn get(&self) -> F {
        O::to_native(*self.deref())
    }
    pub fn set(&mut self, val: F) {
        *self.deref_mut() = O::to_native(val);
    }
}

impl<F: Copy + Serialize + MemInt, O: ByteOrderCombiner> Default for EndianField<F, O> {
    // Default returns an invalid Field, that will cause a panic when used.
    // It can be used as placeholder in structs until proper initialization
    // is performed.
    fn default() -> Self {
        Self {
            offset: usize::max_value(),
            phantom: PhantomData,
        }
    }
}

#[derive(Clone)]
pub struct ArrayField<F: Copy + Serialize> {
    offset: usize,
    len: usize,
    phantom: PhantomData<F>,
}

impl<F: Copy + Serialize> ArrayField<F> {
    /// Create an `ArrayField` with the specified name, initial value, and length.
    pub fn new(name: &'static str, f: F, len: usize) -> Self {
        CurrentState().new_array_field(name, f, len)
    }

    /// Return the number of elements in the array
    pub fn len(&self) -> usize {
        self.len
    }

    /// Similar to the Deref trait, but exposes the correct lifetimes so that
    /// the returned slice does not keep the ArrayField borrowed (as it refers
    /// to the external memory state).
    pub fn as_slice<'s, 'r: 's>(&'s self) -> &'r [F] {
        let state = CurrentState();
        let data = &state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }

    /// Similar to the DerefMut trait, but exposes the correct lifetimes so that
    /// the returned slice does not keep the ArrayField borrowed (as it refers
    /// to the external memory state).
    pub fn as_slice_mut<'s, 'r: 's>(&'s mut self) -> &'r mut [F] {
        let state = CurrentState();
        let data = &mut state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }
}

// A field refers implicitly to the current thread's State, thus we cannot
// Send it across threads.
impl<F> !Send for ArrayField<F> {}

impl<F: Copy + Serialize> Default for ArrayField<F> {
    /// Default returns an invalid ArrayField, that will cause a panic when used.
    /// It can be used as placeholder in structs until proper initialization
    /// is performed.
    fn default() -> Self {
        Self {
            len: 0,
            offset: usize::max_value(),
            phantom: PhantomData,
        }
    }
}

impl<F: Copy + Serialize> Deref for ArrayField<F> {
    type Target = [F];

    fn deref(&self) -> &[F] {
        self.as_slice()
    }
}

impl<F: Copy + Serialize> DerefMut for ArrayField<F> {
    fn deref_mut(&mut self) -> &mut [F] {
        self.as_slice_mut()
    }
}

thread_local!(static STATE: RefCell<State> = RefCell::new(State::new()));

/// Return the current `State` (for the current thread).
#[allow(non_snake_case)]
pub fn CurrentState() -> &'static mut State {
    let s: *const State = STATE.with(|s| &(*s.borrow()) as _);
    let s: *mut State = s as *mut State;
    unsafe { &mut *s }
}

/// State holds a serializable state for the emulator, composed from multiple
/// fields.
///
/// An empty state is automatically created for each new thread, and can be
/// accessed with `CurrentState`.
///
/// Cloning a `State` actually creates a copy of the whole state. Creating a new
/// empty `State` is forbidden (as it would be useless without Field definitions).
///
#[derive(Clone)]
pub struct State {
    data: Vec<u8>,
    info: HashMap<&'static str, (usize, usize)>,
}

#[inline]
fn round_up(base: usize, align: usize) -> usize {
    (base + (align - 1)) & !(align - 1)
}

impl State {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(1024),
            info: HashMap::default(),
        }
    }

    fn alloc_raw(&mut self, size: usize, align: usize) -> usize {
        let offset = round_up(self.data.len(), align);
        self.data.resize(offset + size, 0);
        offset
    }

    fn new_field<F: Copy + Serialize>(&mut self, name: &'static str, value: F) -> Field<F> {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let size = mem::size_of::<F>();
        let offset = self.alloc_raw(size, mem::align_of::<F>());
        self.info.insert(name, (offset, size));

        let mut f = Field {
            offset: offset,
            phantom: PhantomData,
        };
        *f = value;
        f
    }

    fn new_endian_field<F: Copy + Serialize + MemInt, O: ByteOrderCombiner>(
        &mut self,
        name: &'static str,
        value: F,
    ) -> EndianField<F, O> {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let size = mem::size_of::<F>();
        let offset = self.alloc_raw(size, mem::align_of::<F>());
        self.info.insert(name, (offset, size));

        let mut f = EndianField {
            offset: offset,
            phantom: PhantomData,
        };
        f.set(value);
        f
    }

    fn new_array_field<F: Copy + Serialize>(
        &mut self,
        name: &'static str,
        value: F,
        len: usize,
    ) -> ArrayField<F> {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let size = len * mem::size_of::<F>();
        let offset = self.alloc_raw(size, mem::align_of::<F>());
        self.info.insert(name, (offset, size));

        let mut f = ArrayField {
            offset: offset,
            len: len,
            phantom: PhantomData,
        };
        for v in (*f).iter_mut() {
            *v = value;
        }
        f
    }

    /// The size of the state, in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Make the current state as current, moving it.
    /// Returns the previously-current state.
    pub fn make_current(self) -> State {
        STATE.with(|s| {
            if s.borrow().len() != self.len() {
                panic!("State::make_current with different length")
            }
            s.replace(self)
        })
    }

    /// Convert the state into a `CompressedState`, consuming it. Notice
    /// that the compression is performed in a background thread.
    pub fn into_compressed(self) -> CompressedState {
        CompressedState::new(self)
    }
}

/// A compressed snapshot of a `State`, useful for in-process snapshotting.
/// To be made current, it must be decompressed back into a `State` using
/// `decompress`.
///
/// The [LZ4 algorithm](www.lz4.org) is used for the compression.
pub struct CompressedState {
    data: RefCell<Vec<u8>>,
    future_data: RefCell<Option<Oneshot<Vec<u8>>>>,
    info: HashMap<&'static str, (usize, usize)>,
}

impl CompressedState {
    fn new(state: State) -> CompressedState {
        let (c, p) = oneshot::<Vec<u8>>();

        // Compress the state with LZ4 in background; use a future
        // to track completion.
        let data = state.data;
        thread::spawn(move || {
            let mut compressed = Vec::new();
            let mut enc = lz4::EncoderBuilder::new()
                .checksum(lz4::ContentChecksum::NoChecksum)
                .level(9)
                .build(&mut compressed)
                .unwrap();
            io::copy(&mut &data[..], &mut enc).unwrap();
            enc.finish();

            c.send(compressed).unwrap();
        });

        CompressedState {
            data: RefCell::new(Vec::new()),
            future_data: RefCell::new(Some(p)),
            info: state.info,
        }
    }

    /// Decompress into a `State`.
    pub fn decompress(&self) -> State {
        let mut fut = self.future_data.borrow_mut();
        if fut.is_some() {
            *self.data.borrow_mut() = fut.take().unwrap().wait().unwrap();
        }

        let cdata = self.data.borrow();
        let mut dec = lz4::Decoder::new(&cdata[..]).unwrap();
        let mut udata = Vec::new();
        io::copy(&mut dec, &mut udata).unwrap();

        State {
            data: udata,
            info: self.info.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_derive::Serialize;

    #[test]
    fn normal_field() {
        let mut a = Field::new("a", 4u64);
        let mut b = Field::new("b", 12.0f64);

        assert_eq!(*a, 4);
        assert_eq!(*b, 12.0);
        assert_eq!(CurrentState().len(), 16);

        let s1 = CurrentState().clone();

        *a = 5;
        *b = 15.0;

        assert_eq!(*a, 5);
        assert_eq!(*b, 15.0);
        assert_eq!(CurrentState().len(), 16);

        let s2 = s1.make_current();
        assert_eq!(*a, 4);
        assert_eq!(*b, 12.0);

        s2.make_current();
        assert_eq!(*a, 5);
        assert_eq!(*b, 15.0);
    }

    #[test]
    fn normal_field_struct() {
        #[derive(Copy, Clone, Serialize)]
        struct Foo {
            bar: u8,
            baz: u32,
        }

        let mut f = Field::new("a", Foo { bar: 1, baz: 2 });
        assert_eq!((*f).bar, 1);
        assert_eq!((*f).baz, 2);

        let s1 = CurrentState().clone();
        (*f).bar = 3;
        (*f).baz = 4;
        assert_eq!((*f).bar, 3);
        assert_eq!((*f).baz, 4);

        let s2 = s1.make_current();
        assert_eq!((*f).bar, 1);
        assert_eq!((*f).baz, 2);

        s2.make_current();
        assert_eq!((*f).bar, 3);
        assert_eq!((*f).baz, 4);
    }

    #[test]
    fn endian_field() {
        use byteorder::{BigEndian, ByteOrder};

        let mut f = EndianField::<u64, BigEndian>::new("a", 12);
        let array = f.clone().into_array_field();

        let val = BigEndian::read_u64(&array);
        assert_eq!(f.get(), 12);
        assert_eq!(val, 12);

        f.set(15);
        let val = BigEndian::read_u64(&array);
        assert_eq!(f.get(), 15);
        assert_eq!(val, 15);
    }

    #[test]
    fn compress() {
        let mut a = Field::new("a", 4u64);
        let mut b = Field::new("b", 12.0f64);

        let s1 = CurrentState().clone().into_compressed();

        *a = 5;
        *b = 15.0;

        let s2 = s1.decompress().make_current().into_compressed();

        assert_eq!(*a, 4);
        assert_eq!(*b, 12.0);

        s2.decompress().make_current();
        assert_eq!(*a, 5);
        assert_eq!(*b, 15.0);

        *a = 0;
        *b = 0.0;
        s2.decompress().make_current();
        assert_eq!(*a, 5);
        assert_eq!(*b, 15.0);
    }
}
