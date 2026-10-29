pub mod archive;
pub mod generator;
mod minimal_puzzle_seeds;
pub mod multi_clue;
mod persistence;
pub mod pivot;
pub mod solver;
pub mod symmetry;
pub use solver::{
    candidates_for, check_uniqueness, find_alternative_solution,
    find_alternative_solution_with_order, find_hint, solve, AlternativeSearchOrder,
    AlternativeSolutionCheck, AlternativeSolutionStatus, Hint, HintError, HintTechnique,
    SearchError, SearchEvent, SolutionSearch, UniquenessCheck, UniquenessStatus,
};

pub type Matrix = [[u8; 9]; 9];
