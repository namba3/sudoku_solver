use super::args::{next_parsed, next_value};
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

pub(super) fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
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
    };
    let mut args = args;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--target" => options.target = next_parsed(&mut args, "--target")?,
            "--seed" => options.seed = next_parsed(&mut args, "--seed")?,
            "--attempts" => options.attempts = next_parsed(&mut args, "--attempts")?,
            "--nodes" => options.nodes = next_parsed(&mut args, "--nodes")?,
            "--max-candidates" => {
                options.max_candidates = next_parsed(&mut args, "--max-candidates")?
            }
            "--neighborhood-checks" => {
                options.neighborhood_checks = next_parsed(&mut args, "--neighborhood-checks")?
            }
            "--max-intermediate-clues" => {
                options.max_intermediate_clues =
                    next_parsed(&mut args, "--max-intermediate-clues")?;
                options.max_intermediate_clues_explicit = true;
            }
            "--strategy" => {
                options.strategy = match next_value(&mut args, "--strategy")?.as_str() {
                    "adaptive" => DiverseStrategy::Adaptive,
                    "swap-1+1" => DiverseStrategy::SwapOne,
                    "swap+1-1" => DiverseStrategy::PlusOneMinusOne,
                    "swap-2+2" => DiverseStrategy::SwapTwo,
                    "swap-3+3" => DiverseStrategy::SwapThree,
                    "via-18" => DiverseStrategy::ViaEighteen,
                    "via-intermediate" => DiverseStrategy::ViaIntermediate,
                    "drift-walk" => DiverseStrategy::DriftWalk,
                    "solution-space-pivot" => DiverseStrategy::SolutionSpacePivot,
                    "random-17-witness" => DiverseStrategy::Random17Witness,
                    _ => return Err("unknown strategy; use --help for the supported names".into()),
                }
            }
            "--pivot-mode" => {
                options.pivot_mode = match next_value(&mut args, "--pivot-mode")?.as_str() {
                    "random" => PivotMode::Random,
                    "exhaustive" => PivotMode::Exhaustive,
                    _ => return Err("pivot mode must be random or exhaustive".into()),
                }
            }
            "--threads" => {
                options.pivot_threads = next_parsed(&mut args, "--threads")?;
                if options.pivot_threads == 0 {
                    return Err("--threads must be greater than zero".into());
                }
            }
            "--seconds" => {
                let seconds: f64 = next_parsed(&mut args, "--seconds")?;
                if !seconds.is_finite() || seconds <= 0.0 || seconds > 315_360_000.0 {
                    return Err("--seconds must be between 0 and 315360000".into());
                }
                options.pivot_total_seconds = Some(seconds);
            }
            "--prefer-solution-change" => options.pivot_prioritize_solution_change = true,
            "--no-prefer-solution-change" => options.pivot_prioritize_solution_change = false,
            "--parent-id" => options.parent_id = Some(next_value(&mut args, "--parent-id")?),
            "--stats-output" => {
                options.pivot_stats_output =
                    Some(PathBuf::from(next_value(&mut args, "--stats-output")?))
            }
            "--output" => options.output = PathBuf::from(next_value(&mut args, "--output")?),
            "--resume" => options.resume = true,
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(options);
            }
            _ => return Err(format!("unknown argument {argument:?}; use --help")),
        }
    }
    Ok(options)
}

pub(super) fn usage() -> String {
    "usage: sudoku diverse [--target N] [--seed N] [--attempts N] [--nodes N] [--max-candidates N] [--neighborhood-checks N] [--max-intermediate-clues N] [--strategy adaptive|swap-1+1|swap+1-1|swap-2+2|swap-3+3|via-18|via-intermediate|drift-walk|solution-space-pivot|random-17-witness] [--pivot-mode random|exhaustive] [--threads N] [--seconds N] [--parent-id ID] [--stats-output PATH] [--prefer-solution-change|--no-prefer-solution-change] [--output PATH] [--resume]".into()
}
