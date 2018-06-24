use super::bus::Bus;
use super::memint::ByteOrderCombiner;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

pub trait Device {
    type Order: ByteOrderCombiner;

    fn dev_init(&mut self, wself: Rc<RefCell<Self>>);
    fn dev_map(
        &self,
        bus: &mut Bus<Self::Order>,
        bank: usize,
        base: u32,
    ) -> Result<(), &'static str>;
}

#[derive(Clone)]
pub struct DevPtr<T: Device> {
    dev: Rc<RefCell<T>>,
}

impl<'b, T> DevPtr<T>
where
    T: Device,
{
    pub fn new(d: T) -> DevPtr<T> {
        let d = DevPtr {
            dev: Rc::new(RefCell::new(d)),
        };

        d.dev.borrow_mut().dev_init(d.dev.clone());
        return d;
    }

    pub fn borrow(&self) -> Ref<T> {
        self.dev.borrow()
    }

    pub fn borrow_mut(&mut self) -> RefMut<T> {
        self.dev.borrow_mut()
    }
}
