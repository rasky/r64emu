//! A module that allows to implement save states in emulators.
//!
//! ## Design
//!
//! This module implements the concept of a global thread-local "state" of the
//! emulator, which contains all the data that needs to be serialized in order
//! to save and restore the emulation state. A state is composed of multiple
//! "fields", which are single variables (either basic types or structs) that
//! can be used anywhere within the emulator itself
//!
//! You can think of a state like an arena allocator, and a field is a
//! pointer to an object allocated within it. When the current global state is
//! replaced with a different instance, all fields objects are transparently
//! updated with the new value.
//!
//! The state is manipulated through the [`State`](struct.State.html) struct;
//! the current state can be accessed with
//! [`CurrentState()`](fn.CurrentState.html). A current state is automatically
//! created for each new thread, so no explicit initialization is required (not
//! even in tests). To populate a `State`, it is sufficient to create fields
//! using one of three available wrappers: [`Field`](struct.Field.html),
//! [`ArrayField`](struct.ArrayField.html), and
//! [`EndianField`](struct.EndianField.html). Creating a field automatically
//! binds it to the global state. Fields acts as smart pointers to the actual
//! content.
//!
//! Example: TODO.
//!
//! Since a state is defined implicitly as the aggregation of all fields, fields
//! are not meant to be instantiated and freed at runtime while the emulator
//! is running. It is expected that all fields are instantiated during the
//! emulator initial setup, and are then used (accessed, mutated) while the
//! emulator is running.
//!
//! Each field must also have a unique name, that is used as a key while
//! serializing the state. Failure to use unique names will result in runtime
//! panics (hopefully at startup, while fields are being created).
//!
//! `emu::Bus::Mem` and `emu::bus::Reg` internally use
//! [`ArrayField`](struct.ArrayField.html) and
//! [`EndianField`](struct.EndianField.html) to store their contents, so all
//! memory areas and hardware registers defined in the emulator are already part
//! of the `State`.
//!
//!
//! ## Fields
//!
//! Fields are allocated through one of the three smart pointers defined in this
//! module. For instance, `Field<u64>` defines a `u64` variable which is part
//! of the emulator state. As much as possible, smart pointers try to implement
//! `Deref` and `DerefMut` to make it easy to manipulate the underlying data.
//!
//! All types used as part of a field must be `'static` (so they cannot contain
//! non-static references), and they must implement the `Copy`,
//! `serde::Serialize` and `serde::Deserialize<'static>` traits. The simplest
//! way to think of these constraints is to only use aggregation of basic types
//! in fields.
//!
//! There are three different kind of fields that can be used:
//!
//! * [`Field`](struct.Field.html): this is most common and normal field, and
//!   can be used with any type that respects the basic field constraints. eg:
//!   `Field<u64>`, `Field<MyStruct(u32, u64)>`. It implements `Deref` and
//!   `DerefMut`, thus behaving like a smart pointer.
//! * [`ArrayField`](struct.ArrayField.html): a fixed-size array. It derefs as a
//!   slice.
//! * [`EndianField`](struct.EndianField.html): can be used for integers that
//!   must be saved in a specific endianess, maybe because they need to be
//!   accessed at the byte level. It exposes a
//!   [`into_array_field()`](struct.EndianField.html#method.into_array_field) method
//!   to access the byte-level representation.
//!
//! Notice that all fields implement `Default` but their default state is
//! invalid and will panic if accessed. It should be used only as a placeholder
//! in structs in case delayed initialization is required.
//!
//!
//! ## Saving and restoring the state
//!
//! There are two ways of saving a state: cloning it (aka "snapshot") and
//! serializing it.
//!
//! ### Snapshots
//!
//! Snapshotting is extremely fast (it is just a memcpy), but it
//! is not meant for long-term storage, since the memory buffer in the State
//! does not contain metadata (it's just a linear buffer) and the layout of the
//! fields within it depends on the order of construction (which is very
//! fragile). Nonetheless, it is a good choice for in-process snapshotting (eg:
//! for a debugger). A snapshot can also be compressed for reducing memory
//! occupation.
//!
//! A snapshot is just a [`State`](struct.State.html) instance. To snapshot the
//! current state, simply call `clone()` on the
//! [`CurrentState()`](fn.CurrentState.html), using the standard `Clone` trait.
//! To reload a snapshot, call
//! [`State::make_current()`](fn.State.make_current.html), that moves the state
//! into the global thread-local `State` instance; the previously-current
//! `State` is returned.
//!
//! To compress a snapshot, use
//! [`State::into_compress()`](fn.State.html#method.into_compress) to create a
//! [`CompressedState`](struct.CompressedState.html) instance, and
//! [`CompressedState::decompress()`](fn.CompressedState.html#method.decompress)
//! to reverse the process. Compression is currently implemented with
//! [LZ4](https://www.lz4.org), but this is considered an implementation detail,
//! as snapshots are not meant to be inspected or serialized.
//!
//! ### Serialization
//!
//! Serialization is relatively-slower operation that creates a binary
//! representation of the state meant for longer-term storage, and more resilient
//! to changes caused by further developments to the code base. The serialized
//! format includes metadata (the field names), that allow to attempt reloading
//! over a `State` that has slightly changed: in fact, while reloading, fields
//! that do not exist anymore will be ignored, and fields that are not present
//! in the serialization will keep their current value.
//!
//! Serialization includes also a program name (to be used as a magic string
//! to discern between save states of different emulators based on this crate),
//! and a version number. Attempting to deserialize with different magic string
//! or version number will result in an error.
//!
//! Serialization is currently performed using the
//! [MessagePack](https://msgpack.org) format, and then compressed using
//! [LZ4](https://www.lz4.org), but this is considered an implementation detail.
//! The serialization stream format is internally versioned, so changes to the
//! stream version in future version of this module (eg: changes to the
//! compression algorithm) will be gracefully handled without breaking
//! previously serialized states.
//!

