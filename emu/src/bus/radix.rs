const RADIX_BITS: usize = 8;
const RADIX_FIRST_SHIFT: usize = 32 - RADIX_BITS;
const RADIX_DEPTH: usize = 1 << RADIX_BITS;
const RADIX_MASK: u32 = (1 << RADIX_BITS as u32) - 1;

enum Node<'a, T: 'a> {
    Leaf(Option<&'a T>),
    Internal(Box<RadixTree<'a, T>>),
}

impl<'a, 'b, T> Node<'a, T>
where
    'a: 'b,
{
    fn clone(&'b self) -> Option<Node<'a, T>> {
        match self {
            Node::Internal(ot) => None,
            Node::Leaf(t) => Some(Node::Leaf(*t)),
        }
    }

    fn leaf(&'b mut self) -> Option<&'b mut Node<'a, T>> {
        match self {
            Node::Internal(_) => None,
            Node::Leaf(_) => Some(self),
        }
    }

    fn internal(&'b mut self) -> Option<&'b mut RadixTree<'a, T>> {
        match self {
            Node::Internal(ot) => Some(ot.as_mut()),
            Node::Leaf(_) => None,
        }
    }

    fn split(&'b mut self) -> &'b mut Node<'a, T> {
        if self.leaf().is_some() {
            *self = Node::Internal(RadixTree::new_with_node(self.clone().unwrap()));
        }
        return self;
    }
}

pub struct RadixTree<'a, T: 'a> {
    nodes: [Node<'a, T>; RADIX_DEPTH],
}

impl<'a, 'b, T> RadixTree<'a, T>
where
    'a: 'b,
{
    pub fn new() -> Box<RadixTree<'a, T>> {
        return box RadixTree {
            nodes: array![Node::Leaf(None); RADIX_DEPTH],
        };
    }

    fn new_with_node(n: Node<'a, T>) -> Box<RadixTree<'a, T>> {
        let mut t = RadixTree::new();
        for tn in t.nodes.iter_mut() {
            *tn = n.clone().unwrap();
        }
        return t;
    }

    fn iter_range(
        &'b mut self,
        beg: u32,
        end: u32,
        shift: usize,
    ) -> Box<'b + Iterator<Item = &'b mut Node<'a, T>>> {
        let idx1 = (beg >> shift) as usize;
        let idx2 = (end >> shift) as usize;
        let mask = ((1 << shift) - 1);
        let beg = (beg as usize) & mask;
        let end = (end as usize) & mask;

        // We're on the bottom level, we can't recurse anymore.
        if shift == 0 {
            return box self.nodes[idx1..=idx2].iter_mut();
        }

        // See if we're spanning full nodes, in which case we don't need to recurse
        if beg == 0 && end == mask {
            return box self.nodes[idx1..=idx2].iter_mut();
        }

        let nshift = shift.saturating_sub(RADIX_BITS);

        // Partial single node: recurse
        if idx1 == idx2 {
            return self.nodes[idx1]
                .split()
                .internal()
                .unwrap()
                .iter_range(beg as u32, end as u32, nshift);
        }

        // Partial multiple nodes: iterate first inner nodes, then handle
        // first and last
        let (first, mid) = self.nodes[idx1..=idx2].split_at_mut(1);
        let (mid, last) = mid.split_at_mut(idx2 - idx1 - 1);

        let mut iter: Box<'b + Iterator<Item = &'b mut Node<'a, T>>> = box mid.iter_mut();

        if beg == 0 {
            iter = box iter.chain(box first.iter_mut());
        } else {
            iter = box iter.chain(first[0].split().internal().unwrap().iter_range(
                beg as u32,
                mask as u32,
                nshift,
            ));
        }

        if end == mask {
            iter = box iter.chain(box last.iter_mut());
        } else {
            iter = box iter.chain(
                last[0]
                    .split()
                    .internal()
                    .unwrap()
                    .iter_range(0 as u32, end as u32, nshift),
            );
        }

        return iter;
    }

    pub fn insert_range(&'b mut self, begin: u32, end: u32, val: &'a T) -> Result<(), &str> {
        for n in self.iter_range(begin, end, RADIX_FIRST_SHIFT) {
            *n = match n {
                Node::Internal(_) => unreachable!(),
                Node::Leaf(ot) => match ot {
                    Some(_) => return Err("insert_range over non-empty range"),
                    None => Node::Leaf(Some(val)),
                },
            };
        }
        Ok(())
    }

    pub fn lookup(&'b self, mut key: u32) -> Option<&'a T> {
        let mut nodes = &self.nodes;
        let mut shift = RADIX_FIRST_SHIFT;
        loop {
            let idx: usize = ((key >> shift) & RADIX_MASK) as usize;
            match nodes[idx] {
                Node::Internal(ref n) => nodes = &n.nodes,
                Node::Leaf(t) => return t,
            }
            if shift == 0 {
                return None;
            }
            key &= (1 << shift) - 1;
            shift = shift.saturating_sub(RADIX_BITS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    fn lookup(t: &RadixTree<u8>, key: u32) -> u8 {
        println!("lookup: {:x}", key);
        *t.lookup(key).or_else(|| Some(&0)).unwrap()
    }

    #[test]
    fn big_spans() {
        let mut t = RadixTree::<u8>::new();
        assert_eq!(t.insert_range(0x04000000, 0x04ffffff, &1).is_err(), false);
        assert_eq!(t.insert_range(0x05000000, 0x05ffffff, &2).is_err(), false);
        assert_eq!(t.insert_range(0x06000000, 0x09ffffff, &3).is_err(), false);
        assert_eq!(lookup(&t, 0x04000000), 1);
        assert_eq!(lookup(&t, 0x04111111), 1);
        assert_eq!(lookup(&t, 0x05111111), 2);
        assert_eq!(lookup(&t, 0x08111111), 3);
        assert_eq!(lookup(&t, 0x09ffffff), 3);
        assert_eq!(lookup(&t, 0x0a000000), 0);
    }

    #[test]
    fn large_uneven() {
        let mut t = RadixTree::<u8>::new();
        assert_eq!(t.insert_range(0x040000F0, 0x07ffffef, &1).is_err(), false);
        assert_eq!(lookup(&t, 0x04000000), 0);
        assert_eq!(lookup(&t, 0x040000ef), 0);
        assert_eq!(lookup(&t, 0x040000f0), 1);
        assert_eq!(lookup(&t, 0x040000f1), 1);
        assert_eq!(lookup(&t, 0x04111111), 1);
        assert_eq!(lookup(&t, 0x05111111), 1);
        assert_eq!(lookup(&t, 0x06111111), 1);
        assert_eq!(lookup(&t, 0x07111111), 1);
        assert_eq!(lookup(&t, 0x07ffffef), 1);
        assert_eq!(lookup(&t, 0x07fffff0), 0);
        assert_eq!(lookup(&t, 0x08000000), 0);
    }

    #[test]
    fn insert_deep() {
        let mut t = RadixTree::<u8>::new();
        assert_eq!(t.insert_range(0x04000501, 0x04000505, &1).is_err(), false);
        assert_eq!(lookup(&t, 0x04000500), 0);
        assert_eq!(lookup(&t, 0x04000501), 1);
        assert_eq!(lookup(&t, 0x04000502), 1);
        assert_eq!(lookup(&t, 0x04000504), 1);
        assert_eq!(lookup(&t, 0x04000505), 1);
        assert_eq!(lookup(&t, 0x04000506), 0);
        assert_eq!(t.insert_range(0x04000501, 0x04000505, &2).is_err(), true);
        assert_eq!(t.insert_range(0x04000500, 0x04000502, &2).is_err(), true);
    }
}
