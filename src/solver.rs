//! Public solver API and interchangeable reference/optimized implementations.

pub mod backend;
pub mod naive;
pub mod optimized;
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(target_arch = "x86", target_arch = "x86_64")
    ),
    all(target_arch = "wasm32", feature = "wasm-simd")
))]
pub mod optimized_simd;
pub mod optimized_u64;

pub use backend::{
    SolverBackend, check_uniqueness_with, find_alternative_solution_with,
    find_alternative_solution_with_order as find_alternative_solution_with_backend_order,
};
pub use optimized::{
    AlternativeSearchOrder, AlternativeSolutionCheck, AlternativeSolutionStatus, Candidates, Hint,
    HintError, HintTechnique, SearchError, SearchEvent, SolutionSearch, UniquenessCheck,
    UniquenessStatus, candidates_for, check_uniqueness, find_alternative_solution,
    find_alternative_solution_with_order, find_hint, solve,
};

/// Backend used by the stable top-level solver API.
#[cfg(any(
    all(
        not(target_family = "wasm"),
        any(target_arch = "x86", target_arch = "x86_64")
    ),
    all(target_arch = "wasm32", feature = "wasm-simd")
))]
pub type DefaultBackend = optimized_simd::Backend;
#[cfg(not(any(
    all(
        not(target_family = "wasm"),
        any(target_arch = "x86", target_arch = "x86_64")
    ),
    all(target_arch = "wasm32", feature = "wasm-simd")
)))]
pub type DefaultBackend = optimized_u64::Backend;