use crate::memint::{ByteOrderCombiner, MemInt};

use futures::*;
use lz4;
use rmp_serde;
use serde::{Deserialize, Serialize};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::thread;

/// A `Field` is an object that is part of the emulator state. It is a lightweight
/// pointer into the current state, and can be used to mutate the state itself.
///
/// The type of the object pointed by `Field` (`F`) must implement the Copy
/// trait (because its contents will be copied around when snapshotting the state),
/// and the Serialize trait (to allow for long term persistence).
///
/// Cloning a Field creates another field pointing to the same content (aliasing).
#[derive(Clone)]
pub struct Field<F: Copy + Serialize + Deserialize<'static>> {
    offset: usize,
    phantom: PhantomData<F>,
}

// A field refers implicitly to the current thread's State, thus we cannot
// Send it across threads.
impl<F> !Send for Field<F> {}
impl<F> !Sync for Field<F> {}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> Field<F> {
    /// Create a new Field with the specified name and initial value.
    ///
    /// # Panics
    ///
    /// This function will panic if the name
    pub fn new(name: &'static str, f: F) -> Self {
        CurrentState().new_field(name, f)
    }
}

impl<F: Copy + Serialize + Deserialize<'static>> Default for Field<F> {
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

impl<F: Copy + Serialize + Deserialize<'static>> Deref for Field<F> {
    type Target = F;

    fn deref(&self) -> &F {
        let state = CurrentState();
        let data = &state.data[self.offset];
        unsafe { mem::transmute(data) }
    }
}

impl<F: Copy + Serialize + Deserialize<'static>> DerefMut for Field<F> {
    fn deref_mut(&mut self) -> &mut F {
        let state = CurrentState();
        let data = &mut state.data[self.offset];
        unsafe { mem::transmute(data) }
    }
}

/// `EndianField` is a `Field` of integer type (see `emu::MemInt`) with an
/// explicitly-specified memory representation endianess, with a sound byte-level
/// access. It is seldom necessary.
///
/// Normal fields are meant to be used by the emulator code, so their memory
/// representation follows normal Rust compiler rules and CPU architecture
/// conventions; for instance a `Field<u64>` is represented in memory like a
/// normal `u64` is. Sometimes, it might be required for an emulator to save
/// a value in memory in a specified endianess, so that byte-level access
/// is possible. In this case, `EndianField` can be used.
///
/// Compared to `Field`, `EndianField` does not implement `Deref` or `DerefMut`,
/// because it might be necessary to adjust the endianess while getting or
/// setting a value. Thus, all accesses must go through the
/// [`EndianField::get()`](struct.EndianField.html#method.get) and
/// [`EndianField::set()`](struct.EndianField.html#method.set) accessors.
///
/// Since the memory representation is explicitly defined, byte-level access is
/// possible and with a well-defined behavior. To access it, use
/// [`as_array_field()`](struct.EndianField.html#method.as_array_field) to
/// create an [`ArrayField`](struct.ArrayField.html) instance which points (is
/// aliased) to the same field.
///
/// Cloning an `EndianField` creates another field that points to the same
/// contents (aliasing).
#[derive(Clone)]
pub struct EndianField<F: Copy + Serialize + Deserialize<'static> + MemInt, O: ByteOrderCombiner> {
    offset: usize,
    phantom: PhantomData<(F, O)>,
}

