use crate::ty::ID;
use std::collections::HashSet;

#[derive(Default, Clone)]
pub struct Reachability {
    upsets: Vec<HashSet<ID>>,
    downsets: Vec<HashSet<ID>>,
}

impl Reachability {
    pub fn add_node(&mut self) -> ID {
        let i = self.upsets.len();

        let mut set = HashSet::with_capacity(1);
        set.insert(i);

        self.upsets.push(set.clone());
        self.downsets.push(set);
        i
    }

    pub fn add_edge(&mut self, lhs: ID, rhs: ID, out: &mut Vec<(ID, ID)>) {
        if self.downsets[lhs].contains(&rhs) {
            return;
        }

        let mut lhs_set: Vec<_> = self.upsets[lhs].iter().copied().collect();
        lhs_set.sort_unstable();

        let mut rhs_set: Vec<_> = self.downsets[rhs].iter().copied().collect();
        rhs_set.sort_unstable();

        for lhs2 in lhs_set {
            for &rhs2 in &rhs_set {
                if self.downsets[lhs2].insert(rhs2) {
                    self.upsets[rhs2].insert(lhs2);
                    out.push((lhs2, rhs2));
                }
            }
        }
    }
}
