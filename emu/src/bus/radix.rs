use array_macro::array;

const RADIX_BITS: usize = 11;
const RADIX_BREADTH: usize = 1 << RADIX_BITS;
const RADIX_DEPTH: usize = (32 + RADIX_BITS - 1) / RADIX_BITS;
const RADIX_FIRST_SHIFT: usize = 32 - RADIX_BITS;
const RADIX_MASK: u32 = (1 << RADIX_BITS as u32) - 1;

enum Node<T: Clone> {
    Leaf(Option<T>),
    Internal(Box<RadixTree<T>>),
}

impl<T: Clone> Node<T> {
    fn clone(&self) -> Option<Node<T>> {
        match self {
            Node::Internal(_) => None,
            Node::Leaf(t) => Some(Node::Leaf(t.clone())),
        }
    }

    fn leaf(&mut self) -> Option<&mut Node<T>> {
        match self {
            Node::Internal(_) => None,
            Node::Leaf(_) => Some(self),
        }
    }

    fn internal(&mut self) -> Option<&mut RadixTree<T>> {
        match self {
            Node::Internal(ot) => Some(ot.as_mut()),
            Node::Leaf(_) => None,
        }
    }

    fn split(&mut self) -> &mut Node<T> {
        if self.leaf().is_some() {
            *self = Node::Internal(RadixTree::new_with_node(self.clone().unwrap()));
        }
        return self;
    }
}

pub struct RadixTree<T: Clone> {
    nodes: [Node<T>; RADIX_BREADTH],
}

impl<T: Clone> RadixTree<T> {
    pub fn new() -> Box<RadixTree<T>> {
        return box RadixTree {
            nodes: array![Node::Leaf(None); RADIX_BREADTH],
        };
    }

    fn new_with_node(n: Node<T>) -> Box<RadixTree<T>> {
        let mut t = RadixTree::new();
        for tn in t.nodes.iter_mut() {
            *tn = n.clone().unwrap();
        }
        return t;
    }

    fn iter_range<'a>(
        &'a mut self,
        beg: u32,
        end: u32,
        shift: usize,
    ) -> Box<'a + Iterator<Item = &mut Node<T>>> {
        let idx1 = (beg >> shift) as usize;
        let idx2 = (end >> shift) as usize;
        let mask = (1 << shift) - 1;
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

        let mut iter: Box<Iterator<Item = &mut Node<T>>> = box mid.iter_mut();

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

    pub fn insert_range<'s, 'r>(
        &'s mut self,
        begin: u32,
        end: u32,
        val: T,
        force: bool,
    ) -> Result<(), &'r str>
    where
        'r: 's,
    {
        for n in self.iter_range(begin, end, RADIX_FIRST_SHIFT) {
            *n = match n {
                Node::Internal(_) => unreachable!(),
                Node::Leaf(ot) => {
                    if !force && ot.is_some() {
                        return Err("insert_range over non-empty range");
                    }
                    Node::Leaf(Some(val.clone()))
                }
            };
        }
        Ok(())
    }

    pub fn lookup(&self, mut key: u32) -> Option<&T> {
        let mut nodes = &self.nodes;
        let mut shift = RADIX_FIRST_SHIFT;
        for i in 0..RADIX_DEPTH {
            let idx: usize = ((key >> shift) & RADIX_MASK) as usize;
            match nodes[idx] {
                Node::Internal(ref n) => nodes = &n.nodes,
                Node::Leaf(ref t) => return t.as_ref(),
            }
            key &= ((1 << shift) - 1) as u32;
            if i == RADIX_DEPTH - 2 {
                shift = 0;
            } else {
                shift -= RADIX_BITS;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(t: &RadixTree<u8>, key: u32) -> u8 {
        println!("lookup: {:x}", key);
        *t.lookup(key).or_else(|| Some(&0)).unwrap()
    }

    #[test]
    fn big_spans() {
        let mut t = RadixTree::<u8>::new();
        assert_eq!(
            t.insert_range(0x04000000, 0x04ffffff, 1, false).is_err(),
            false
        );
        assert_eq!(
            t.insert_range(0x05000000, 0x05ffffff, 2, false).is_err(),
            false
        );
        assert_eq!(
            t.insert_range(0x06000000, 0x09ffffff, 3, false).is_err(),
            false
        );
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
        assert_eq!(
            t.insert_range(0x040000F0, 0x07ffffef, 1, false).is_err(),
            false
        );
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
        assert_eq!(
            t.insert_range(0x04000501, 0x04000505, 1, false).is_err(),
            false
        );
        assert_eq!(lookup(&t, 0x04000500), 0);
        assert_eq!(lookup(&t, 0x04000501), 1);
        assert_eq!(lookup(&t, 0x04000502), 1);
        assert_eq!(lookup(&t, 0x04000504), 1);
        assert_eq!(lookup(&t, 0x04000505), 1);
        assert_eq!(lookup(&t, 0x04000506), 0);
        assert_eq!(
            t.insert_range(0x04000501, 0x04000505, 2, false).is_err(),
            true
        );
        assert_eq!(
            t.insert_range(0x04000500, 0x04000502, 2, false).is_err(),
            true
        );
    }
}
