//! Reference minimality verification implementation.

use super::{analyze_using, MinimalityBackend};
use crate::archive::{MinimalPuzzleAnalysis, MinimalityError};
use crate::Matrix;

/// Reference implementation that validates the supplied completion on every
/// alternative-solution query.
pub struct Naive;

impl MinimalityBackend for Naive {
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
                crate::solver::find_alternative_solution(puzzle, solution, nodes)
            },
        )
    }
}
