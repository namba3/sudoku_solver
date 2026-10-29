//! Minimality verification using the native SIMD solution-search backend.

use super::{analyze_using, MinimalityBackend};
use crate::archive::{MinimalPuzzleAnalysis, MinimalityError};
use crate::Matrix;

/// Minimality verifier backed by native SIMD MRV candidate counting.
pub struct OptimizedSimd;

impl MinimalityBackend for OptimizedSimd {
    fn analyze(
        puzzle: Matrix,
        solution: Matrix,
        max_nodes_per_check: u64,
    ) -> Result<MinimalPuzzleAnalysis, MinimalityError> {
        analyze_using(
            puzzle,
            solution,
            max_nodes_per_check,
            |puzzle, solution, nodes| {
                crate::solver::optimized::find_alternative_solution_for_verified_solution_simd(
                    puzzle, solution, nodes,
                )
            },
        )
    }
}
