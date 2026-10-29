//! Shared API for reference and optimized minimal-puzzle searches.

mod naive;
mod optimized;

pub use naive::Naive;
pub use optimized::Optimized;

use super::{
    fill_random_solution, SplitMix64, MAX_CLUE_COUNT, MAX_INTERMEDIATE_CLUE_COUNT, MIN_CLUE_COUNT,
};
use crate::archive::MinimalPuzzleAnalysis;
use crate::generator::DiverseGenerationError;
use crate::Matrix;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinimalSearchBudget {
    /// Number of independent candidate-generation attempts.
    pub candidate_attempts: u32,
    /// Node limit for each uniqueness or alternative-solution query.
    pub max_nodes_per_check: u64,
    /// Hitting-set search limit for each Diverse candidate attempt.
    pub max_hitting_set_nodes: u64,
    /// Neighborhood-query limit passed to each Diverse candidate attempt.
    pub max_neighborhood_checks: u64,
    /// Largest intermediate clue count explored by the Diverse strategy.
    pub max_intermediate_clues: usize,
}

impl Default for MinimalSearchBudget {
    fn default() -> Self {
        Self {
            candidate_attempts: 8,
            max_nodes_per_check: 100_000,
            max_hitting_set_nodes: 100_000,
            max_neighborhood_checks: 1_088,
            max_intermediate_clues: 18,
        }
    }
}

/// Backend contract for finding a puzzle whose exact clue count and
/// minimality have both been proven.
pub trait MinimalClueSearchBackend {
    fn search(
        seed: u64,
        clue_count: usize,
        budget: MinimalSearchBudget,
    ) -> Result<MinimalPuzzleAnalysis, DiverseGenerationError>;
}

pub fn search_with<B: MinimalClueSearchBackend>(
    seed: u64,
    clue_count: usize,
    budget: MinimalSearchBudget,
) -> Result<MinimalPuzzleAnalysis, DiverseGenerationError> {
    B::search(seed, clue_count, budget)
}

pub(super) fn validate(
    clue_count: usize,
    budget: MinimalSearchBudget,
) -> Result<(), DiverseGenerationError> {
    if !(MIN_CLUE_COUNT..=MAX_CLUE_COUNT).contains(&clue_count) {
        return Err(DiverseGenerationError::InvalidClueCount);
    }
    if budget.candidate_attempts == 0
        || budget.max_nodes_per_check == 0
        || budget.max_neighborhood_checks == 0
    {
        return Err(DiverseGenerationError::EmptyBudget);
    }
    if !(18..=MAX_INTERMEDIATE_CLUE_COUNT).contains(&budget.max_intermediate_clues) {
        return Err(DiverseGenerationError::InvalidIntermediateClueCount);
    }
    Ok(())
}

pub(super) fn random_clue_subset(
    clue_count: usize,
    rng: &mut SplitMix64,
) -> Option<(Matrix, Matrix)> {
    let mut solution = [[0; 9]; 9];
    let mut rows = [0; 9];
    let mut columns = [0; 9];
    let mut blocks = [0; 9];
    if !fill_random_solution(&mut solution, &mut rows, &mut columns, &mut blocks, rng) {
        return None;
    }
    let mut puzzle = solution;
    let mut positions = std::array::from_fn::<_, 81, _>(|position| position);
    rng.shuffle(&mut positions);
    for &position in positions.iter().skip(clue_count) {
        puzzle[position / 9][position % 9] = 0;
    }
    Some((puzzle, solution))
}

#[cfg(test)]
mod tests {
    use super::{search_with, MinimalSearchBudget, Naive, Optimized};

    #[test]
    fn backends_reject_invalid_clue_counts_and_empty_budgets() {
        let budget = MinimalSearchBudget {
            candidate_attempts: 1,
            max_nodes_per_check: 1_000,
            max_hitting_set_nodes: 1_000,
            max_neighborhood_checks: 1,
            max_intermediate_clues: 18,
        };
        assert!(search_with::<Optimized>(411, 16, budget).is_err());
        assert!(search_with::<Optimized>(
            411,
            17,
            MinimalSearchBudget {
                max_neighborhood_checks: 0,
                ..budget
            }
        )
        .is_err());
        assert!(search_with::<Naive>(
            411,
            17,
            MinimalSearchBudget {
                candidate_attempts: 0,
                ..budget
            }
        )
        .is_err());
    }
}
