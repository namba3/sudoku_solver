mod archive_cmd;
mod atomic_file;
mod frontier;
mod inspect;
mod minimal;
mod minimal_cmd;
mod minimal_scan;
mod pool;
mod pool_args;
mod symmetry_bench;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use sudoku_solver::generator::DiverseStrategy;
use sudoku_solver::pivot::PivotMode;

#[derive(Debug, Parser)]
#[command(
    name = "sudoku",
    version,
    about = "Sudoku solver and minimal-puzzle search tools",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect and maintain the verified minimal-puzzle archive.
    Archive {
        #[command(subcommand)]
        command: ArchiveCommand,
    },
    /// Verify puzzles or collect minimal puzzles from a clue-count frontier.
    Minimal {
        #[command(subcommand)]
        command: MinimalCommand,
    },
    /// Descend a multi-clue frontier and add verified results to the archive.
    Frontier {
        #[command(subcommand)]
        command: FrontierCommand,
    },
    /// Generate diverse puzzles with configurable search strategies.
    Diverse(DiverseArgs),
    /// Search multiple clue counts in parallel and collect minimal puzzles.
    Scan(ScanArgs),
    /// Analyze solution counts, singles, and minimality for a puzzle.
    Analyze(inspect::InspectArgs),
    /// Run local performance benchmarks.
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ArchiveCommand {
    /// Print record counts grouped by clue count.
    Stats(ArchiveArgs),
    /// Fully re-verify every archive record.
    Verify(ArchiveArgs),
    /// Verify and merge another archive.
    Import(ArchiveArgs),
    /// Write the archive to another path.
    Export(ArchiveArgs),
    /// Add the bundled verified 17-clue bases to the archive.
    SeedFastBases(ArchiveArgs),
}

