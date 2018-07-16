extern crate num;
use self::num::PrimInt;
use super::super::fp::{FixedPoint, Q};
use std::fmt;
use std::ops;

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct Point<FP: FixedPoint> {
    pub x: Q<FP>,
    pub y: Q<FP>,
}

impl<FP: FixedPoint> Point<FP> {
    #[inline(always)]
    pub fn new(x: Q<FP>, y: Q<FP>) -> Self {
        Point { x, y }
    }
    #[inline(always)]
    pub fn from_int<N: PrimInt>(x: N, y: N) -> Self {
        Self::new(Q::from_int(x), Q::from_int(y))
    }
    #[inline(always)]
    pub fn from_bits(x: FP::BITS, y: FP::BITS) -> Self {
        Self::new(Q::from_bits(x), Q::from_bits(y))
    }
    #[inline(always)]
    pub fn cast<FP2: FixedPoint>(self) -> Point<FP2> {
        Point::new(self.x.cast(), self.y.cast())
    }
    #[inline(always)]
    pub fn truncate(self) -> Point<impl FixedPoint> {
        Point::new(self.x.truncate(), self.y.truncate())
    }
}

impl<FP: FixedPoint> ops::Add for Point<FP> {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl<FP: FixedPoint> ops::Sub for Point<FP> {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl<FP: FixedPoint> fmt::Debug for Point<FP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Point {{ x:{:?} , y:{:?} }}", self.x, self.y)
    }
}

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct Rect<FP: FixedPoint> {
    pub c0: Point<FP>,
    pub c1: Point<FP>,
}

impl<FP: FixedPoint> Rect<FP> {
    #[inline(always)]
    pub fn new(c0: Point<FP>, c1: Point<FP>) -> Self {
        Rect { c0, c1 }
    }

    #[inline(always)]
    pub fn from_bits(x0: FP::BITS, y0: FP::BITS, x1: FP::BITS, y1: FP::BITS) -> Self {
        Self::new(Point::from_bits(x0, y0), Point::from_bits(x1, y1))
    }

    #[inline(always)]
    pub fn width(self) -> Q<FP> {
        self.c1.x - self.c0.x
    }
    #[inline(always)]
    pub fn height(self) -> Q<FP> {
        self.c1.y - self.c0.y
    }
    #[inline(always)]
    pub fn cast<FP2: FixedPoint>(self) -> Rect<FP2> {
        Rect::new(self.c0.cast(), self.c1.cast())
    }
    #[inline(always)]
    pub fn truncate(self) -> Rect<impl FixedPoint> {
        Rect::new(self.c0.truncate(), self.c1.truncate())
    }
    #[inline(always)]
    pub fn set_width(&mut self, w: Q<FP>) {
        self.c1.x = self.c0.x + w;
    }
    #[inline(always)]
    pub fn set_height(&mut self, h: Q<FP>) {
        self.c1.y = self.c0.y + h;
    }
}

impl<FP: FixedPoint> fmt::Debug for Rect<FP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Rect {{ {:?},{:?} - {:?},{:?} }}",
            self.c0.x, self.c0.y, self.c1.x, self.c1.y
        )
    }
}
