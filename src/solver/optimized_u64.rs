//! Scalar MRV backend using four packed 16-bit lanes in a `u64`.

use super::backend::SolverBackend;
use super::optimized::SolutionSearchEngine;
use super::SearchError;
use crate::Matrix;

/// MRV solver that computes four candidate counts with packed `u64` operations.
pub struct Backend;

impl SolverBackend for Backend {
    type Search = SolutionSearchEngine<2>;

    fn search(board: &Matrix) -> Result<Self::Search, SearchError> {
        SolutionSearchEngine::<2>::new(board)
    }

    fn search_with_preferred_solution(
        board: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self::Search, SearchError> {
        SolutionSearchEngine::<2>::new_preferring_solution(board, preferred_solution)
    }

    fn solve(board: &mut Matrix) -> bool {
        super::optimized::solve_with_selection::<2>(board)
    }

    fn candidates_for(board: &Matrix, x: usize, y: usize) -> Vec<u8> {
        super::optimized::candidates_for(board, x, y)
    }

    fn explored_nodes(search: &Self::Search) -> u64 {
        search.explored_nodes()
    }

    fn solutions_found(search: &Self::Search) -> usize {
        search.solutions_found()
    }
}

/// Return candidate counts for four aligned lanes of row, column, and box masks.
pub fn candidate_counts(rows: [u16; 4], columns: [u16; 4], boxes: [u16; 4]) -> [u8; 4] {
    const PAIRS: u64 = 0x5555_5555_5555_5555;
    const TWO_BIT_GROUPS: u64 = 0x3333_3333_3333_3333;
    const BYTE_NIBBLES: u64 = 0x0f0f_0f0f_0f0f_0f0f;
    const LOW_BYTE_PER_WORD: u64 = 0x000f_000f_000f_000f;

    let mut used = pack_lanes(rows) | pack_lanes(columns) | pack_lanes(boxes);
    used -= (used >> 1) & PAIRS;
    used = (used & TWO_BIT_GROUPS) + ((used >> 2) & TWO_BIT_GROUPS);
    used = (used + (used >> 4)) & BYTE_NIBBLES;
    let counts = (used & LOW_BYTE_PER_WORD) + ((used >> 8) & LOW_BYTE_PER_WORD);

    std::array::from_fn(|lane| {
        let used_count = ((counts >> (lane * 16)) & 0x0f) as u8;
        9 - used_count
    })
}

fn pack_lanes(lanes: [u16; 4]) -> u64 {
    u64::from(lanes[0])
        | (u64::from(lanes[1]) << 16)
        | (u64::from(lanes[2]) << 32)
        | (u64::from(lanes[3]) << 48)
}

#[cfg(test)]
mod tests {
    use super::candidate_counts;

    #[test]
    fn packed_counts_match_individual_popcounts_for_all_lane_patterns() {
        for seed in 0..4096u16 {
            let rows =
                std::array::from_fn(|lane| seed.rotate_left(lane as u32).wrapping_mul(37) & 0x01ff);
            let columns = std::array::from_fn(|lane| {
                seed.wrapping_mul(lane as u16 + 11)
                    .rotate_right(lane as u32)
                    & 0x01ff
            });
            let boxes = std::array::from_fn(|lane| seed.wrapping_add(lane as u16 * 19) & 0x01ff);
            assert_eq!(
                candidate_counts(rows, columns, boxes),
                std::array::from_fn(|lane| {
                    9 - (rows[lane] | columns[lane] | boxes[lane]).count_ones() as u8
                })
            );
        }
    }
}
