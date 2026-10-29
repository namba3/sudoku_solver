//! Shared contract for interchangeable solution-search implementations.

use super::{AlternativeSolutionCheck, SearchError, SearchEvent, UniquenessCheck};
use crate::Matrix;

/// A solution-search implementation with the same board and result contract.
///
/// Backends may visit a different number of nodes and yield solutions in a
/// different order. Exhaustion, solution contents, and bounded-result meaning
/// must agree. `Search` is lazy so callers can stop after the first or second
/// solution without enumerating the rest of the solution space.
pub trait SolverBackend {
    type Search: Iterator<Item = SearchEvent>;

    fn search(board: &Matrix) -> Result<Self::Search, SearchError>;
    fn search_with_preferred_solution(
        board: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self::Search, SearchError>;
    fn solve(board: &mut Matrix) -> bool;
    fn candidates_for(board: &Matrix, x: usize, y: usize) -> Vec<u8>;
    fn explored_nodes(search: &Self::Search) -> u64;
    fn solutions_found(search: &Self::Search) -> usize;
}

/// Scalar MRV and bit-mask implementation retained for backend comparisons.
pub struct Optimized;

impl SolverBackend for Optimized {
    type Search = super::optimized::SolutionSearchEngine<0>;

    fn search(board: &Matrix) -> Result<Self::Search, SearchError> {
        Self::Search::new(board)
    }

    fn search_with_preferred_solution(
        board: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self::Search, SearchError> {
        Self::Search::new_preferring_solution(board, preferred_solution)
    }

    fn solve(board: &mut Matrix) -> bool {
        super::optimized::solve_with_selection::<0>(board)
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

/// Count solutions up to two using a selected backend.
pub fn check_uniqueness_with<B: SolverBackend>(
    board: &Matrix,
    max_nodes: u64,
) -> Result<UniquenessCheck, SearchError> {
    let mut search = B::search(board)?;
    if max_nodes == 0 {
        return Ok(UniquenessCheck {
            status: super::UniquenessStatus::Unknown,
            solutions_found: 0,
            explored_nodes: 0,
        });
    }

    loop {
        let explored_nodes = B::explored_nodes(&search);
        if explored_nodes >= max_nodes {
            return Ok(UniquenessCheck {
                status: super::UniquenessStatus::Unknown,
                solutions_found: B::solutions_found(&search),
                explored_nodes,
            });
        }
        if search.next().is_none() {
            return Ok(UniquenessCheck {
                status: if B::solutions_found(&search) == 0 {
                    super::UniquenessStatus::NoSolution
                } else {
                    super::UniquenessStatus::Unique
                },
                solutions_found: B::solutions_found(&search),
                explored_nodes: B::explored_nodes(&search),
            });
        }
        if B::solutions_found(&search) >= 2 {
            return Ok(UniquenessCheck {
                status: super::UniquenessStatus::Multiple,
                solutions_found: B::solutions_found(&search),
                explored_nodes: B::explored_nodes(&search),
            });
        }
    }
}

/// Find a solution different from a supplied completion using a selected backend.
pub fn find_alternative_solution_with<B: SolverBackend>(
    puzzle: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
) -> AlternativeSolutionCheck {
    find_alternative_solution_with_order::<B>(
        puzzle,
        known_solution,
        max_nodes,
        super::AlternativeSearchOrder::SolverDefault,
    )
}

/// Find a different solution using a selected backend and explicit branch order.
pub fn find_alternative_solution_with_order<B: SolverBackend>(
    puzzle: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
    order: super::AlternativeSearchOrder,
) -> AlternativeSolutionCheck {
    let clues_match = (0..9).all(|row| {
        (0..9).all(|col| {
            !(1..=9).contains(&puzzle[row][col]) || puzzle[row][col] == known_solution[row][col]
        })
    });
    let mut checked_solution = *known_solution;
    if !clues_match || !B::solve(&mut checked_solution) || checked_solution != *known_solution {
        return AlternativeSolutionCheck {
            status: super::AlternativeSolutionStatus::InvalidInput,
            alternative: None,
            explored_nodes: 0,
        };
    }

    let search_result = match order {
        super::AlternativeSearchOrder::SolverDefault => B::search(puzzle),
        super::AlternativeSearchOrder::PreferKnownValues => {
            B::search_with_preferred_solution(puzzle, *known_solution)
        }
    };
    let Ok(mut search) = search_result else {
        return AlternativeSolutionCheck {
            status: super::AlternativeSolutionStatus::InvalidInput,
            alternative: None,
            explored_nodes: 0,
        };
    };
    loop {
        let explored_nodes = B::explored_nodes(&search);
        if explored_nodes >= max_nodes {
            return AlternativeSolutionCheck {
                status: super::AlternativeSolutionStatus::Unknown,
                alternative: None,
                explored_nodes,
            };
        }
        match search.next() {
            Some(SearchEvent::SolutionFound { board, .. }) if board != *known_solution => {
                return AlternativeSolutionCheck {
                    status: super::AlternativeSolutionStatus::AlternativeFound,
                    alternative: Some(board),
                    explored_nodes: B::explored_nodes(&search),
                };
            }
            Some(SearchEvent::SolutionFound { .. } | SearchEvent::Progress { .. }) => {}
            None => {
                return AlternativeSolutionCheck {
                    status: if B::solutions_found(&search) > 0 {
                        super::AlternativeSolutionStatus::Unique
                    } else {
                        super::AlternativeSolutionStatus::InvalidInput
                    },
                    alternative: None,
                    explored_nodes: B::explored_nodes(&search),
                };
            }
        }
    }
}
