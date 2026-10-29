//! Reference minimal-puzzle search by random clue-subset sampling.

use super::{random_clue_subset, validate, MinimalClueSearchBackend, MinimalSearchBudget};
use crate::archive::minimality;
use crate::archive::{MinimalPuzzleAnalysis, VerificationStatus};
use crate::generator::{DiverseGenerationError, SplitMix64};

/// Samples random clue subsets and keeps only candidates proven minimal by the
/// reference verifier.
pub struct Naive;

impl MinimalClueSearchBackend for Naive {
    fn search(
        seed: u64,
        clue_count: usize,
        budget: MinimalSearchBudget,
    ) -> Result<MinimalPuzzleAnalysis, DiverseGenerationError> {
        validate(clue_count, budget)?;
        let mut rng = SplitMix64(seed);
        let limit = budget
            .candidate_attempts
            .min(u32::try_from(budget.max_neighborhood_checks).unwrap_or(u32::MAX));
        let mut total_nodes = 0u64;

        for _ in 0..limit {
            let Some((puzzle, solution)) = random_clue_subset(clue_count, &mut rng) else {
                continue;
            };
            let analysis = minimality::analyze_with::<minimality::Naive>(
                puzzle,
                solution,
                budget.max_nodes_per_check,
            )
            .map_err(|_| DiverseGenerationError::InvalidParent)?;
            total_nodes = total_nodes.saturating_add(analysis.evidence.total_nodes);
            if analysis.status == VerificationStatus::VerifiedMinimal {
                return Ok(analysis);
            }
        }

        Err(DiverseGenerationError::BudgetExhausted {
            attempts: limit,
            uniqueness_nodes: total_nodes,
            hitting_set_nodes: 0,
            neighborhood_checks: u64::from(limit),
        })
    }
}
