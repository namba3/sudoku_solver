use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::args::parse_value;
use sudoku_solver::archive::{
    analyze_minimality_from_puzzle, MinimalPuzzleArchive, Provenance, VerificationStatus,
};
use sudoku_solver::generator::{generate_diverse, DiverseBudget, DiverseStrategy};
use sudoku_solver::Matrix;

#[derive(Clone)]
struct Options {
    archive: PathBuf,
    seed: u64,
    seconds: u64,
    nodes: u64,
    checks: u64,
    threads: usize,
    from: u8,
    to: u8,
}

struct Candidate {
    puzzle: Matrix,
    solution: Matrix,
    starting_clues: u8,
    seed: u64,
    search_depth: u32,
}

pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let options = parse_args(args)?;
    let mut archive = if options.archive.exists() {
        MinimalPuzzleArchive::load(&options.archive, options.nodes)
            .map_err(|error| format!("archive verification failed: {error}"))?
    } else {
        MinimalPuzzleArchive::new()
    };

    let worker_count = options.threads;
    let search_started = Instant::now();
    let deadline = search_started + Duration::from_secs(options.seconds);
    let stopped = Arc::new(AtomicBool::new(false));
    let attempts: Arc<[AtomicU64; 82]> = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
    let (sender, receiver) = mpsc::channel::<Candidate>();

    thread::scope(|scope| {
        for worker_id in 0..worker_count {
            let worker_sender = sender.clone();
            let worker_stopped = Arc::clone(&stopped);
            let worker_attempts = Arc::clone(&attempts);
            let worker_options = options.clone();
            scope.spawn(move || {
                worker(
                    worker_id,
                    worker_count,
                    worker_options,
                    deadline,
                    worker_stopped,
                    worker_attempts,
                    worker_sender,
                );
            });
        }
        drop(sender);

        let mut inserted_by_clue = [0usize; 82];
        let mut candidates_verified = 0usize;
        loop {
            match receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(candidate) => {
                    candidates_verified += 1;
                    let clue_count = candidate
                        .puzzle
                        .iter()
                        .flatten()
                        .filter(|&&value| (1..=9).contains(&value))
                        .count() as u8;
                    if !(options.from..=options.to).contains(&clue_count) {
                        continue;
                    }
                    match archive.import_puzzle(
                        candidate.puzzle,
                        candidate.solution,
                        Provenance {
                            source: "sudoku scan".into(),
                            algorithm: "random-completion-greedy-minimal-descent".into(),
                            seed: Some(candidate.seed),
                            parent_id: None,
                            operation: Some(format!("start-{}-clues", candidate.starting_clues)),
                            search_depth: candidate.search_depth,
                        },
                        options.nodes,
                    ) {
                        Ok((analysis, true)) => {
                            if analysis.status == VerificationStatus::VerifiedMinimal {
                                inserted_by_clue[analysis.clue_count as usize] += 1;
                                if let Err(error) = archive.save_atomic(&options.archive) {
                                    stopped.store(true, Ordering::Relaxed);
                                    eprintln!("archive save failed: {error}");
                                }
                            }
                        }
                        Ok((analysis, false)) => {
                            if analysis.status != VerificationStatus::VerifiedMinimal {
                                eprintln!("archive rejected non-minimal candidate");
                            }
                        }
                        Err(error) => eprintln!("candidate import failed: {error}"),
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        stopped.store(true, Ordering::Relaxed);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let mut clues = options.from;
        while clues <= options.to {
            println!(
                "clues={clues} attempts={} new_archive_records={}",
                attempts[clues as usize].load(Ordering::Relaxed),
                inserted_by_clue[clues as usize]
            );
            clues += 1;
        }
        println!(
            "candidate_minima_verified={candidates_verified} archive_total={} workers={} elapsed_seconds={:.3} archive={}",
            archive.records().len(),
            worker_count,
            search_started.elapsed().as_secs_f64(),
            options.archive.display()
        );
    });

    Ok(())
}

fn worker(
    worker_id: usize,
    worker_count: usize,
    options: Options,
    deadline: Instant,
    stopped: Arc<AtomicBool>,
    attempts: Arc<[AtomicU64; 82]>,
    sender: mpsc::Sender<Candidate>,
) {
    let mut seed = options.seed.wrapping_add(worker_id as u64);
    let clue_count_range = usize::from(options.to - options.from + 1);
    let mut clue_offset = worker_id % clue_count_range;
    let budget = DiverseBudget {
        max_attempts: 1,
        max_nodes_per_uniqueness_check: options.nodes,
        max_hitting_set_nodes: options.checks,
        max_neighborhood_checks: options.checks,
        max_intermediate_clues: 80,
        strategy: DiverseStrategy::Adaptive,
    };

    while !stopped.load(Ordering::Relaxed) && Instant::now() < deadline {
        while !stopped.load(Ordering::Relaxed) && Instant::now() < deadline {
            let starting_clues = options.from + clue_offset as u8;
            clue_offset = (clue_offset + 1) % clue_count_range;
            let attempt_seed = seed;
            seed = seed.wrapping_add(worker_count as u64);
            attempts[starting_clues as usize].fetch_add(1, Ordering::Relaxed);
            let Ok(puzzle) = generate_diverse(attempt_seed, starting_clues as usize, budget) else {
                continue;
            };
            if Instant::now() >= deadline {
                return;
            }
            let Some((analysis, search_depth)) = descend_to_minimal(
                puzzle,
                starting_clues,
                attempt_seed,
                options.nodes,
                deadline,
            ) else {
                continue;
            };
            if analysis.status == VerificationStatus::VerifiedMinimal
                && (options.from..=options.to).contains(&analysis.clue_count)
            {
                if sender
                    .send(Candidate {
                        puzzle: analysis.puzzle,
                        solution: analysis.solution,
                        starting_clues,
                        seed: attempt_seed,
                        search_depth,
                    })
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}

fn descend_to_minimal(
    mut puzzle: Matrix,
    starting_clues: u8,
    seed: u64,
    max_nodes_per_check: u64,
    deadline: Instant,
) -> Option<(sudoku_solver::archive::MinimalPuzzleAnalysis, u32)> {
    let mut depth = 0u32;
    let mut branch_seed = seed;
    loop {
        if Instant::now() >= deadline {
            return None;
        }
        let analysis = analyze_minimality_from_puzzle(puzzle, max_nodes_per_check).ok()?;
        match analysis.status {
            VerificationStatus::VerifiedMinimal => return Some((analysis, depth)),
            VerificationStatus::VerifiedNonMinimal => {
                if analysis.direct_reductions.is_empty()
                    || depth >= u32::from(starting_clues.saturating_sub(17))
                {
                    return None;
                }
                branch_seed = splitmix64(branch_seed);
                let index = branch_seed as usize % analysis.direct_reductions.len();
                puzzle = analysis.direct_reductions[index];
                depth += 1;
            }
            VerificationStatus::BudgetExceeded
            | VerificationStatus::Cancelled
            | VerificationStatus::InvalidPuzzle => return None,
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^ (mixed >> 31)
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut archive = PathBuf::from("data/minimal-puzzle-archive.json");
    let mut seed = 20261160u64;
    let mut seconds = 120u64;
    let mut nodes = 1_000_000u64;
    let mut checks = 1_088u64;
    let mut threads = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .div_ceil(2)
        .max(1);
    let mut from = 17u8;
    let mut to = 20u8;
    let mut args = args;
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}\n{}", usage()))?;
        match option.as_str() {
            "--archive" => archive = PathBuf::from(value),
            "--seed" => seed = parse_value(&value, "--seed")?,
            "--seconds" => seconds = parse_value(&value, "--seconds")?,
            "--nodes" => nodes = parse_value(&value, "--nodes")?,
            "--checks" => checks = parse_value(&value, "--checks")?,
            "--threads" => threads = parse_value(&value, "--threads")?,
            "--from" => from = parse_value(&value, "--from")?,
            "--to" => to = parse_value(&value, "--to")?,
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
    }
    if seconds == 0 || nodes == 0 || checks == 0 || threads == 0 {
        return Err("seconds, nodes, checks, and threads must be positive".into());
    }
    if from < 17 || from > to || to > 80 {
        return Err("clue range must satisfy 17 <= from <= to <= 80".into());
    }
    Ok(Options {
        archive,
        seed,
        seconds,
        nodes,
        checks,
        threads,
        from,
        to,
    })
}

pub(super) fn usage() -> String {
    "usage: sudoku scan [--archive PATH] [--from N] [--to N] [--seed N] [--seconds N] [--nodes N] [--checks N] [--threads N]\n       Runs clue-count workers in parallel, greedily descends unique Diverse puzzles to minimal puzzles, and serially merges verified results into the archive.".into()
}