impl<F, O> !Send for EndianField<F, O> {}
impl<F, O> !Sync for EndianField<F, O> {}

impl<F, O> EndianField<F, O>
where
    F: 'static + Copy + Serialize + Deserialize<'static> + MemInt,
    O: 'static + ByteOrderCombiner,
{
    pub fn new(name: &'static str, f: F) -> Self {
        CurrentState().new_endian_field(name, f)
    }
    pub fn as_array_field(&self) -> ArrayField<u8> {
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

/// `ArrayField` represents a fixed-size array of fields. The size can be
/// specified at runtime while constructing it, but cannot be changed after
/// construction. It implements the `Deref` and `DerefMut` trait that deref
/// as a slice.
///
/// `ArrayField` (like all fields) implements `Default` by returning an invalid
/// `ArrayField` that will panic as soon as it is accessed. This is used as a
/// placeholder in structs for delayed initialization, but should eventually be
/// replaced by a correctly-initialized instance returned by
/// [`ArrayField::new()`](struct.ArrayField.html#method.new).
#[derive(Clone)]
pub struct ArrayField<F: Copy + Serialize + Deserialize<'static>> {
    offset: usize,
    len: usize,
    phantom: PhantomData<F>,
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> ArrayField<F> {
    /// Create an `ArrayField` with the specified name, initial value, and length.
    /// NOTE: `serialize` is deprecated, always pass `true`.
    pub fn new(name: &'static str, f: F, len: usize, serialize: bool) -> Self {
        CurrentState().new_array_field(name, f, len, serialize)
    }

    /// Return the number of elements in the array
    pub fn len(&self) -> usize {
        self.len
    }

    /// `as_slice()` returns a slice to access the underlying array. It is
    /// similar to the Deref trait, but exposes the correct lifetimes so that
    /// the returned slice does not keep the `ArrayField` instance borrowed (as
    /// it actually borrows the actual memory within the state).
    pub fn as_slice<'s, 'r: 's>(&'s self) -> &'r [F] {
        let state = CurrentState();
        let data = &state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }

    /// `as_slice_mut()` is the mutable version of [`as_slice()`](struct.ArrayState.html#method.as_slice).
    pub fn as_slice_mut<'s, 'r: 's>(&'s mut self) -> &'r mut [F] {
        let state = CurrentState();
        let data = &mut state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }
}

// A field refers implicitly to the current thread's State, thus we cannot
// Send it across threads.
impl<F> !Send for ArrayField<F> {}
impl<F> !Sync for ArrayField<F> {}

impl<F: Copy + Serialize + Deserialize<'static>> Default for ArrayField<F> {
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

impl<F: 'static + Copy + Serialize + Deserialize<'static>> Deref for ArrayField<F> {
    type Target = [F];

    fn deref(&self) -> &[F] {
        self.as_slice()
    }
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> DerefMut for ArrayField<F> {
    fn deref_mut(&mut self) -> &mut [F] {
        self.as_slice_mut()
    }
}

thread_local!(static STATE: RefCell<State> = RefCell::new(State::new()));

/// Return a mutable reference to the current [`State`](struct.State.html) (for
/// the current thread).
///
/// An empty `State` instance is automatically created for each new thread, so
/// there is no need to perform an initialization before calling `CurrentState()`.
///
/// Currently, there is no way to move a `State` among different threads; all
/// fields are `!Send` and `!Sync`, so the part of the emulator using fields
/// cannot be moved across threads as well.
#[allow(non_snake_case)]
pub fn CurrentState() -> &'static mut State {
    let s: *const State = STATE.with(|s| &(*s.borrow()) as _);
    let s: *mut State = s as *mut State;
    unsafe { &mut *s }
}

type Ser<'de, 'c> = rmp_serde::Serializer<
    &'de mut lz4::Encoder<&'c mut Vec<u8>>,
    rmp_serde::encode::StructMapWriter,
>;
type Deser<'de> = rmp_serde::Deserializer<rmp_serde::decode::ReadReader<lz4::Decoder<&'de [u8]>>>;

// FieldInfo contains the type-erased serialization and deserialization functions
// for each field.
struct FieldInfo {
    name: &'static str,
    serialize: Box<for<'de, 'c> Fn(&mut Ser<'de, 'c>) -> Result<(), rmp_serde::encode::Error>>,
    deserialize: Box<for<'de> FnMut(&mut Deser<'de>) -> Result<(), rmp_serde::decode::Error>>,
}