#[derive(Debug, Args, Default)]
struct ArchiveArgs {
    /// Path to the archive.
    #[arg(long)]
    archive: Option<PathBuf>,
    /// Source archive for import.
    #[arg(long)]
    source: Option<PathBuf>,
    /// Destination path for export.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Maximum solver nodes used during verification.
    #[arg(long)]
    nodes: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum MinimalCommand {
    /// Analyze a puzzle's uniqueness and minimality.
    Verify(MinimalVerifyArgs),
    /// Collect minimal puzzles from a source clue count to a target count.
    Collect(MinimalCollectArgs),
}

#[derive(Debug, Args)]
struct MinimalVerifyArgs {
    /// Puzzle as 81 digits, using 0 for empty cells.
    #[arg(long)]
    puzzle: Option<String>,
    /// Maximum solver nodes used during verification.
    #[arg(long)]
    nodes: Option<u64>,
    /// Maximum checks used during verification.
    #[arg(long)]
    checks: Option<u64>,
}

#[derive(Debug, Args)]
struct MinimalCollectArgs {
    /// Source puzzle clue count.
    #[arg(long)]
    clues: Option<u8>,
    /// Target number of puzzles to collect.
    #[arg(long)]
    target: Option<usize>,
    /// Target clue count; inferred from --clues, --remove, and --add if omitted.
    #[arg(long)]
    to: Option<u8>,
    /// Number of clues to remove from each source.
    #[arg(long)]
    remove: Option<u8>,
    /// Number of clues to add after removal.
    #[arg(long)]
    add: Option<u8>,
    /// Candidate attempts per source puzzle.
    #[arg(long)]
    attempts: Option<u64>,
    /// Random seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Checkpoint file path.
    #[arg(long)]
    checkpoint: Option<PathBuf>,
    /// Archive file path.
    #[arg(long)]
    archive: Option<PathBuf>,
    /// Maximum solver nodes per check.
    #[arg(long)]
    nodes: Option<u64>,
    /// Maximum checks per candidate.
    #[arg(long)]
    checks: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum FrontierCommand {
    /// Descend from one clue count to a lower clue count.
    Descend(FrontierArgs),
}

#[derive(Debug, Args)]
struct FrontierArgs {
    /// Starting clue count.
    #[arg(long)]
    from: Option<u8>,
    /// Target clue count.
    #[arg(long)]
    to: Option<u8>,
    /// Clues to remove at each step.
    #[arg(long)]
    remove: Option<u8>,
    /// Clues to add at each step.
    #[arg(long)]
    add: Option<u8>,
    /// Maximum number of source puzzles.
    #[arg(long)]
    max_sources: Option<usize>,
    /// Archive file path.
    #[arg(long)]
    archive: Option<PathBuf>,
    /// Checkpoint file path.
    #[arg(long)]
    checkpoint: Option<PathBuf>,
    /// Maximum solver nodes per check.
    #[arg(long)]
    nodes: Option<u64>,
    /// Maximum checks per source puzzle.
    #[arg(long)]
    checks: Option<u64>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum StrategyArg {
    Adaptive,
    #[value(name = "swap-1+1")]
    SwapOne,
    #[value(name = "swap+1-1")]
    PlusOneMinusOne,
    #[value(name = "swap-2+2")]
    SwapTwo,
    #[value(name = "swap-3+3")]
    SwapThree,
    #[value(name = "via-18")]
    ViaEighteen,
    #[value(name = "via-intermediate")]
    ViaIntermediate,
    #[value(name = "drift-walk")]
    DriftWalk,
    #[value(name = "solution-space-pivot")]
    SolutionSpacePivot,
    #[value(name = "random-17-witness")]
    Random17Witness,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PivotModeArg {
    Random,
    Exhaustive,
}

#[derive(Debug, Args)]
struct DiverseArgs {
    /// Target number of puzzles.
    #[arg(long)]
    target: Option<usize>,
    /// Random seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Maximum generation attempts.
    #[arg(long)]
    attempts: Option<u32>,
    /// Maximum solver nodes per candidate.
    #[arg(long)]
    nodes: Option<u64>,
    /// Maximum candidate puzzles to examine.
    #[arg(long)]
    max_candidates: Option<u64>,
    /// Maximum neighborhood checks.
    #[arg(long)]
    neighborhood_checks: Option<u64>,
    /// Maximum clue count for intermediate states.
    #[arg(long)]
    max_intermediate_clues: Option<usize>,
    /// Search strategy.
    #[arg(long, value_enum)]
    strategy: Option<StrategyArg>,
    /// Solution-space pivot enumeration mode.
    #[arg(long, value_enum)]
    pivot_mode: Option<PivotModeArg>,
    /// Number of pivot worker threads.
    #[arg(long)]
    threads: Option<usize>,
    /// Pivot search time budget in seconds.
    #[arg(long)]
    seconds: Option<f64>,
    /// Parent archive record ID.
    #[arg(long)]
    parent_id: Option<String>,
    /// Write pivot statistics to this path.
    #[arg(long)]
    stats_output: Option<PathBuf>,
    /// Prefer candidates with a different solution.
    #[arg(long, conflicts_with = "no_prefer_solution_change")]
    prefer_solution_change: bool,
    /// Do not prefer candidates with a different solution.
    #[arg(long, conflicts_with = "prefer_solution_change")]
    no_prefer_solution_change: bool,
    /// Output pool path.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Resume from the existing output checkpoint.
    #[arg(long)]
    resume: bool,
}

#[derive(Debug, Args)]
struct ScanArgs {
    /// Archive file path.
    #[arg(long)]
    archive: Option<PathBuf>,
    /// First clue count to search.
    #[arg(long)]
    from: Option<u8>,
    /// Last clue count to search.
    #[arg(long)]
    to: Option<u8>,
    /// Random seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Search time budget in seconds.
    #[arg(long)]
    seconds: Option<u64>,
    /// Maximum solver nodes per check.
    #[arg(long)]
    nodes: Option<u64>,
    /// Maximum checks per candidate.
    #[arg(long)]
    checks: Option<u64>,
    /// Number of clue-count workers.
    #[arg(long)]
    threads: Option<usize>,
}

#[derive(Debug, Subcommand)]
enum BenchCommand {
    /// Benchmark Sudoku symmetry canonicalization.
    Symmetry(BenchSymmetryArgs),
}

#[derive(Debug, Args)]
struct BenchSymmetryArgs {
    /// Number of iterations per benchmark.
    #[arg(long)]
    iterations: Option<usize>,
}

pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let cli = match Cli::try_parse_from(std::iter::once("sudoku".to_owned()).chain(args)) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };

