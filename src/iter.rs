use git2::Commit;
use git2::Oid;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::HashSet;

pub struct HistoryIter<'repo> {
    seen: HashSet<Oid>,
    to_visit: BinaryHeap<Generation<'repo>>,
}

pub struct HistoryItem<'repo> {
    pub commit: Commit<'repo>,
    pub is_merge: bool,
}

struct Generation<'repo> {
    num: u32,
    nth: u32,
    commit: Commit<'repo>,
}

impl<'repo> HistoryIter<'repo> {
    pub fn new(start: Commit<'repo>) -> Self {
        let mut to_visit = BinaryHeap::with_capacity(16);
        to_visit.push(Generation{
            commit:start,
            num:0, nth:0,
        });
        Self {
            seen: HashSet::with_capacity(500),
            to_visit,
        }
    }
}

impl<'repo> Iterator for HistoryIter<'repo> {
    type Item = HistoryItem<'repo>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(Generation{commit, num, ..}) = self.to_visit.pop() {
            let seen = &mut self.seen; // technically needed only after merges
            let mut is_merge = false;
            self.to_visit.extend(commit.parents()
                .take(1)
                .filter(|commit| {
                    seen.insert(commit.id())
                })
                .enumerate()
                .map(|(nth, commit)| {
                    if nth > 0 {is_merge = true;}
                    Generation {num: num+1, nth: nth as u32, commit}
                }));
            Some(HistoryItem {
                commit, is_merge,
            })
        } else {
            None
        }
    }
}

impl<'repo> PartialEq for Generation<'repo> {
    fn eq(&self, other: &Generation<'_>) -> bool {
        other.num == self.num && self.nth == other.nth
    }
}
impl<'repo> PartialOrd for Generation<'repo> {
    fn partial_cmp(&self, other: &Generation<'_>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<'repo> Eq for Generation<'repo> {}
impl<'repo> Ord for Generation<'repo> {
    fn cmp(&self, other: &Generation<'_>) -> Ordering {
        other.num.cmp(&self.num).then(other.nth.cmp(&self.nth))
    }
}
