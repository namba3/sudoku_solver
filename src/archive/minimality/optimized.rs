//! Optimized minimality verification implementation.

use super::{analyze_using, MinimalityBackend};
use crate::archive::{MinimalPuzzleAnalysis, MinimalityError};
use crate::Matrix;

/// Reuses the completion proof established during input validation instead of
/// re-solving that completion for each clue.
pub struct Optimized;

impl MinimalityBackend for Optimized {
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
                crate::solver::optimized::find_alternative_solution_for_verified_solution(
                    puzzle, solution, nodes,
                )
            },
        )
    }
}
