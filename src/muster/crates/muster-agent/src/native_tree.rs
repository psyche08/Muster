//! Bounded identity-tree traversal. Pruning one branch never stops siblings.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub fn walk_pruned<N: Clone + PartialEq>(
    root: N,
    limit: usize,
    budget: Duration,
    mut children: impl FnMut(&N) -> Option<Vec<N>>,
    mut descend: impl FnMut(&N) -> bool,
) -> (usize, &'static str, u128) {
    let started = Instant::now();
    let mut pending = VecDeque::from([root]);
    let mut seen = Vec::new();
    while let Some(node) = pending.pop_front() {
        if seen.contains(&node) {
            continue;
        }
        if seen.len() >= limit {
            return (seen.len(), "node_limit", started.elapsed().as_millis());
        }
        if started.elapsed() >= budget {
            return (seen.len(), "time_limit", started.elapsed().as_millis());
        }
        seen.push(node.clone());
        if descend(&node) {
            let Some(next) = children(&node) else {
                return (
                    seen.len(),
                    "unreadable_children",
                    started.elapsed().as_millis(),
                );
            };
            pending.extend(next);
        }
    }
    (seen.len(), "complete", started.elapsed().as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_pruning_still_finds_duplicate_in_later_window() {
        // Two windows expose the same target page. Neither page's large
        // conversation may consume the scan budget or hide the other page.
        let mut pages = Vec::new();
        let report = walk_pruned(
            0,
            5,
            Duration::from_secs(1),
            |n| {
                Some(match n {
                    0 => vec![1, 2],
                    1 => vec![3],
                    2 => vec![4],
                    _ => panic!("conversation descendants must not be read"),
                })
            },
            |n| {
                if *n >= 3 {
                    pages.push(*n);
                    false
                } else {
                    true
                }
            },
        );
        assert_eq!((report.0, report.1), (5, "complete"));
        assert_eq!(pages, [3, 4]);
    }

    #[test]
    fn cycles_do_not_create_false_duplicates_or_hide_incomplete_reads() {
        let report = walk_pruned(
            0,
            2,
            Duration::from_secs(1),
            |n| Some(if *n == 0 { vec![1, 0] } else { vec![0] }),
            |_| true,
        );
        assert_eq!((report.0, report.1), (2, "complete"));
        let report = walk_pruned(
            0,
            2,
            Duration::from_secs(1),
            |n| if *n == 0 { Some(vec![1]) } else { None },
            |_| true,
        );
        assert_eq!(report.1, "unreadable_children");
        let report = walk_pruned(0, 1, Duration::from_secs(1), |_| Some(vec![1]), |_| true);
        assert_eq!(report.1, "node_limit");
        let report = walk_pruned(0, 2, Duration::ZERO, |_| Some(vec![]), |_| true);
        assert_eq!(report.1, "time_limit");
    }
}