impl FieldInfo {
    fn new<F>(name: &'static str, field: &Field<F>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        let field1 = field.clone();
        let mut field2 = field.clone();
        Self {
            name,
            serialize: Box::new(move |ser| (*field1).serialize(ser)),
            deserialize: Box::new(move |deser| {
                *field2 = serde::Deserialize::deserialize(deser)?;
                Ok(())
            }),
        }
    }

    fn new_endian<F, O>(name: &'static str, field: &EndianField<F, O>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static> + MemInt,
        O: 'static + ByteOrderCombiner,
    {
        let field1 = field.clone();
        let mut field2 = field.clone();
        Self {
            name,
            serialize: Box::new(move |ser| field1.get().serialize(ser)),
            deserialize: Box::new(move |deser| {
                field2.set(serde::Deserialize::deserialize(deser)?);
                Ok(())
            }),
        }
    }

    fn new_array<F>(name: &'static str, field: &ArrayField<F>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        let field1 = field.clone();
        let mut field2 = field.clone();
        Self {
            name,
            serialize: Box::new(move |ser| (*field1).serialize(ser)),
            deserialize: Box::new(move |deser| {
                let buf: Vec<F> = serde::Deserialize::deserialize(deser)?;
                field2.copy_from_slice(&buf[..]);
                Ok(())
            }),
        }
    }
}

/// State holds a serializable state for the emulator, composed from multiple
/// fields.
///
/// An empty state is automatically created for each new thread, and can be
/// accessed with [`CurrentState()`](fn.CurrentState.html).
///
/// Cloning a `State` actually creates a copy of the whole state. Creating a new
/// empty `State` is forbidden (as it would be useless without Field definitions).
///
/// See module-level documentation for more details.
#[derive(Clone)]
pub struct State {
    data: Vec<u8>,
    info: Rc<RefCell<BTreeMap<String, FieldInfo>>>,
}

#[inline]
fn round_up(base: usize, align: usize) -> usize {
    (base + (align - 1)) & !(align - 1)
}

impl State {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(1024),
            info: Rc::new(RefCell::new(BTreeMap::default())),
        }
    }

    fn alloc_raw(&mut self, size: usize, align: usize) -> usize {
        let offset = round_up(self.data.len(), align);
        self.data.resize(offset + size, 0);
        offset
    }

    fn new_field<F>(&mut self, name: &'static str, value: F) -> Field<F>
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.borrow().contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let mut f = Field {
            offset: self.alloc_raw(mem::size_of::<F>(), mem::align_of::<F>()),
            phantom: PhantomData,
        };
        self.info
            .borrow_mut()
            .insert(name.to_owned(), FieldInfo::new(name, &f));
        *f = value;
        f
    }

    fn new_endian_field<F, O>(&mut self, name: &'static str, value: F) -> EndianField<F, O>
    where
        F: 'static + Copy + Serialize + Deserialize<'static> + MemInt,
        O: 'static + ByteOrderCombiner,
    {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.borrow().contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let size = mem::size_of::<F>();
        let offset = self.alloc_raw(size, mem::align_of::<F>());

        let mut f = EndianField {
            offset: offset,
            phantom: PhantomData,
        };
        self.info
            .borrow_mut()
            .insert(name.to_owned(), FieldInfo::new_endian(name, &f));
        f.set(value);
        f
    }

    fn new_array_field<F>(
        &mut self,
        name: &'static str,
        value: F,
        len: usize,
        serialize: bool,
    ) -> ArrayField<F>
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.borrow().contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let size = len * mem::size_of::<F>();
        let offset = self.alloc_raw(size, mem::align_of::<F>());

        let mut f = ArrayField {
            offset: offset,
            len: len,
            phantom: PhantomData,
        };
        if serialize {
            self.info
                .borrow_mut()
                .insert(name.to_owned(), FieldInfo::new_array(name, &f));
        }
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

    /// Serialize the state into a persistence format that can be written
    /// to disk and reloaded in different process. It relies on Serde-based
    /// serialization.
    pub fn serialize(
        &self,
        magic: &str,
        version: u32,
    ) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        use serde::Serializer;
        use std::io::Write;

        // Write the header
        let header = "EMUSTATE\x00";
        let mut output = Vec::new();
        (&mut output).write(header.as_bytes()).unwrap();

        let mut compress = lz4::EncoderBuilder::new()
            .level(9)
            .build(&mut output)
            .unwrap();
        let mut ser = rmp_serde::Serializer::new_named(&mut compress);

        // Serialize the whole state like a struct. Each `Field` is a field
        // of this struct, using its name as name of the struct field.
        ser.serialize_str(magic)?;
        ser.serialize_u32(version)?;
        ser.serialize_u32(self.info.borrow().len() as u32)?;
        for fi in self.info.borrow().values() {
            ser.serialize_str(fi.name)?;
            (*fi.serialize)(&mut ser)?;
        }

        let (_, res) = compress.finish();
        res.unwrap();
        Ok(output)
    }

    /// Deserialize into the current state.
    /// Notice that any field not present in the serialized state
    /// maintain their current value, and no error is returned. It is thus
    /// suggested to deserialize over a default initial state.
    pub fn deserialize(
        &mut self,
        wanted_magic: &'static str,
        wanted_version: u32,
        data: &[u8],
    ) -> Result<(), rmp_serde::decode::Error> {
        use rmp_serde::decode::Error;
        use std::io::Read;

        let mut reader = &data[..];
        let mut header = vec![0u8; 9];
        reader
            .read_exact(&mut header)
            .or_else(|_| Err(Error::Syntax(format!("invalid save state format"))))?;
        if header != "EMUSTATE\x00".as_bytes() {
            return Err(Error::Syntax(format!("invalid save state format")));
        }

        let dec = lz4::Decoder::new(reader)
            .or_else(|_| Err(Error::Syntax("invalid save state format".into())))?;

        let mut de = rmp_serde::Deserializer::new(dec);

        let magic: String = Deserialize::deserialize(&mut de)?;
        if magic != wanted_magic {
            return Err(Error::Syntax(format!("invalid magic: {}", magic)));
        }

        let version: u32 = Deserialize::deserialize(&mut de)?;
        if version != wanted_version {
            return Err(Error::Syntax(format!("unsupported version: {}", version)));
        }

        let num_fields: u32 = Deserialize::deserialize(&mut de)?;
        for _ in 0..num_fields {
            let fname: String = Deserialize::deserialize(&mut de)?;
            match self.info.borrow_mut().get_mut(&fname) {
                Some(fi) => (*fi.deserialize)(&mut de)?,
                None => {}
            };
        }

        Ok(())
    }
}

