//! Choices about paints that tile and free-form mode share.

use crate::colour::Rgb;

/// Merges paints the person chose to merge. Each `(from, to)` pair gives every cell of
/// colour `from` to colour `to`, in order, so a later pair sees the earlier ones and chains
/// work. A pair naming a colour that is not, or no longer, in the palette changes nothing,
/// which keeps a list of merges valid while other settings change.
///
/// Returns the palette in its old order less the colours merged away, and for each old
/// paint the index of the paint that now holds its cells.
pub fn merge_plan(palette: &[Rgb], merges: &[(Rgb, Rgb)]) -> (Vec<Rgb>, Vec<usize>) {
    let index = |colour: Rgb| palette.iter().position(|&c| c == colour);
    // The palette entry each entry's cells now belong to.
    let mut owner: Vec<usize> = (0..palette.len()).collect();
    for &(from, to) in merges {
        let (Some(f), Some(t)) = (index(from), index(to)) else { continue };
        if f == t || owner[f] != f || owner[t] != t {
            continue;
        }
        for o in &mut owner {
            if *o == f {
                *o = t;
            }
        }
    }
    let mut kept = Vec::new();
    let mut renumber = vec![0; palette.len()];
    for (i, &colour) in palette.iter().enumerate() {
        if owner[i] == i {
            renumber[i] = kept.len();
            kept.push(colour);
        }
    }
    (kept, owner.iter().map(|&o| renumber[o]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Rgb; 4] = [[0, 0, 0], [80, 0, 0], [160, 0, 0], [240, 0, 0]];

    #[test]
    fn merging_drops_the_paint_and_moves_its_cells() {
        let (palette, moved) = merge_plan(&PALETTE, &[([80, 0, 0], [240, 0, 0])]);
        assert_eq!(palette, [[0, 0, 0], [160, 0, 0], [240, 0, 0]]);
        assert_eq!(moved, [0, 2, 1, 2]);
    }

    #[test]
    fn merges_chain_and_ignore_what_is_missing() {
        let (palette, moved) = merge_plan(&PALETTE, &[([0, 0, 0], [80, 0, 0]), ([80, 0, 0], [160, 0, 0])]);
        assert_eq!(palette, [[160, 0, 0], [240, 0, 0]]);
        assert_eq!(moved, [0, 0, 0, 1]);
        let unchanged = merge_plan(&PALETTE, &[([1, 2, 3], [0, 0, 0]), ([80, 0, 0], [80, 0, 0])]);
        assert_eq!(unchanged, (PALETTE.to_vec(), vec![0, 1, 2, 3]));
    }
}
