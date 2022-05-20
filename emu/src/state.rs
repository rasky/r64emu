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
use serde_bytes;

use failure::{Error, Fail};
use std::cell::Cell;
use std::cell::{RefCell, RefMut};
use std::collections::BTreeMap;
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

#[derive(Debug, Fail)]
enum SerializationFailure {
    #[fail(display = "invalid state serialization format")]
    InvalidFormat,

    #[fail(display = "invalid magic string: {}", magic)]
    InvalidMagic { magic: String },
    #[fail(display = "invalid version: {}", version)]
    InvalidVersion { version: u32 },
}

// Global per-thread state. Notice that we use #[thread_local] rather than
// thread_local!() as it's much faster at accessing the state, and also allows
// non-scoped access (that is, without `with`).
#[thread_local]
static mut STATE: Option<RefCell<State>> = None;

// ID used to cache pointers within the global state. Any time the global State
// changes in a way that makes all previous pointer invalid, this counter is
// incremented.
static STATE_ID: AtomicU32 = AtomicU32::new(0);

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
pub fn CurrentState() -> RefMut<'static, State> {
    unsafe {
        if STATE.is_none() {
            STATE = Some(RefCell::new(State::new()));
        }
        STATE.as_ref().unwrap().borrow_mut()
    }
}

// Like CurrentState, but does not enforce exclusive mutable access to State.
// This is useful within fields' implementation, as each field accesses a distinct
// subslice of the slice buffer, so there's no aliasing issue.
#[allow(non_snake_case)]
#[inline(always)]
unsafe fn UnsafeCurrentState() -> &'static mut State {
    // NOTE: we don't use unwrap() here because it doesn't always get inlined.
    if let Some(S) = STATE.as_ref() {
        return &mut *S.as_ptr();
    }
    panic!("UnsafeCurrentState called before State initialization");
}

/// A `Field` is an object that is part of the emulator state. It is a lightweight
/// pointer into the current state, and can be used to mutate the state itself.
///
/// The type of the object pointed by `Field` (`F`) must implement the Copy
/// trait (because its contents will be copied around when snapshotting the state),
/// and the Serialize trait (to allow for long term persistence).
///
/// Cloning a `Field` creates another field pointing to the same content (aliasing).
/// For this reason, cloning is marked as unsafe.
pub struct Field<F: Copy + Serialize + Deserialize<'static>> {
    offset: usize,
    cache: Cell<(u32, *mut F)>,
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
    pub fn new(name: &str, f: F) -> Self {
        let mut field = CurrentState().new_field(name);
        *field = f;
        field
    }

    /// Create an aliased copy to this field, that is a field that points to the
    /// same underlying data. It is unsafe because aliasing can allow multiple
    /// mutable borrows, and thus it cannot be used to impl Clone.
    pub unsafe fn clone(&self) -> Self {
        Self {
            offset: self.offset,
            cache: Cell::new(self.cache.get()),
        }
    }

    fn as_ref_with_state<'s, 'f: 's>(&'s self, state: &'f State) -> &'f F {
        unsafe {
            let data = &state.data[self.offset];
            mem::transmute(data)
        }
    }

    fn as_mut_with_state<'s, 'f: 's>(&'s mut self, state: &'f mut State) -> &'f mut F {
        unsafe {
            let data = &mut state.data[self.offset];
            mem::transmute(data)
        }
    }

    /// `as–ref()` returns a reference to access the underlying data. It is
    /// similar to the Deref trait, but it defines lifetimes to keep the `State`
    /// borrowed rather than the field instance. Since this violates aliasing
    /// rules, it is unsafe.
    pub unsafe fn as_ref<'s, 'f: 's>(&'s self) -> &'f F {
        let sid = STATE_ID.load(Ordering::Relaxed);
        let cache = self.cache.get();
        if sid == cache.0 {
            return &*cache.1;
        }
        let f = self.as_ref_with_state(UnsafeCurrentState());
        self.cache.set((sid, f as *const F as *mut F));
        f
    }
    /// `as_mut()` is the mutable version of [`as_ref()`](struct.Field.html#method.as_ref).
    pub unsafe fn as_mut<'s, 'f: 's>(&'s mut self) -> &'f mut F {
        let sid = STATE_ID.load(Ordering::Relaxed);
        let cache = self.cache.get();
        if sid == cache.0 {
            return &mut *cache.1;
        }
        let f = self.as_mut_with_state(UnsafeCurrentState());
        self.cache.set((sid, f as *mut F));
        f
    }
}