/// A compressed snapshot of a `State`, useful for in-process snapshotting.
/// To be made current, it must be decompressed back into a [`State`](struct.State.html) using
/// [`decompress()`](#method.decompress).
///
/// The [LZ4 algorithm](https://www.lz4.org) is used for the compression.
pub struct CompressedState {
    data: RefCell<Vec<u8>>,
    future_data: RefCell<Option<Oneshot<Vec<u8>>>>,
    info: Rc<RefCell<BTreeMap<String, FieldInfo>>>,
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
            info: state.info.clone(),
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
    use serde_derive::{Deserialize, Serialize};

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
        #[derive(Copy, Clone, Serialize, Deserialize)]
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
        let array = f.as_array_field();

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

    #[test]
    fn serialize() {
        use byteorder::BigEndian;

        let mut a = Field::new("a", 4u64);
        let mut b = Field::new("b", 12.0f64);
        let mut c = EndianField::<u32, BigEndian>::new("c", 99u32);
        let mut d = ArrayField::new("x", 7u8, 4, true);
        let mut e = ArrayField::new("y", 7u8, 4, false);

        let s1 = CurrentState().serialize("test", 1).unwrap();

        assert!(CurrentState().deserialize("xest", 1, &s1).is_err());
        assert!(CurrentState().deserialize("test", 2, &s1).is_err());

        *a = 5;
        *b = 13.0;
        c.set(1234);
        d[0] = 0;
        d[1] = 1;
        d[2] = 2;
        d[3] = 3;
        e[0] = 0;
        e[1] = 1;
        e[2] = 2;
        e[3] = 3;

        let res = CurrentState().deserialize("test", 1, &s1);
        assert!(res.is_ok());
        assert_eq!(*a, 4);
        assert_eq!(*b, 12.0);
        assert_eq!(c.get(), 99);
        assert_eq!(d[0], 7);
        assert_eq!(d[1], 7);
        assert_eq!(d[2], 7);
        assert_eq!(d[3], 7);
        assert_eq!(e[0], 0);
        assert_eq!(e[1], 1);
        assert_eq!(e[2], 2);
        assert_eq!(e[3], 3);
    }
}
