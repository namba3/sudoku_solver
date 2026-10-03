pub mod solver;
pub use solver::{
    candidates_for, find_hint, solve, Hint, HintError, HintTechnique, SearchError, SearchEvent,
    SolutionSearch,
};

pub type Matrix = [[u8; 9]; 9];