impl<F: Copy + Serialize + Deserialize<'static>> Default for Field<F> {
    // Default returns an invalid Field, that will cause a panic when used.
    // It can be used as placeholder in structs until proper initialization
    // is performed.
    fn default() -> Self {
        Self {
            offset: usize::max_value(),
            cache: Cell::new((u32::max_value(), ptr::null_mut())),
        }
    }
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> Deref for Field<F> {
    type Target = F;

    fn deref(&self) -> &F {
        // Keeping self borrowed makes as_ref() safe
        unsafe { self.as_ref() }
    }
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> DerefMut for Field<F> {
    fn deref_mut(&mut self) -> &mut F {
        // Keeping self borrowed makes as_mut() safe
        unsafe { self.as_mut() }
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
/// Cloning a `EndianField` creates another `EndianField` pointing to the same
/// content (aliasing). For this reason, cloning is marked as unsafe.
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
    pub fn new(name: &str, f: F) -> Self {
        let mut field = CurrentState().new_endian_field(name);
        field.set(f);
        field
    }
    pub fn as_array_field(&self) -> ArrayField<u8> {
        ArrayField {
            offset: self.offset,
            len: mem::size_of::<F>(),
            phantom: PhantomData,
        }
    }

    /// Create an aliased copy to this field, that is a field that points to the
    /// same underlying data. It is unsafe because aliasing can allow multiple
    /// mutable borrows, and thus it cannot be used to impl Clone.
    pub unsafe fn clone(&self) -> Self {
        Self {
            offset: self.offset,
            phantom: PhantomData,
        }
    }

    fn as_ref(&self, state: &State) -> &F {
        unsafe {
            let data = &state.data[self.offset];
            mem::transmute(data)
        }
    }

    fn as_mut(&mut self, state: &mut State) -> &mut F {
        unsafe {
            let data = &mut state.data[self.offset];
            mem::transmute(data)
        }
    }

    fn get_with_state(&self, state: &State) -> F {
        O::to_native(*self.as_ref(state))
    }
    fn set_with_state(&mut self, state: &mut State, val: F) {
        *self.as_mut(state) = O::to_native(val);
    }

    /// Get the current value for this field. Getters and setters are explicit
    /// methods in `EndianField` because they must perform endianess adjustments.
    pub fn get(&self) -> F {
        unsafe { self.get_with_state(UnsafeCurrentState()) }
    }

    /// Set the current value for this field. Getters and setters are explicit
    /// methods in `EndianField` because they must perform endianess adjustments.
    pub fn set(&mut self, val: F) {
        unsafe {
            self.set_with_state(UnsafeCurrentState(), val);
        }
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
///
/// Cloning a `ArrayField` creates another `ArrayField` pointing to the same
/// content (aliasing). For this reason, cloning is marked as unsafe.
pub struct ArrayField<F: Copy + Serialize + Deserialize<'static>> {
    offset: usize,
    len: usize,
    phantom: PhantomData<F>,
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> ArrayField<F> {
    /// Create an `ArrayField` with the specified name, initial value, and length.
    pub fn new(name: &str, f: F, len: usize) -> Self {
        Self::internal_new(name, f, len, true)
    }

    // FIXME: this is hack for Bus; instead of having a "non-serializable ArrayState",
    // we should revisit Bus::HwIo in a way that it doesn't require ArrayField.
    pub(crate) fn internal_new(name: &str, f: F, len: usize, serialize: bool) -> Self {
        let mut field = CurrentState().new_array_field(name, len, serialize);
        for v in field.iter_mut() {
            *v = f;
        }
        field
    }

    /// Return the number of elements in the array
    pub fn len(&self) -> usize {
        self.len
    }

    /// Create an aliased copy to this field, that is a field that points to the
    /// same underlying data. It is unsafe because aliasing can allow multiple
    /// mutable borrows, and thus it cannot be used to impl Clone.
    pub unsafe fn clone(&self) -> Self {
        Self {
            offset: self.offset,
            len: self.len,
            phantom: PhantomData,
        }
    }

    fn as_ref_with_state(&self, state: &State) -> &[F] {
        let data = &state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }

    fn as_mut_with_state(&mut self, state: &mut State) -> &mut [F] {
        let data = &mut state.data[self.offset..self.offset + self.len * mem::size_of::<F>()];
        unsafe { mem::transmute(data) }
    }

    /// `as_slice()` returns a slice to access the underlying array. It is
    /// similar to the Deref trait, but it defines lifetimes to keep the State
    /// borrowed rather than the field instance. Since this violates aliasing
    /// rules, it is unsafe.
    pub(crate) unsafe fn as_slice<'s, 'r: 's>(&'s self) -> &'r [F] {
        let state = UnsafeCurrentState();
        // We use the unchecked slice access as we can be sure that the offset
        // is within the state's bounds.
        let data = state
            .data
            .get_unchecked(self.offset..self.offset + self.len * mem::size_of::<F>());
        mem::transmute(data)
    }

    /// `as_slice_mut()` is the mutable version of
    /// [`as_slice()`](struct.ArrayState.html#method.as_slice).
    pub(crate) unsafe fn as_slice_mut<'s, 'r: 's>(&'s mut self) -> &'r mut [F] {
        let state = UnsafeCurrentState();
        // We use the unchecked slice access as we can be sure that the offset
        // is within the state's bounds.
        let data = state
            .data
            .get_unchecked_mut(self.offset..self.offset + self.len * mem::size_of::<F>());
        mem::transmute(data)
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
        // Since the lifetimes are correct in this function (that is, we keep
        // the field instance borrowed, not the state), this function is safe.
        unsafe { self.as_slice() }
    }
}

