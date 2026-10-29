//! Native x86 and optional WebAssembly SIMD solver backend.

use super::backend::SolverBackend;
use super::optimized::SolutionSearchEngine;
use super::{AlternativeSolutionCheck, SearchError};
use crate::Matrix;

/// MRV solver that evaluates eight cells' candidate counts with SSE2 at a time.
pub struct Backend;

impl SolverBackend for Backend {
    type Search = SolutionSearchEngine<1>;

    fn search(board: &Matrix) -> Result<Self::Search, SearchError> {
        SolutionSearchEngine::<1>::new(board)
    }

    fn search_with_preferred_solution(
        board: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self::Search, SearchError> {
        SolutionSearchEngine::<1>::new_preferring_solution(board, preferred_solution)
    }

    fn solve(board: &mut Matrix) -> bool {
        super::optimized::solve_simd(board)
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

/// Solve using the SSE2 or WebAssembly SIMD128 backend selected for this target.
pub fn solve(board: &mut Matrix) -> bool {
    <Backend as SolverBackend>::solve(board)
}

/// Lazily enumerate solutions using the target's SIMD backend.
pub fn search(board: &Matrix) -> Result<SolutionSearchEngine<1>, SearchError> {
    <Backend as SolverBackend>::search(board)
}

/// Find an alternative solution using the target's SIMD backend.
pub fn find_alternative_solution(
    puzzle: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
) -> AlternativeSolutionCheck {
    super::backend::find_alternative_solution_with::<Backend>(puzzle, known_solution, max_nodes)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn candidate_counts(rows: [u16; 8], columns: [u16; 8], boxes: [u16; 8]) -> [u8; 8] {
    // SSE2 is part of the x86_64 baseline.
    unsafe { candidate_counts_sse2(rows, columns, boxes) }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn candidate_counts(rows: [u16; 8], columns: [u16; 8], boxes: [u16; 8]) -> [u8; 8] {
    // This backend is only included for wasm builds explicitly enabling it.
    unsafe { candidate_counts_wasm_simd(rows, columns, boxes) }
}

#[cfg(target_arch = "x86")]
pub(crate) fn candidate_counts(rows: [u16; 8], columns: [u16; 8], boxes: [u16; 8]) -> [u8; 8] {
    if std::is_x86_feature_detected!("sse2") {
        // The runtime check above proves the target feature is available.
        unsafe { candidate_counts_sse2(rows, columns, boxes) }
    } else {
        scalar_candidate_counts(rows, columns, boxes)
    }
}

#[cfg(target_arch = "x86")]
fn scalar_candidate_counts(rows: [u16; 8], columns: [u16; 8], boxes: [u16; 8]) -> [u8; 8] {
    std::array::from_fn(|lane| {
        let used = rows[lane] | columns[lane] | boxes[lane];
        9 - used.count_ones() as u8
    })
}

#[cfg(target_arch = "wasm32")]
use std::arch::wasm32::*;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
unsafe fn candidate_counts_sse2(rows: [u16; 8], columns: [u16; 8], boxes: [u16; 8]) -> [u8; 8] {
    let rows = _mm_loadu_si128(rows.as_ptr().cast());
    let columns = _mm_loadu_si128(columns.as_ptr().cast());
    let boxes = _mm_loadu_si128(boxes.as_ptr().cast());
    let used = _mm_or_si128(_mm_or_si128(rows, columns), boxes);

    // Parallel 16-bit SWAR popcount. The masks stay within each 16-bit lane.
    let ones = _mm_set1_epi16(0x5555);
    let twos = _mm_set1_epi16(0x3333);
    let fours = _mm_set1_epi16(0x0f0f);
    let mut counts = _mm_sub_epi16(used, _mm_and_si128(_mm_srli_epi16::<1>(used), ones));
    counts = _mm_add_epi16(
        _mm_and_si128(counts, twos),
        _mm_and_si128(_mm_srli_epi16::<2>(counts), twos),
    );
    counts = _mm_and_si128(_mm_add_epi16(counts, _mm_srli_epi16::<4>(counts)), fours);
    counts = _mm_add_epi16(counts, _mm_srli_epi16::<8>(counts));
    counts = _mm_and_si128(counts, _mm_set1_epi16(0x001f));

    let mut used_counts = [0u16; 8];
    _mm_storeu_si128(used_counts.as_mut_ptr().cast(), counts);
    std::array::from_fn(|lane| 9 - used_counts[lane] as u8)
}

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
unsafe fn candidate_counts_wasm_simd(
    rows: [u16; 8],
    columns: [u16; 8],
    boxes: [u16; 8],
) -> [u8; 8] {
    let rows = u16x8(
        rows[0], rows[1], rows[2], rows[3], rows[4], rows[5], rows[6], rows[7],
    );
    let columns = u16x8(
        columns[0], columns[1], columns[2], columns[3], columns[4], columns[5], columns[6],
        columns[7],
    );
    let boxes = u16x8(
        boxes[0], boxes[1], boxes[2], boxes[3], boxes[4], boxes[5], boxes[6], boxes[7],
    );
    let used = v128_or(v128_or(rows, columns), boxes);

    // Parallel 16-bit SWAR popcount over the eight candidate cells.
    let ones = u16x8_splat(0x5555);
    let twos = u16x8_splat(0x3333);
    let fours = u16x8_splat(0x0f0f);
    let mut counts = u16x8_sub(used, v128_and(u16x8_shr(used, 1), ones));
    counts = u16x8_add(v128_and(counts, twos), v128_and(u16x8_shr(counts, 2), twos));
    counts = v128_and(u16x8_add(counts, u16x8_shr(counts, 4)), fours);
    counts = u16x8_add(counts, u16x8_shr(counts, 8));
    counts = v128_and(counts, u16x8_splat(0x001f));

    let used_counts: [u16; 8] = std::mem::transmute(counts);
    std::array::from_fn(|lane| 9 - used_counts[lane] as u8)
}

#[cfg(test)]
mod tests {
    use super::Backend as Simd;
    use crate::solver::backend::{Optimized, SolverBackend};
    use crate::solver::SearchEvent;
    use crate::Matrix;

    const PUZZLE: Matrix = [
        [5, 3, 0, 0, 7, 0, 0, 0, 0],
        [6, 0, 0, 1, 9, 5, 0, 0, 0],
        [0, 9, 8, 0, 0, 0, 0, 6, 0],
        [8, 0, 0, 0, 6, 0, 0, 0, 3],
        [4, 0, 0, 8, 0, 3, 0, 0, 1],
        [7, 0, 0, 0, 2, 0, 0, 0, 6],
        [0, 6, 0, 0, 0, 0, 2, 8, 0],
        [0, 0, 0, 4, 1, 9, 0, 0, 5],
        [0, 0, 0, 0, 8, 0, 0, 7, 9],
    ];

    const TWO_SOLUTION: Matrix = [
        [5, 3, 4, 6, 7, 8, 9, 1, 2],
        [6, 7, 2, 1, 9, 5, 3, 4, 8],
        [1, 9, 8, 3, 4, 2, 5, 6, 7],
        [8, 5, 9, 7, 6, 0, 4, 2, 0],
        [4, 2, 6, 8, 5, 0, 7, 9, 0],
        [7, 1, 3, 9, 2, 4, 8, 5, 6],
        [9, 6, 1, 5, 3, 7, 2, 8, 4],
        [2, 8, 7, 4, 1, 9, 6, 3, 5],
        [3, 4, 5, 2, 8, 6, 1, 7, 9],
    ];

    fn solutions<B: SolverBackend>(puzzle: &Matrix) -> (Vec<Matrix>, u64) {
        let mut search = B::search(puzzle).unwrap();
        let boards = search
            .by_ref()
            .filter_map(|event| match event {
                SearchEvent::SolutionFound { board, .. } => Some(board),
                SearchEvent::Progress { .. } => None,
            })
            .collect();
        (boards, B::explored_nodes(&search))
    }

    #[test]
    fn simd_candidate_counts_match_scalar_for_all_lane_patterns() {
        for seed in 0..512u16 {
            let rows =
                std::array::from_fn(|lane| seed.rotate_left(lane as u32).wrapping_mul(37) & 0x01ff);
            let columns = std::array::from_fn(|lane| {
                (seed
                    .wrapping_mul(lane as u16 + 11)
                    .rotate_right(lane as u32))
                    & 0x01ff
            });
            let boxes = std::array::from_fn(|lane| (seed.wrapping_add(lane as u16 * 19)) & 0x01ff);
            assert_eq!(
                super::candidate_counts(rows, columns, boxes),
                std::array::from_fn(|lane| {
                    9 - (rows[lane] | columns[lane] | boxes[lane]).count_ones() as u8
                })
            );
        }
    }

    #[test]
    fn simd_search_preserves_solution_order_and_node_counts() {
        for puzzle in [PUZZLE, TWO_SOLUTION] {
            assert_eq!(solutions::<Optimized>(&puzzle), solutions::<Simd>(&puzzle));
            assert_eq!(
                solutions::<Optimized>(&puzzle),
                solutions::<super::super::optimized_u64::Backend>(&puzzle)
            );
        }
    }

    #[test]
    fn simd_solve_matches_optimized() {
        let mut optimized = PUZZLE;
        let mut simd = PUZZLE;
        let mut packed = PUZZLE;
        let optimized_solved = Optimized::solve(&mut optimized);
        assert_eq!(optimized_solved, Simd::solve(&mut simd));
        assert_eq!(
            optimized_solved,
            super::super::optimized_u64::Backend::solve(&mut packed)
        );
        assert_eq!(optimized, simd);
        assert_eq!(optimized, packed);
    }
}
