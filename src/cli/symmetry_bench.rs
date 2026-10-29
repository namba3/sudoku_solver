//! Reproducible local baseline for Sudoku canonicalization paths.
//!
//! Run with `cargo run --release --bin sudoku -- bench symmetry --iterations 5`.

use super::args::parse_value;
use std::hint::black_box;
use std::time::Instant;
use sudoku_solver::{
    generator::generate_puzzle_with_clues,
    solve,
    symmetry::{
        canonicalize_puzzle, canonicalize_solution, check_equivalent_pairwise, EquivalenceResult,
        SpatialTransform,
    },
};

pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut iterations = 3usize;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--iterations" => {
                iterations = parse_value(
                    &args.next().ok_or("--iterations requires a value")?,
                    "--iterations",
                )?;
                if iterations == 0 {
                    return Err("--iterations must be greater than zero".into());
                }
            }
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(());
            }
            _ => return Err(format!("unknown option {argument:?}")),
        }
    }
    let seed = 2026;
    let puzzle = generate_puzzle_with_clues(seed, 17).expect("17 clue generation");
    let mut solution = puzzle;
    assert!(solve(&mut solution), "generated puzzle must be solvable");
    let mut transformed = SpatialTransform {
        rows: [3, 4, 5, 0, 1, 2, 6, 7, 8],
        cols: [1, 2, 0, 4, 5, 3, 7, 8, 6],
        transpose: true,
    }
    .apply(&puzzle);
    for value in transformed.iter_mut().flatten() {
        if (1..=9).contains(value) {
            *value = 10 - *value;
        }
    }

    println!("seed={seed} clue_count=17 iterations={iterations}");
    let start = Instant::now();
    let mut puzzle_stats = Default::default();
    for _ in 0..iterations {
        let result = canonicalize_puzzle(black_box(&puzzle));
        puzzle_stats = result.stats;
        black_box(result.key);
    }
    report("puzzle_minlex", start, iterations);
    println!(
        "  transforms={} pattern_prefix_prunes={} min_pattern_transforms={}",
        puzzle_stats.transforms_considered,
        puzzle_stats.pattern_prefix_prunes,
        puzzle_stats.minimum_pattern_transforms
    );

    let start = Instant::now();
    let mut solution_stats = Default::default();
    for _ in 0..iterations {
        let result = canonicalize_solution(black_box(&puzzle), black_box(&solution), 1_000_000)
            .expect("generated 17 clue puzzle must be unique within budget");
        solution_stats = result.stats;
        black_box(result.key);
    }
    report("solution_minlex", start, iterations);
    println!(
        "  transforms={} solution_prefix_prunes={} uniqueness_nodes={}",
        solution_stats.transforms_considered,
        solution_stats.solution_prefix_prunes,
        solution_stats.uniqueness_nodes
    );

    let start = Instant::now();
    let mut pairwise_stats = Default::default();
    for _ in 0..iterations {
        match check_equivalent_pairwise(black_box(&puzzle), black_box(&transformed), u64::MAX) {
            EquivalenceResult::Equivalent { stats, .. } => pairwise_stats = stats,
            other => panic!("equivalent benchmark pair returned {other:?}"),
        }
    }
    report("pairwise_equivalent", start, iterations);
    println!(
        "  transforms={} invariant_rejections={}",
        pairwise_stats.transforms_considered, pairwise_stats.invariant_rejections
    );

    let dense = generate_puzzle_with_clues(seed, 30).expect("30 clue generation");
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(canonicalize_puzzle(black_box(&dense)));
    }
    report("puzzle_minlex_30_clues", start, iterations);
    Ok(())
}

pub(super) fn usage() -> &'static str {
    "usage: sudoku bench symmetry [--iterations N]"
}

fn report(name: &str, start: Instant, iterations: usize) {
    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{name}: total_ms={:.3} mean_ms={:.3}",
        total_ms,
        total_ms / iterations as f64
    );
}