impl<F: 'static + Copy + Serialize + Deserialize<'static>> DerefMut for ArrayField<F> {
    fn deref_mut(&mut self) -> &mut [F] {
        // Since the lifetimes are correct in this function (that is, we keep
        // the field instance borrowed, not the state), this function is safe.
        unsafe { self.as_slice_mut() }
    }
}

type Ser<'de> = rmp_serde::Serializer<&'de mut Vec<u8>, rmp_serde::encode::StructMapWriter>;
type Deser<'de> = rmp_serde::Deserializer<rmp_serde::decode::ReadReader<&'de [u8]>>;

// FieldInfo contains the type-erased serialization and deserialization functions
// for each field.
struct FieldInfo {
    name: String,
    serialize: Box<for<'de> Fn(&mut Ser<'de>, &State) -> Result<(), rmp_serde::encode::Error>>,
    deserialize: Box<
        dyn for<'de> FnMut(&mut Deser<'de>, &mut State) -> Result<(), rmp_serde::decode::Error>,
    >,
}

impl FieldInfo {
    fn new<F>(name: &str, field: &Field<F>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        let field1 = unsafe { field.clone() };
        let mut field2 = unsafe { field.clone() };
        Self {
            name: name.to_owned(),
            serialize: Box::new(move |ser, state| field1.as_ref_with_state(state).serialize(ser)),
            deserialize: Box::new(move |deser, state| {
                *field2.as_mut_with_state(state) = serde::Deserialize::deserialize(deser)?;
                Ok(())
            }),
        }
    }

    fn new_endian<F, O>(name: &str, field: &EndianField<F, O>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static> + MemInt,
        O: 'static + ByteOrderCombiner,
    {
        let field1 = unsafe { field.clone() };
        let mut field2 = unsafe { field.clone() };
        Self {
            name: name.to_owned(),
            serialize: Box::new(move |ser, state| field1.get_with_state(state).serialize(ser)),
            deserialize: Box::new(move |deser, state| {
                field2.set_with_state(state, serde::Deserialize::deserialize(deser)?);
                Ok(())
            }),
        }
    }

    fn new_array<F>(name: &str, field: &ArrayField<F>) -> Self
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        // Use trait specialization to special-case ArrayField<u8> using
        // serde_bytes. This speeds us serialization of large memory buffers
        // a lot, because they're handled through a fast-path that just copies
        // the whole buffer, rather than iterating element by element
        // like for a slice of any other type.
        trait BufferSerializer: Serialize {
            fn buffer_serialize<'de>(
                &self,
                ser: &mut Ser<'de>,
            ) -> Result<(), rmp_serde::encode::Error>;
        }

