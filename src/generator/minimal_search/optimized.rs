//! Minimal-puzzle search using Diverse candidate generation and optimized
//! minimality verification.

use super::{validate, MinimalClueSearchBackend, MinimalSearchBudget};
use crate::archive::minimality;
use crate::archive::{MinimalPuzzleAnalysis, VerificationStatus};
use crate::generator::{DiverseBudget, DiverseGenerationError, DiverseStrategy};

/// Evolves unique states with Diverse's adaptive neighborhoods and returns a
/// candidate only after the optimized minimality proof is complete.
pub struct Optimized;

impl MinimalClueSearchBackend for Optimized {
    fn search(
        seed: u64,
        clue_count: usize,
        budget: MinimalSearchBudget,
    ) -> Result<MinimalPuzzleAnalysis, DiverseGenerationError> {
        validate(clue_count, budget)?;
        let mut total_nodes = 0u64;
        let mut total_hitting_set_nodes = 0u64;
        let mut checks = 0u64;
        for attempt in 0..budget.candidate_attempts {
            let generation_budget = DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: budget.max_nodes_per_check,
                max_hitting_set_nodes: budget.max_hitting_set_nodes,
                max_neighborhood_checks: budget.max_neighborhood_checks,
                max_intermediate_clues: budget.max_intermediate_clues,
                strategy: DiverseStrategy::Adaptive,
            };
            let mut observed_checks = 0;
            match super::super::generate_diverse_with_progress(
                seed.wrapping_add(u64::from(attempt)),
                clue_count,
                generation_budget,
                |progress| observed_checks = progress.neighborhood_checks,
            ) {
                Ok(puzzle) => {
                    checks = checks.saturating_add(observed_checks);
                    let mut search = crate::solver::optimized::SolutionSearch::new(&puzzle)
                        .map_err(|_| DiverseGenerationError::InvalidParent)?;
                    let solution = loop {
                        if search.explored_nodes() >= budget.max_nodes_per_check {
                            return Err(DiverseGenerationError::BudgetExhausted {
                                attempts: attempt + 1,
                                uniqueness_nodes: total_nodes
                                    .saturating_add(search.explored_nodes()),
                                hitting_set_nodes: total_hitting_set_nodes,
                                neighborhood_checks: checks,
                            });
                        }
                        match search.next() {
                            Some(crate::solver::SearchEvent::SolutionFound { board, .. }) => {
                                break board;
                            }
                            Some(crate::solver::SearchEvent::Progress { .. }) => {}
                            None => return Err(DiverseGenerationError::InvalidParent),
                        }
                    };
                    let analysis = minimality::analyze_with::<minimality::Optimized>(
                        puzzle,
                        solution,
                        budget.max_nodes_per_check,
                    )
                    .map_err(|_| DiverseGenerationError::InvalidParent)?;
                    total_nodes = total_nodes.saturating_add(analysis.evidence.total_nodes);
                    if analysis.status == VerificationStatus::VerifiedMinimal
                        && analysis.clue_count as usize == clue_count
                    {
                        return Ok(analysis);
                    }
                }
                Err(DiverseGenerationError::BudgetExhausted {
                    uniqueness_nodes,
                    hitting_set_nodes: candidate_hitting_set_nodes,
                    neighborhood_checks,
                    ..
                }) => {
                    total_nodes = total_nodes.saturating_add(uniqueness_nodes);
                    total_hitting_set_nodes =
                        total_hitting_set_nodes.saturating_add(candidate_hitting_set_nodes);
                    checks = checks.saturating_add(neighborhood_checks.max(observed_checks));
                }
                Err(error) => return Err(error),
            }
        }
        Err(DiverseGenerationError::BudgetExhausted {
            attempts: budget.candidate_attempts,
            uniqueness_nodes: total_nodes,
            hitting_set_nodes: total_hitting_set_nodes,
            neighborhood_checks: checks,
        })
    }
}
