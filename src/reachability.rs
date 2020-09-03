use crate::ty::ID;
use std::{collections::HashSet, hash::Hash};

#[derive(Default, Clone)]
struct OrderedSet<T> {
    v: Vec<T>,
    s: HashSet<T>,
}

impl<T> OrderedSet<T>
where
    T: Eq + Hash + Clone,
{
    fn insert(&mut self, value: T) -> bool {
        if self.s.insert(value.clone()) {
            self.v.push(value);
            true
        } else {
            false
        }
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.v.iter()
    }
}

#[derive(Default, Clone)]
pub struct Reachability {
    upsets: Vec<OrderedSet<ID>>,
    downsets: Vec<OrderedSet<ID>>,
}

impl Reachability {
    pub fn add_node(&mut self) -> ID {
        let i = self.upsets.len();
        self.upsets.push(Default::default());
        self.downsets.push(Default::default());
        i
    }

    pub fn add_edge(&mut self, lhs: ID, rhs: ID, out: &mut Vec<(ID, ID)>) {
        let mut work = vec![(lhs, rhs)];
        while let Some((lhs, rhs)) = work.pop() {
            if !self.downsets[lhs].insert(rhs) {
                continue;
            }

            self.upsets[lhs].insert(lhs);
            out.push((lhs, rhs));

            for lhs2 in self.upsets[lhs].iter().copied() {
                work.push((lhs2, rhs));
            }
            for rhs2 in self.downsets[rhs].iter().copied() {
                work.push((lhs, rhs2));
            }
        }
    }
}
