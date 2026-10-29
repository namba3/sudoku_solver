use std::hint::black_box;
use std::time::Instant;

use sudoku_solver::solver::backend::SolverBackend;
use sudoku_solver::solver::{self, SearchEvent};
use sudoku_solver::Matrix;

const EASY: Matrix = [
    [5, 3, 0, 0, 7, 0, 0, 0, 0],
    [6, 0, 0, 1, 9, 5, 0, 0, 0],
    [0, 9, 8, 0, 0, 0, 0, 6, 0],
    [8, 0, 0, 0, 6, 0, 0, 0, 3],
    [4, 0, 0, 8, 0, 3, 0, 0, 1],
    [7, 0, 0, 0, 2, 0, 0, 0, 6],
    [0, 6, 0, 0, 0, 0, 2, 8, 0],
    [0, 0, 0, 4, 1, 9, 0, 0, 5],
    [0, 0, 0, 0, 8, 0, 0, 7, 9],
];

const TWO_SOLUTION: Matrix = [
    [5, 3, 4, 6, 7, 8, 9, 1, 2],
    [6, 7, 2, 1, 9, 5, 3, 4, 8],
    [1, 9, 8, 3, 4, 2, 5, 6, 7],
    [8, 5, 9, 7, 6, 0, 4, 2, 0],
    [4, 2, 6, 8, 5, 0, 7, 9, 0],
    [7, 1, 3, 9, 2, 4, 8, 5, 6],
    [9, 6, 1, 5, 3, 7, 2, 8, 4],
    [2, 8, 7, 4, 1, 9, 6, 3, 5],
    [3, 4, 5, 2, 8, 6, 1, 7, 9],
];

fn main() {
    println!("Sudoku solver backend benchmark (release mode, black_box)");
    println!(
        "Each timing row reports the median of 7 samples; search node counts are per operation."
    );
    for (name, board) in [("easy-unique", EASY), ("two-solution", TWO_SOLUTION)] {
        benchmark::<solver::naive::Backend>("naive", name, &board);
        benchmark::<solver::backend::Optimized>("optimized", name, &board);
        benchmark::<solver::optimized_u64::Backend>("optimized-u64", name, &board);
        #[cfg(all(
            not(target_family = "wasm"),
            any(target_arch = "x86", target_arch = "x86_64")
        ))]
        benchmark::<solver::optimized_simd::Backend>("optimized-simd", name, &board);
    }
    let (puzzle17, solution) = sudoku_solver::generator::verified_17_clue_base(411);
    for (name, puzzle) in [
        ("minimal-17", puzzle17),
        ("nonminimal-18", add_one_clue(puzzle17, &solution)),
    ] {
        benchmark_minimality::<sudoku_solver::archive::minimality::Naive>(
            "naive", name, puzzle, solution,
        );
        benchmark_minimality::<sudoku_solver::archive::minimality::Optimized>(
            "optimized",
            name,
            puzzle,
            solution,
        );
        benchmark_minimality::<sudoku_solver::archive::minimality::OptimizedU64>(
            "packed-u64",
            name,
            puzzle,
            solution,
        );
        #[cfg(all(
            not(target_family = "wasm"),
            any(target_arch = "x86", target_arch = "x86_64")
        ))]
        benchmark_minimality::<sudoku_solver::archive::minimality::OptimizedSimd>(
            "optimized-simd",
            name,
            puzzle,
            solution,
        );
    }
    benchmark_minimal_search::<sudoku_solver::generator::minimal_search::Naive>("naive");
    benchmark_minimal_search::<sudoku_solver::generator::minimal_search::Optimized>("optimized");
}

fn benchmark_minimal_search<
    B: sudoku_solver::generator::minimal_search::MinimalClueSearchBackend,
>(
    backend: &str,
) {
    use sudoku_solver::generator::minimal_search::{search_with, MinimalSearchBudget};

    let budget = MinimalSearchBudget {
        candidate_attempts: 1,
        max_nodes_per_check: 1_000,
        max_hitting_set_nodes: 5_000,
        max_neighborhood_checks: 128,
        max_intermediate_clues: 18,
    };
    let start = Instant::now();
    let mut successes = 0;
    let mut nodes = 0;
    for seed in 411..414 {
        match search_with::<B>(black_box(seed), black_box(18), black_box(budget)) {
            Ok(result) => {
                successes += 1;
                nodes += result.evidence.total_nodes;
                black_box(result);
            }
            Err(sudoku_solver::generator::DiverseGenerationError::BudgetExhausted {
                uniqueness_nodes,
                ..
            }) => nodes += uniqueness_nodes,
            Err(_) => {}
        }
    }
    println!(
        "{backend:9} minimal-search-18 elapsed={:>8.2?}/3 seeds successes={successes}/3 nodes(total)={nodes}",
        start.elapsed()
    );
}

fn add_one_clue(mut puzzle: Matrix, solution: &Matrix) -> Matrix {
    let position = puzzle
        .iter()
        .flatten()
        .position(|&value| value == 0)
        .expect("a 17-clue puzzle has an empty cell");
    puzzle[position / 9][position % 9] = solution[position / 9][position % 9];
    puzzle
}

fn benchmark<B: SolverBackend>(backend: &str, puzzle_name: &str, puzzle: &Matrix) {
    for operation in ["first", "up-to-two", "solve"] {
        let mut durations = Vec::with_capacity(7);
        let mut nodes = 0;
        for _ in 0..7 {
            let start = Instant::now();
            for _ in 0..100 {
                nodes = black_box(run::<B>(operation, black_box(puzzle)));
            }
            durations.push(start.elapsed() / 100);
        }
        durations.sort_unstable();
        if operation == "solve" {
            println!(
                "{backend:9} {puzzle_name:14} {operation:9} median={:>8.2?}/op",
                durations[3]
            );
        } else {
            println!(
                "{backend:9} {puzzle_name:14} {operation:9} median={:>8.2?}/op nodes={nodes}",
                durations[3]
            );
        }
    }
}

fn benchmark_minimality<B: sudoku_solver::archive::minimality::MinimalityBackend>(
    backend: &str,
    puzzle_name: &str,
    puzzle: Matrix,
    solution: Matrix,
) {
    let mut durations = Vec::with_capacity(7);
    let mut nodes = 0;
    for _ in 0..7 {
        let start = Instant::now();
        for _ in 0..3 {
            let result = sudoku_solver::archive::minimality::analyze_with::<B>(
                black_box(puzzle),
                black_box(solution),
                1_000_000,
            )
            .expect("benchmark puzzle has a valid completion");
            nodes = result.evidence.total_nodes;
            black_box(result);
        }
        durations.push(start.elapsed() / 3);
    }
    durations.sort_unstable();
    println!(
        "{backend:9} {puzzle_name:14} minimality median={:>8.2?}/op nodes={nodes}",
        durations[3]
    );
}

fn run<B: SolverBackend>(operation: &str, puzzle: &Matrix) -> u64 {
    match operation {
        "first" => {
            let mut search = B::search(puzzle).expect("benchmark puzzle is valid");
            for event in &mut search {
                if matches!(event, SearchEvent::SolutionFound { .. }) {
                    break;
                }
            }
            B::explored_nodes(&search)
        }
        "up-to-two" => {
            let mut search = B::search(puzzle).expect("benchmark puzzle is valid");
            while B::solutions_found(&search) < 2 {
                if search.next().is_none() {
                    break;
                }
            }
            B::explored_nodes(&search)
        }
        "solve" => {
            let mut board = *puzzle;
            black_box(B::solve(&mut board));
            black_box(board);
            0
        }
        _ => unreachable!(),
    }
}
