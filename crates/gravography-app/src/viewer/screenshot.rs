//! FR-X1 screenshots: a tone-mapped PNG and a linear HDR EXR of the current view at the full window resolution,
//! rendered to convergence first.

use std::path::{Path, PathBuf};

/// `<dir>/gravography-<unix seconds>-<n>.png` and `.exr` (the counter keeps names unique within a second).
pub fn screenshot_paths(dir: &Path, unix_seconds: u64, n: u32) -> (PathBuf, PathBuf) {
    let stem = format!("gravography-{unix_seconds}-{n:03}");
    (dir.join(format!("{stem}.png")), dir.join(format!("{stem}.exr")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_paired() {
        let (a, b) = screenshot_paths(Path::new("shots"), 1_700_000_000, 7);
        assert_eq!(a, Path::new("shots/gravography-1700000000-007.png"));
        assert_eq!(b, Path::new("shots/gravography-1700000000-007.exr"));
        assert_ne!(screenshot_paths(Path::new("s"), 1, 1).0, screenshot_paths(Path::new("s"), 1, 2).0);
    }
}
