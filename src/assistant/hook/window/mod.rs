//! The window the assistant runs in: the hook's nearest ancestor, at most 12 up, that owns a window you can raise.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

const MOST_ANCESTORS: usize = 12;

#[cfg(target_os = "macos")]
pub(super) fn owner() -> Option<u32> {
    self::macos::owner(std::process::id())
}

#[cfg(windows)]
pub(super) fn owner() -> Option<u32> {
    self::windows::owner(std::process::id())
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(super) fn owner() -> Option<u32> {
    None
}

#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
fn nearest(start: u32, parent: impl Fn(u32) -> Option<u32>, owns: impl Fn(u32) -> bool) -> Option<u32> {
    let mut pid = start;
    for _ in 0..MOST_ANCESTORS {
        // Stop at the root, or at a process that claims itself as parent.
        pid = parent(pid).filter(|next| *next != 0 && *next != pid)?;
        if owns(pid) {
            return Some(pid);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn chain(links: &[(u32, u32)]) -> impl Fn(u32) -> Option<u32> {
        let parents: HashMap<u32, u32> = links.iter().copied().collect();
        move |pid| parents.get(&pid).copied()
    }

    #[test]
    fn nearest_ancestor_with_window_wins() {
        let parents = chain(&[(10, 20), (20, 30), (30, 40), (40, 50)]);
        assert_eq!(nearest(10, &parents, |pid| pid == 30 || pid == 50), Some(30));
        assert_eq!(nearest(10, &parents, |pid| pid == 10), None, "the hook itself never counts");
        assert_eq!(nearest(10, &parents, |_| false), None);
    }

    #[test]
    fn walk_stops_at_twelve_or_a_loop() {
        let links: Vec<(u32, u32)> = (1..=20).map(|pid| (pid, pid + 1)).collect();
        let parents = chain(&links);
        assert_eq!(nearest(1, &parents, |pid| pid == 13), Some(13));
        assert_eq!(nearest(1, &parents, |pid| pid == 14), None);
        assert_eq!(nearest(5, chain(&[(5, 5)]), |_| true), None);
        assert_eq!(nearest(5, chain(&[(5, 0)]), |_| true), None);
    }

    #[test]
    #[ignore = "needs a desktop session: prints the window owner of the shell running the tests"]
    fn this_process_has_a_window_owner() {
        let owner = owner();
        println!("window owner: {owner:?}");
        assert!(owner.is_some());
    }
}
