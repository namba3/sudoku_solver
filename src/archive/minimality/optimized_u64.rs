//! Minimality verification using the packed-u64 solution-search backend.

use super::{analyze_using, MinimalityBackend};
use crate::archive::{MinimalPuzzleAnalysis, MinimalityError};
use crate::Matrix;

/// Minimality verifier backed by four-lane packed-u64 MRV selection.
pub struct OptimizedU64;

impl MinimalityBackend for OptimizedU64 {
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
                crate::solver::optimized::find_alternative_solution_for_verified_solution_u64(
                    puzzle, solution, nodes,
                )
            },
        )
    }
}
