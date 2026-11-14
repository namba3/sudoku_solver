use std::path::PathBuf;
use sudoku_solver::generator::DiverseStrategy;
use sudoku_solver::pivot::PivotMode;

pub(super) struct Options {
    pub(super) target: usize,
    pub(super) seed: u64,
    pub(super) attempts: u32,
    pub(super) nodes: u64,
    pub(super) max_candidates: u64,
    pub(super) neighborhood_checks: u64,
    pub(super) max_intermediate_clues: usize,
    pub(super) max_intermediate_clues_explicit: bool,
    pub(super) strategy: DiverseStrategy,
    pub(super) pivot_mode: PivotMode,
    pub(super) pivot_threads: usize,
    pub(super) pivot_max_solutions: u64,
    pub(super) pivot_total_seconds: Option<f64>,
    pub(super) pivot_prioritize_solution_change: bool,
    pub(super) pivot_stats_output: Option<PathBuf>,
    pub(super) parent_id: Option<String>,
    pub(super) output: PathBuf,
    pub(super) resume: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            target: 100,
            seed: 1,
            attempts: 8,
            nodes: 100_000,
            max_candidates: 10_000,
            neighborhood_checks: 1_088,
            max_intermediate_clues: 18,
            max_intermediate_clues_explicit: false,
            strategy: DiverseStrategy::Adaptive,
            pivot_mode: PivotMode::Random,
            pivot_threads: sudoku_solver::pivot::PivotOptions::default().threads,
            pivot_max_solutions: 256,
            pivot_total_seconds: None,
            pivot_prioritize_solution_change: true,
            pivot_stats_output: None,
            parent_id: None,
            output: PathBuf::from("data/17-clue-pool.json"),
            resume: false,
        }
    }
}

impl Options {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.target == 0 || self.attempts == 0 || self.nodes == 0 {
            return Err("target, attempts, and nodes must be greater than zero".into());
        }
        if self.pivot_threads == 0 {
            return Err("--threads must be greater than zero".into());
        }
        if let Some(seconds) = self.pivot_total_seconds {
            if !seconds.is_finite() || seconds <= 0.0 || seconds > 315_360_000.0 {
                return Err("--seconds must be between 0 and 315360000".into());
            }
        }
        Ok(())
    }
}