    match cli
        .command
        .ok_or_else(|| "a command is required".to_owned())?
    {
        Command::Archive { command } => run_archive(command),
        Command::Minimal { command } => run_minimal(command),
        Command::Frontier { command } => run_frontier(command),
        Command::Diverse(args) => pool::run(diverse_options(args)),
        Command::Scan(args) => minimal_scan::run(scan_options(args)),
        Command::Analyze(args) => inspect::run(args),
        Command::Bench { command } => run_bench(command),
    }
}

fn run_archive(command: ArchiveCommand) -> Result<(), String> {
    let (name, args) = match command {
        ArchiveCommand::Stats(args) => ("stats", args),
        ArchiveCommand::Verify(args) => ("verify", args),
        ArchiveCommand::Import(args) => ("import", args),
        ArchiveCommand::Export(args) => ("export", args),
        ArchiveCommand::SeedFastBases(args) => ("seed-fast-bases", args),
    };
    archive_cmd::run(name, args)
}

fn run_minimal(command: MinimalCommand) -> Result<(), String> {
    minimal_cmd::run(command)
}

fn run_frontier(command: FrontierCommand) -> Result<(), String> {
    let FrontierCommand::Descend(args) = command;
    frontier::run(frontier::Options {
        archive_path: args
            .archive
            .unwrap_or_else(|| PathBuf::from("data/minimal-puzzle-archive.json")),
        checkpoint_path: args
            .checkpoint
            .unwrap_or_else(|| PathBuf::from("data/multi-clue-frontier.json")),
        from_clues: args.from,
        to_clues: args.to,
        remove_count: args.remove,
        add_count: args.add,
        max_sources: args.max_sources,
        nodes: args.nodes.unwrap_or(100_000),
        checks: args.checks.unwrap_or(10_000),
    })
}

fn run_bench(command: BenchCommand) -> Result<(), String> {
    let BenchCommand::Symmetry(args) = command;
    symmetry_bench::run(args.iterations)
}

fn diverse_options(args: DiverseArgs) -> pool_args::Options {
    pool_args::Options {
        target: args.target.unwrap_or(100),
        seed: args.seed.unwrap_or(1),
        attempts: args.attempts.unwrap_or(8),
        nodes: args.nodes.unwrap_or(100_000),
        max_candidates: args.max_candidates.unwrap_or(10_000),
        neighborhood_checks: args.neighborhood_checks.unwrap_or(1_088),
        max_intermediate_clues: args.max_intermediate_clues.unwrap_or(18),
        max_intermediate_clues_explicit: args.max_intermediate_clues.is_some(),
        strategy: args
            .strategy
            .map_or(DiverseStrategy::Adaptive, |strategy| match strategy {
                StrategyArg::Adaptive => DiverseStrategy::Adaptive,
                StrategyArg::SwapOne => DiverseStrategy::SwapOne,
                StrategyArg::PlusOneMinusOne => DiverseStrategy::PlusOneMinusOne,
                StrategyArg::SwapTwo => DiverseStrategy::SwapTwo,
                StrategyArg::SwapThree => DiverseStrategy::SwapThree,
                StrategyArg::ViaEighteen => DiverseStrategy::ViaEighteen,
                StrategyArg::ViaIntermediate => DiverseStrategy::ViaIntermediate,
                StrategyArg::DriftWalk => DiverseStrategy::DriftWalk,
                StrategyArg::SolutionSpacePivot => DiverseStrategy::SolutionSpacePivot,
                StrategyArg::Random17Witness => DiverseStrategy::Random17Witness,
            }),
        pivot_mode: args
            .pivot_mode
            .map_or(PivotMode::Random, |mode| match mode {
                PivotModeArg::Random => PivotMode::Random,
                PivotModeArg::Exhaustive => PivotMode::Exhaustive,
            }),
        pivot_threads: args
            .threads
            .unwrap_or_else(|| sudoku_solver::pivot::PivotOptions::default().threads),
        pivot_max_solutions: 256,
        pivot_total_seconds: args.seconds,
        pivot_prioritize_solution_change: !args.no_prefer_solution_change,
        pivot_stats_output: args.stats_output,
        parent_id: args.parent_id,
        output: args
            .output
            .unwrap_or_else(|| PathBuf::from("data/17-clue-pool.json")),
        resume: args.resume,
    }
}

fn scan_options(args: ScanArgs) -> minimal_scan::Options {
    let mut options = minimal_scan::Options::default();
    if let Some(archive) = args.archive {
        options.archive = archive;
    }
    options.from = args.from.unwrap_or(options.from);
    options.to = args.to.unwrap_or(options.to);
    options.seed = args.seed.unwrap_or(options.seed);
    options.seconds = args.seconds.unwrap_or(options.seconds);
    options.nodes = args.nodes.unwrap_or(options.nodes);
    options.checks = args.checks.unwrap_or(options.checks);
    options.threads = args.threads.unwrap_or(options.threads);
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_accepts_nested_commands_and_typed_options() {
        let cli = Cli::try_parse_from([
            "sudoku",
            "diverse",
            "--target",
            "17",
            "--strategy",
            "swap-2+2",
            "--threads",
            "4",
        ])
        .unwrap();

        let Command::Diverse(args) = cli.command.unwrap() else {
            panic!("expected diverse command");
        };
        assert_eq!(args.target, Some(17));
        assert!(matches!(args.strategy, Some(StrategyArg::SwapTwo)));
        assert_eq!(args.threads, Some(4));
    }

    #[test]
    fn clap_rejects_unknown_options_and_invalid_enum_values() {
        assert!(Cli::try_parse_from(["sudoku", "scan", "--unknown"]).is_err());
        assert!(
            Cli::try_parse_from(["sudoku", "diverse", "--strategy", "not-a-strategy"]).is_err()
        );
    }

    #[test]
    fn analyze_requires_exactly_one_puzzle_source() {
        assert!(Cli::try_parse_from(["sudoku", "analyze"]).is_err());
        assert!(
            Cli::try_parse_from([
                "sudoku",
                "analyze",
                "--puzzle",
                &"0".repeat(81),
                "--file",
                "sample_problem/minimal-23.txt"
            ])
            .is_err()
        );
        assert!(Cli::try_parse_from(["sudoku", "analyze", "--file", "puzzle.txt"]).is_ok());
    }
}
