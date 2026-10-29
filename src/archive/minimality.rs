//! Interchangeable minimality-verification implementations.

mod naive;
mod optimized;
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(target_arch = "x86", target_arch = "x86_64")
    ),
    all(target_arch = "wasm32", feature = "wasm-simd")
))]
mod optimized_simd;
mod optimized_u64;

pub use naive::Naive;
pub use optimized::Optimized;
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(target_arch = "x86", target_arch = "x86_64")
    ),
    all(target_arch = "wasm32", feature = "wasm-simd")
))]
pub use optimized_simd::OptimizedSimd;
pub use optimized_u64::OptimizedU64;

use super::{
    canonical_key, is_complete_solution, matches_clues, solution_difference_mask,
    AlternativeSolutionStatus, DeletionStatus, DeletionWitness, MinimalPuzzleAnalysis,
    MinimalityError, MinimalityEvidence, VerificationStatus,
};
use crate::Matrix;

#[cfg(test)]
mod tests {
    use super::{analyze_with, Naive, Optimized, OptimizedU64};
    use crate::archive::{analyze_minimality, VerificationStatus};
    use crate::generator::verified_17_clue_base;

    #[test]
    fn reference_and_optimized_minimality_results_match() {
        let (puzzle, solution) = verified_17_clue_base(411);
        let reference = analyze_with::<Naive>(puzzle, solution, 100_000).unwrap();
        let optimized = analyze_with::<Optimized>(puzzle, solution, 100_000).unwrap();
        let packed = analyze_with::<OptimizedU64>(puzzle, solution, 100_000).unwrap();
        assert_eq!(reference, optimized);
        assert_eq!(optimized, packed);
        assert_eq!(
            analyze_minimality(puzzle, solution, 100_000).unwrap(),
            optimized
        );
        assert_eq!(optimized.status, VerificationStatus::VerifiedMinimal);
    }

    #[cfg(any(
        all(
            not(target_family = "wasm"),
            any(target_arch = "x86", target_arch = "x86_64")
        ),
        all(target_arch = "wasm32", feature = "wasm-simd")
    ))]
    #[test]
    fn simd_minimality_result_matches_optimized() {
        let (puzzle, solution) = verified_17_clue_base(411);
        let optimized = analyze_with::<Optimized>(puzzle, solution, 100_000).unwrap();
        let simd = analyze_with::<super::OptimizedSimd>(puzzle, solution, 100_000).unwrap();
        assert_eq!(optimized, simd);
    }
}

/// Common contract for verifying puzzle uniqueness and clue minimality.
pub trait MinimalityBackend {
    fn analyze(
        puzzle: Matrix,
        solution: Matrix,
        max_nodes_per_check: u64,
    ) -> Result<MinimalPuzzleAnalysis, MinimalityError>;
}

/// Run minimality verification with the selected implementation.
pub fn analyze_with<B: MinimalityBackend>(
    puzzle: Matrix,
    solution: Matrix,
    max_nodes_per_check: u64,
) -> Result<MinimalPuzzleAnalysis, MinimalityError> {
    B::analyze(puzzle, solution, max_nodes_per_check)
}

pub(super) fn analyze_using(
    puzzle: Matrix,
    solution: Matrix,
    max_nodes_per_check: u64,
    find_alternative: impl Fn(&Matrix, &Matrix, u64) -> crate::solver::AlternativeSolutionCheck,
) -> Result<MinimalPuzzleAnalysis, MinimalityError> {
    if max_nodes_per_check == 0 {
        return Err(MinimalityError::InvalidNodeBudget);
    }
    if puzzle
        .iter()
        .flatten()
        .any(|&value| value != 0 && !(1..=9).contains(&value))
    {
        return Err(MinimalityError::InvalidPuzzleValues);
    }
    if !is_complete_solution(&solution) {
        return Err(MinimalityError::InvalidSolution);
    }
    if !matches_clues(&solution, &puzzle) {
        return Err(MinimalityError::SolutionDoesNotMatchPuzzle);
    }

    let clue_count = puzzle
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count() as u8;
    let input_check = find_alternative(&puzzle, &solution, max_nodes_per_check);
    let mut evidence = MinimalityEvidence {
        input_unique: input_check.status == AlternativeSolutionStatus::Unique,
        input_proof_nodes: input_check.explored_nodes,
        deletions: Vec::with_capacity(clue_count as usize),
        total_nodes: input_check.explored_nodes,
    };
    let status = match input_check.status {
        AlternativeSolutionStatus::AlternativeFound | AlternativeSolutionStatus::InvalidInput => {
            VerificationStatus::InvalidPuzzle
        }
        AlternativeSolutionStatus::Unknown => VerificationStatus::BudgetExceeded,
        AlternativeSolutionStatus::Unique => {
            let mut redundant_clue = false;
            let mut unknown_deletion = false;
            let mut direct_reductions = Vec::new();
            for position in 0..81 {
                if !(1..=9).contains(&puzzle[position / 9][position % 9]) {
                    continue;
                }
                let mut reduced = puzzle;
                reduced[position / 9][position % 9] = 0;
                let check = find_alternative(&reduced, &solution, max_nodes_per_check);
                evidence.total_nodes = evidence.total_nodes.saturating_add(check.explored_nodes);
                let (deletion_status, alternative, difference_mask) = match check.status {
                    AlternativeSolutionStatus::AlternativeFound => {
                        let alternative = check.alternative;
                        let difference_mask = alternative
                            .as_ref()
                            .map_or(0, |other| solution_difference_mask(&solution, other));
                        (
                            DeletionStatus::AlternativeFound,
                            alternative,
                            difference_mask,
                        )
                    }
                    AlternativeSolutionStatus::Unique => {
                        redundant_clue = true;
                        direct_reductions.push(reduced);
                        (DeletionStatus::UniqueAfterDeletion, None, 0)
                    }
                    AlternativeSolutionStatus::Unknown => {
                        unknown_deletion = true;
                        (DeletionStatus::Unknown, None, 0)
                    }
                    AlternativeSolutionStatus::InvalidInput => {
                        return Err(MinimalityError::SolutionDoesNotMatchPuzzle)
                    }
                };
                evidence.deletions.push(DeletionWitness {
                    position: position as u8,
                    status: deletion_status,
                    alternative,
                    difference_mask,
                    explored_nodes: check.explored_nodes,
                });
            }
            let status = if redundant_clue {
                VerificationStatus::VerifiedNonMinimal
            } else if unknown_deletion {
                VerificationStatus::BudgetExceeded
            } else {
                VerificationStatus::VerifiedMinimal
            };
            return Ok(MinimalPuzzleAnalysis {
                puzzle,
                solution,
                clue_count,
                puzzle_key: canonical_key(&puzzle),
                solution_class: canonical_key(&solution),
                status,
                input_status: AlternativeSolutionStatus::Unique,
                evidence,
                direct_reductions,
            });
        }
    };
    Ok(MinimalPuzzleAnalysis {
        puzzle,
        solution,
        clue_count,
        puzzle_key: canonical_key(&puzzle),
        solution_class: canonical_key(&solution),
        status,
        input_status: input_check.status,
        evidence,
        direct_reductions: Vec::new(),
    })
}