        // Generic version
        impl<F: Serialize> BufferSerializer for F {
            default fn buffer_serialize<'de>(
                &self,
                ser: &mut Ser<'de>,
            ) -> Result<(), rmp_serde::encode::Error> {
                self.serialize(ser)
            }
        }

        // Specialization for [u8]
        impl BufferSerializer for &[u8] {
            fn buffer_serialize<'de>(
                &self,
                ser: &mut Ser<'de>,
            ) -> Result<(), rmp_serde::encode::Error> {
                serde_bytes::Bytes::new(self).serialize(ser)
            }
        }

        trait BufferDeserializer {
            fn buffer_deserialize<'de>(
                self,
                deser: &mut Deser<'de>,
            ) -> Result<(), rmp_serde::decode::Error>;
        }

        // Generic version
        impl<'r, F: 'static + Copy> BufferDeserializer for &'r mut [F]
        where
            F: serde::de::Deserialize<'static>,
        {
            default fn buffer_deserialize<'de>(
                self,
                deser: &mut Deser<'de>,
            ) -> Result<(), rmp_serde::decode::Error> {
                let buf: Vec<F> = serde::Deserialize::deserialize(deser)?;
                (*self).copy_from_slice(&buf[..]);
                Ok(())
            }
        }

        // Specialization for [u8]
        impl<'r> BufferDeserializer for &mut [u8] {
            fn buffer_deserialize<'de>(
                self,
                deser: &mut Deser<'de>,
            ) -> Result<(), rmp_serde::decode::Error> {
                let buf: Vec<u8> = serde_bytes::deserialize(deser)?;
                (*self).copy_from_slice(&buf[..]);
                Ok(())
            }
        }

        let field1 = unsafe { field.clone() };
        let mut field2 = unsafe { field.clone() };
        Self {
            name: name.to_owned(),
            serialize: Box::new(move |ser, state| {
                field1.as_ref_with_state(state).buffer_serialize(ser)
            }),
            deserialize: Box::new(move |deser, state| {
                field2.as_mut_with_state(state).buffer_deserialize(deser)
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
        let newsize = offset + size;
        if newsize > self.data.capacity() {
            STATE_ID.fetch_add(1, Ordering::Relaxed);
        }
        self.data.resize(newsize, 0);
        offset
    }

    fn new_field<F>(&mut self, name: &str) -> Field<F>
    where
        F: 'static + Copy + Serialize + Deserialize<'static>,
    {
        if name == "" {
            panic!("empty name for state field");
        }
        if self.info.borrow().contains_key(name) {
            panic!("duplicated field in state: {}", name);
        }

        let f = Field {
            offset: self.alloc_raw(mem::size_of::<F>(), mem::align_of::<F>()),
            cache: Cell::new((u32::max_value(), ptr::null_mut())),
        };
        self.info
            .borrow_mut()
            .insert(name.to_owned(), FieldInfo::new(name, &f));
        f
    }

    fn new_endian_field<F, O>(&mut self, name: &str) -> EndianField<F, O>
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

        let f = EndianField {
            offset: offset,
            phantom: PhantomData,
        };
        self.info
            .borrow_mut()
            .insert(name.to_owned(), FieldInfo::new_endian(name, &f));
        f
    }

    fn new_array_field<F>(&mut self, name: &str, len: usize, serialize: bool) -> ArrayField<F>
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

        let f = ArrayField {
            offset: offset,
            len: len,
            phantom: PhantomData,
        };
        if serialize {
            self.info
                .borrow_mut()
                .insert(name.to_owned(), FieldInfo::new_array(name, &f));
        }
        f
    }

    /// The size of the state, in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Make the current state as current, moving it.
    /// Returns the previously-current state.
    pub fn make_current(mut self) -> State {
        std::mem::swap(&mut *CurrentState(), &mut self);
        STATE_ID.fetch_add(1, Ordering::Relaxed);
        self
    }

    /// Convert the state into a `CompressedState`, consuming it. Notice
    /// that the compression is performed in a background thread.
    pub fn into_compressed(self) -> CompressedState {
        CompressedState::new(self)
    }

    /// Serialize the state into a persistence format that can be written
    /// to disk and reloaded in different process. It relies on Serde-based
    /// serialization.
    pub fn serialize<W: io::Write>(
        &self,
        mut writer: W,
        magic: &str,
        version: u32,
    ) -> Result<(), Error> {
        use serde::Serializer;

        // Write the header
        let header = "EMUSTATE\x00";
        writer.write(header.as_bytes())?;

        // Serialize the whole state like a struct. Each `Field` is a field
        // of this struct, using its name as name of the struct field.
        let mut output = Vec::new();
        let mut ser = rmp_serde::Serializer::new_named(&mut output);
        ser.serialize_str(magic)?;
        ser.serialize_u32(version)?;
        ser.serialize_u32(self.info.borrow().len() as u32)?;
        for fi in self.info.borrow().values() {
            ser.serialize_str(&fi.name)?;
            (*fi.serialize)(&mut ser, &self)?;
        }

        // Compress the output
        use lz4::block::CompressionMode::*;
        let data = lz4::block::compress(&output, Some(HIGHCOMPRESSION(9)), true)?;

        writer.write(&data)?;

        Ok(())
    }

    /// Deserialize into the current state.
    /// Notice that any field not present in the serialized state
    /// maintain their current value, and no error is returned. It is thus
    /// suggested to deserialize over a default initial state.
    pub fn deserialize<R: io::Read>(
        &mut self,
        mut reader: R,
        wanted_magic: &str,
        wanted_version: u32,
    ) -> Result<(), Error> {
        let mut header = vec![0u8; 9];
        reader.read_exact(&mut header)?;
        if header != "EMUSTATE\x00".as_bytes() {
            return Err(SerializationFailure::InvalidFormat.into());
        }

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let dec = lz4::block::decompress(&buf, None)?;

        let mut de = rmp_serde::Deserializer::new(&dec[..]);

        let magic: String = Deserialize::deserialize(&mut de)?;
        if magic != wanted_magic {
            return Err(SerializationFailure::InvalidMagic { magic }.into());
        }

        let version: u32 = Deserialize::deserialize(&mut de)?;
        if version != wanted_version {
            return Err(SerializationFailure::InvalidVersion { version }.into());
        }

        let num_fields: u32 = Deserialize::deserialize(&mut de)?;
        let info = self.info.clone(); // avoid borrowing self
        for _ in 0..num_fields {
            let fname: String = Deserialize::deserialize(&mut de)?;
            match info.borrow_mut().get_mut(&fname) {
                Some(fi) => {
                    (*fi.deserialize)(&mut de, self)?;
                }
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
    fn array_field() {
        let mut f = ArrayField::<u8>::new("a", 5, 16);

        assert_eq!(f[0], 5);
        assert_eq!(f[15], 5);

        f[4] = 8;
        f[5] = 6;

        assert_eq!(f[4], 8);
        assert_eq!(f[5], 6);
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
        let mut d = ArrayField::internal_new("x", 7u8, 4, true);
        let mut e = ArrayField::internal_new("y", 7u8, 4, false);

        let mut s1 = Vec::new();
        CurrentState().serialize(&mut s1, "test", 1).unwrap();

        assert!(CurrentState().deserialize(&s1[..], "xest", 1).is_err());
        assert!(CurrentState().deserialize(&s1[..], "test", 2).is_err());

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

        let res = CurrentState().deserialize(&s1[..], "test", 1);
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

    #[test]
    fn serialize_non_current() {
        use byteorder::BigEndian;

        let mut a = Field::new("a", 4u64);
        let mut b = Field::new("b", 12.0f64);
        let mut c = EndianField::<u32, BigEndian>::new("c", 99u32);
        let mut d = ArrayField::internal_new("x", 7u8, 4, true);
        let mut e = ArrayField::internal_new("y", 7u8, 4, false);

        let s1 = CurrentState().clone();

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

        let mut bin = Vec::new();
        s1.serialize(&mut bin, "test", 1).unwrap();

        let mut s2 = CurrentState().clone();
        s2.deserialize(&bin[..], "test", 1).unwrap();

        assert_eq!(*a, 5);
        assert_eq!(*b, 13.0);
        assert_eq!(c.get(), 1234);
        assert_eq!(d[0], 0);
        assert_eq!(d[1], 1);
        assert_eq!(d[2], 2);
        assert_eq!(d[3], 3);
        assert_eq!(e[0], 0);
        assert_eq!(e[1], 1);
        assert_eq!(e[2], 2);
        assert_eq!(e[3], 3);

        s2.make_current();

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

    #[test]
    #[should_panic]
    fn double_state_borrow() {
        // Check that we CANNOT get two mutable borrows to the current state.
        let s1 = CurrentState();
        let s2 = CurrentState(); // This will panic: double mutable borrow
        let s3 = s1.clone();
        let s4 = s2.clone();
        s3.make_current();
        s4.make_current();
    }

    #[test]
    fn double_field_borrow() {
        // Check that we can get two mutable borrows to two fields within the
        // current state. This is sound because the fields refer to distinct
        // parts of the same state (concept similar to split_at_mut).
        let mut a = ArrayField::new("a", 0u64, 8);
        let mut b = ArrayField::new("b", 0u64, 8);

        let ra = &mut a[0..7];
        let rb = &mut b[0..7];
        ra[0] = 5;
        rb[3] = 6;

        assert_eq!(ra[0], 5);
        assert_eq!(rb[3], 6);
    }
}
