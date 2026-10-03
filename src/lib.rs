pub mod solver;
pub use solver::{candidates_for, solve};

pub type Matrix = [[u8; 9]; 9];
