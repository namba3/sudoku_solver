use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::pool_args::{parse_args, Options};
use sudoku_solver::generator::{DiverseBudget, DiverseStrategy, MAX_INTERMEDIATE_CLUE_COUNT};
use sudoku_solver::pivot::{PivotCache, PivotCandidateMode, PivotOptions, PivotSearchReport};
use sudoku_solver::symmetry::{canonical_key, canonicalize_solution, CanonicalKey};
use sudoku_solver::{check_uniqueness, Matrix, UniquenessStatus};

#[derive(Clone)]
struct Record {
    id: String,
    puzzle: String,
    solution: String,
    symmetry_class: String,
    solution_symmetry_class: String,
    seed: u64,
    uniqueness_nodes: u64,
    parent_id: Option<String>,
    strategy: String,
    search_depth: u32,
    neighborhood_checks: u64,
}

pub(crate) fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut options = parse_args(args)?;
    if options.target == 0 || options.attempts == 0 || options.nodes == 0 {
        return Err("target, attempts, and nodes must be greater than zero".into());
    }
    let mut records = Vec::new();
    let mut classes = HashSet::new();
    let mut solution_classes = HashSet::new();
    let mut seed_cursor = 0u64;
    let mut candidate_runs = 0u64;
    let mut total_nodes = 0u64;
    let mut total_hitting_nodes = 0u64;
    let mut prior_elapsed_seconds = 0.0;
    if options.resume && options.output.exists() {
        let checkpoint: serde_json::Value =
            serde_json::from_slice(&fs::read(&options.output).map_err(|error| error.to_string())?)
                .map_err(|error| format!("invalid checkpoint JSON: {error}"))?;
        if checkpoint["format_version"].as_u64() != Some(7) {
            return Err("unsupported checkpoint format_version".into());
        }
        if checkpoint["generation"]["seed"].as_u64() != Some(options.seed) {
            return Err("resume seed does not match the checkpoint seed".into());
        }
        if checkpoint["generation"]["strategy"].as_str() != Some(options.strategy.as_str()) {
            return Err("resume strategy does not match the checkpoint strategy".into());
        }
        (seed_cursor, candidate_runs) = checkpoint_run_counters(&checkpoint["generation"])?;
        total_nodes = checkpoint["generation"]["uniqueness_nodes"]
            .as_u64()
            .ok_or_else(|| "checkpoint is missing uniqueness_nodes".to_owned())?;
        total_hitting_nodes = checkpoint["generation"]["search_candidate_checks"]
            .as_u64()
            .ok_or_else(|| "checkpoint is missing search_candidate_checks".to_owned())?;
        prior_elapsed_seconds = checkpoint["generation"]["elapsed_seconds"]
            .as_f64()
            .ok_or_else(|| "checkpoint is missing elapsed_seconds".to_owned())?;
        if !options.max_intermediate_clues_explicit {
            options.max_intermediate_clues = checkpoint["generation"]["max_intermediate_clues"]
                .as_u64()
                .ok_or_else(|| "checkpoint is missing max_intermediate_clues".to_owned())?
                as usize;
        }
        let saved = checkpoint["records"]
            .as_array()
            .ok_or_else(|| "checkpoint records must be an array".to_string())?;
        let mut record_ids = HashSet::new();
        let mut record_depths = HashMap::new();
        for value in saved {
            let mut record = parse_record(value)?;
            match record.parent_id.as_deref() {
                Some(parent_id) => {
                    let parent_depth = record_depths.get(parent_id).ok_or_else(|| {
                        format!("{} references an unknown or later parent", record.id)
                    })?;
                    if record.search_depth != parent_depth + 1 {
                        return Err(format!("{} has an inconsistent search depth", record.id));
                    }
                }
                None if record.search_depth == 0 => {}
                None => return Err(format!("{} is missing its parent ID", record.id)),
            }
            if !matches!(
                record.strategy.as_str(),
                "adaptive"
                    | "swap-1+1"
                    | "swap+1-1"
                    | "swap-2+2"
                    | "swap-3+3"
                    | "via-18"
                    | "via-intermediate"
                    | "drift-walk"
                    | "solution-space-pivot"
                    | "random-17-witness"
            ) {
                return Err(format!("{} has an unknown search strategy", record.id));
            }
            validate_record(&mut record, options.nodes)?;
            if !record_ids.insert(record.id.clone()) {
                return Err(format!("duplicate record ID in checkpoint: {}", record.id));
            }
            if !classes.insert(record.symmetry_class.clone()) {
                return Err(format!(
                    "duplicate symmetry class in checkpoint: {}",
                    record.id
                ));
            }
            if !solution_classes.insert(record.solution_symmetry_class.clone()) {
                return Err(format!(
                    "duplicate solution symmetry class in checkpoint: {}",
                    record.id
                ));
            }
            record_depths.insert(record.id.clone(), record.search_depth);
            records.push(record);
        }
        if records.len() > options.target {
            return Err("checkpoint contains more records than the requested target".into());
        }
    } else if options.output.exists() {
        return Err("output exists; pass --resume to validate and continue it".into());
    }

    if !(18..=MAX_INTERMEDIATE_CLUE_COUNT).contains(&options.max_intermediate_clues) {
        return Err(format!(
            "max intermediate clues must be between 18 and {MAX_INTERMEDIATE_CLUE_COUNT}"
        ));
    }
    let budget = DiverseBudget {
        max_attempts: options.attempts,
        max_nodes_per_uniqueness_check: options.nodes,
        max_hitting_set_nodes: options.nodes,
        max_neighborhood_checks: options.neighborhood_checks,
        max_intermediate_clues: options.max_intermediate_clues,
        strategy: options.strategy,
    };

    let started = Instant::now();
    let mut pivot_cache = PivotCache::default();
    if records.len() == options.target {
        println!(
            "already complete: {} records, {} candidate runs, {} uniqueness nodes",
            records.len(),
            candidate_runs,
            total_nodes
        );
        return Ok(());
    }

    while records.len() < options.target
        && candidate_runs < options.max_candidates
        && options
            .pivot_total_seconds
            .map_or(true, |limit| started.elapsed().as_secs_f64() < limit)
    {
        let seed = options.seed.wrapping_add(seed_cursor);
        let mut last_nodes = 0u64;
        let mut last_hitting_nodes = 0u64;
        let mut last_neighborhood_checks = 0u64;
        let mut last_strategy = budget.strategy;
        let mut parent_id = None;
        let mut search_depth = 0u32;
        let on_progress = |progress: sudoku_solver::generator::DiverseProgress| {
            last_nodes = progress.uniqueness_nodes;
            last_strategy = progress.strategy;
            last_neighborhood_checks = progress.neighborhood_checks;
            last_hitting_nodes = progress
                .hitting_set_nodes
                .saturating_add(progress.neighborhood_checks);
        };
        let generated = if budget.strategy == DiverseStrategy::Random17Witness {
            // This strategy starts from its own random completed grid rather
            // than evolving one of the checkpoint records.
            sudoku_solver::generator::generate_diverse_with_progress(seed, 17, budget, on_progress)
        } else if budget.strategy == DiverseStrategy::SolutionSpacePivot {
            let (parent, solution) = if records.is_empty() {
                if options.parent_id.is_some() {
                    return Err("--parent-id requires a checkpoint with records".into());
                }
                let (parent, solution) = sudoku_solver::generator::verified_17_clue_base(seed);
                (parent, solution)
            } else {
                let parent_index = if let Some(parent_id) = options.parent_id.as_deref() {
                    records
                        .iter()
                        .position(|record| record.id == parent_id)
                        .ok_or_else(|| format!("unknown pivot parent ID: {parent_id}"))?
                } else {
                    (seed_cursor as usize) % records.len()
                };
                let selected_parent = &records[parent_index];
                parent_id = Some(selected_parent.id.clone());
                search_depth = selected_parent.search_depth.saturating_add(1);
                (
                    decode(&selected_parent.puzzle)?,
                    decode(&selected_parent.solution)?,
                )
            };
            let remaining_seconds = options
                .pivot_total_seconds
                .map(|limit| (limit - started.elapsed().as_secs_f64()).max(0.0));
            let pivot_options = PivotOptions {
                mode: options.pivot_mode,
                candidate_mode: PivotCandidateMode::Direct,
                threads: options.pivot_threads,
                max_solutions: options.pivot_max_solutions,
                max_nodes_per_search: options.nodes,
                max_nodes_per_candidate_check: options.nodes,
                max_candidate_checks: options.neighborhood_checks,
                seed,
                prioritize_different_solution: options.pivot_prioritize_solution_change,
                max_duration: remaining_seconds.map(std::time::Duration::from_secs_f64),
            };
            let cancelled = std::sync::atomic::AtomicBool::new(false);
            eprintln!("Solution-Space Pivot: verifying parent for seed {seed}");
            let mut last_pivot_progress = Instant::now();
            let pivot_report =
                sudoku_solver::pivot::search_solution_space_pivots_with_progress_verified_parent(
                    seed,
                    parent,
                    solution,
                    pivot_options,
                    &cancelled,
                    &mut pivot_cache,
                    |progress| {
                        last_strategy = DiverseStrategy::SolutionSpacePivot;
                        last_nodes = progress.explored_nodes;
                        last_neighborhood_checks = progress.candidate_checks;
                        last_hitting_nodes = progress.candidate_checks;
                        if last_pivot_progress.elapsed().as_secs_f64() >= 1.0 {
                            eprintln!(
                            "Solution-Space Pivot: removed=({}, {}) phase={:?} nodes={} checks={}",
                            progress.removed_position / 9 + 1,
                            progress.removed_position % 9 + 1,
                            progress.phase,
                            progress.explored_nodes,
                            progress.candidate_checks
                        );
                            last_pivot_progress = Instant::now();
                        }
                    },
                )
                .map_err(|error| format!("Solution-Space Pivot rejected its request: {error:?}"))?;
            last_strategy = DiverseStrategy::SolutionSpacePivot;
            last_nodes = pivot_report.explored_nodes;
            last_neighborhood_checks = pivot_report.candidate_checks;
            last_hitting_nodes = pivot_report.candidate_checks;
            let stats_path = options
                .pivot_stats_output
                .clone()
                .unwrap_or_else(|| default_pivot_stats_path(&options.output));
            append_pivot_stats(
                &stats_path,
                seed,
                parent_id.as_deref(),
                pivot_options,
                &pivot_report,
            )?;

            let mut selected = None;
            for candidate in &pivot_report.candidates {
                let candidate_class = canonical_key(&candidate.puzzle);
                let candidate_solution_class =
                    canonical_solution_key(&candidate.puzzle, &candidate.solution, options.nodes)?;
                let puzzle_duplicate = classes.contains(&candidate_class);
                let solution_duplicate = solution_classes.contains(&candidate_solution_class);
                if puzzle_duplicate != solution_duplicate {
                    return Err(
                        "Puzzle MinLex and Solution MinLex disagree on a pivot duplicate".into(),
                    );
                }
                if !puzzle_duplicate {
                    selected = Some(candidate.puzzle);
                    break;
                }
            }
            match selected {
                Some(puzzle) => Ok(puzzle),
                None => {
                    eprintln!(
                        "Solution-Space Pivot produced no candidate: stop_reason={:?}, complete={}, candidate_checks={}",
                        pivot_report.stop_reason,
                        pivot_report.complete,
                        pivot_report.candidate_checks
                    );
                    Err(
                        sudoku_solver::generator::DiverseGenerationError::BudgetExhausted {
                            attempts: 1,
                            uniqueness_nodes: pivot_report.explored_nodes,
                            hitting_set_nodes: 0,
                            neighborhood_checks: pivot_report.candidate_checks,
                        },
                    )
                }
            }
        } else if records.is_empty() {
            sudoku_solver::generator::generate_diverse_with_progress(seed, 17, budget, on_progress)
        } else {
            let parent_index = if let Some(parent_id) = options.parent_id.as_deref() {
                records
                    .iter()
                    .position(|record| record.id == parent_id)
                    .ok_or_else(|| format!("unknown parent ID: {parent_id}"))?
            } else {
                (seed_cursor as usize) % records.len()
            };
            let selected_parent = &records[parent_index];
            parent_id = Some(selected_parent.id.clone());
            search_depth = selected_parent.search_depth.saturating_add(1);
            let parent = decode(&selected_parent.puzzle)?;
            let solution = decode(&selected_parent.solution)?;
            sudoku_solver::generator::generate_diverse_from_parent_with_progress(
                seed,
                parent,
                solution,
                budget,
                on_progress,
            )
        };
        if budget.strategy == DiverseStrategy::Random17Witness {
            let (unique_candidate, uniqueness_nodes, neighborhood_checks) = match &generated {
                Ok(_) => (true, last_nodes, last_neighborhood_checks),
                Err(sudoku_solver::generator::DiverseGenerationError::BudgetExhausted {
                    uniqueness_nodes,
                    neighborhood_checks,
                    ..
                }) => (false, *uniqueness_nodes, *neighborhood_checks),
                Err(error) => {
                    return Err(format!("random-17-witness rejected its request: {error:?}"))
                }
            };
            let stats_path = options
                .pivot_stats_output
                .clone()
                .unwrap_or_else(|| default_random_witness_stats_path(&options.output));
            append_random_witness_stats(
                &stats_path,
                seed,
                unique_candidate,
                uniqueness_nodes,
                neighborhood_checks,
            )?;
        }
        match &generated {
            Ok(_) => {}
            Err(error) => match error {
                sudoku_solver::generator::DiverseGenerationError::BudgetExhausted {
                    uniqueness_nodes,
                    hitting_set_nodes,
                    neighborhood_checks,
                    ..
                } => {
                    last_nodes = *uniqueness_nodes;
                    last_neighborhood_checks = *neighborhood_checks;
                    last_strategy = budget.strategy;
                    last_hitting_nodes = hitting_set_nodes.saturating_add(*neighborhood_checks);
                }
                sudoku_solver::generator::DiverseGenerationError::InvalidParent => {
                    return Err("selected checkpoint parent failed validation".into())
                }
                _ => {
                    return Err(format!(
                        "Diverse generator rejected its fixed request: {error:?}"
                    ))
                }
            },
        }
        // `--max-candidates` counts complete generator runs (one seed each).
        // `--attempts` is the independent search budget within each run.
        let attempts_used = candidate_runs_consumed(&generated);
        let Ok(puzzle) = generated else {
            candidate_runs = candidate_runs.saturating_add(attempts_used);
            seed_cursor = seed_cursor.saturating_add(attempts_used);
            total_nodes = total_nodes.saturating_add(last_nodes);
            total_hitting_nodes = total_hitting_nodes.saturating_add(last_hitting_nodes);
            save_checkpoint(
                &options.output,
                options.seed,
                options.strategy,
                options.max_intermediate_clues,
                candidate_runs,
                seed_cursor,
                total_nodes,
                total_hitting_nodes,
                prior_elapsed_seconds + started.elapsed().as_secs_f64(),
                &records,
            )?;
            report(
                &options,
                &records,
                candidate_runs,
                total_nodes,
                total_hitting_nodes,
                started.elapsed().as_secs_f64(),
            );
            continue;
        };

        let uniqueness = check_uniqueness(&puzzle, options.nodes)
            .map_err(|error| format!("generated puzzle has conflicting givens: {error:?}"))?;
        candidate_runs = candidate_runs.saturating_add(attempts_used);
        seed_cursor = seed_cursor.saturating_add(attempts_used);
        let record_nodes = last_nodes.saturating_add(uniqueness.explored_nodes);
        total_nodes = total_nodes.saturating_add(last_nodes);
        total_hitting_nodes = total_hitting_nodes.saturating_add(last_hitting_nodes);
        total_nodes = total_nodes.saturating_add(uniqueness.explored_nodes);
        if uniqueness.status != UniquenessStatus::Unique {
            save_checkpoint(
                &options.output,
                options.seed,
                options.strategy,
                options.max_intermediate_clues,
                candidate_runs,
                seed_cursor,
                total_nodes,
                total_hitting_nodes,
                prior_elapsed_seconds + started.elapsed().as_secs_f64(),
                &records,
            )?;
            report(
                &options,
                &records,
                candidate_runs,
                total_nodes,
                total_hitting_nodes,
                started.elapsed().as_secs_f64(),
            );
            continue;
        }

        let mut solution = puzzle;
        if !sudoku_solver::solve(&mut solution) {
            return Err("generated puzzle could not be solved during record creation".into());
        }
        let class = canonical_key(&puzzle);
        let solution_class = canonical_solution_key(&puzzle, &solution, options.nodes)?;
        let puzzle_duplicate = classes.contains(&class);
        let solution_duplicate = solution_classes.contains(&solution_class);
        if puzzle_duplicate != solution_duplicate {
            return Err("Puzzle MinLex and Solution MinLex disagree on a duplicate".into());
        }
        if puzzle_duplicate {
            save_checkpoint(
                &options.output,
                options.seed,
                options.strategy,
                options.max_intermediate_clues,
                candidate_runs,
                seed_cursor,
                total_nodes,
                total_hitting_nodes,
                prior_elapsed_seconds + started.elapsed().as_secs_f64(),
                &records,
            )?;
            report(
                &options,
                &records,
                candidate_runs,
                total_nodes,
                total_hitting_nodes,
                started.elapsed().as_secs_f64(),
            );
            continue;
        }
        let record = Record {
            id: format!("diverse-{:04}", records.len() + 1),
            puzzle: encode(&puzzle),
            solution: encode(&solution),
            symmetry_class: class,
            solution_symmetry_class: solution_class,
            seed,
            uniqueness_nodes: record_nodes,
            parent_id,
            strategy: last_strategy.as_str().to_owned(),
            search_depth,
            neighborhood_checks: last_neighborhood_checks,
        };
        records.push(record);
        classes.insert(records.last().unwrap().symmetry_class.clone());
        solution_classes.insert(records.last().unwrap().solution_symmetry_class.clone());
        save_checkpoint(
            &options.output,
            options.seed,
            options.strategy,
            options.max_intermediate_clues,
            candidate_runs,
            seed_cursor,
            total_nodes,
            total_hitting_nodes,
            prior_elapsed_seconds + started.elapsed().as_secs_f64(),
            &records,
        )?;
        report(
            &options,
            &records,
            candidate_runs,
            total_nodes,
            total_hitting_nodes,
            started.elapsed().as_secs_f64(),
        );
    }
    if records.len() == options.target {
        Ok(())
    } else {
        Err(format!(
            "candidate budget exhausted: verified {} of {} requested records after {} candidate runs",
            records.len(),
            options.target,
            candidate_runs
        ))
    }
}

fn candidate_runs_consumed(
    generated: &Result<Matrix, sudoku_solver::generator::DiverseGenerationError>,
) -> u64 {
    match generated {
        Ok(_) | Err(sudoku_solver::generator::DiverseGenerationError::BudgetExhausted { .. }) => 1,
        Err(_) => 0,
    }
}

fn checkpoint_run_counters(generation: &serde_json::Value) -> Result<(u64, u64), String> {
    let seed_cursor = generation["seed_cursor"]
        .as_u64()
        .ok_or_else(|| "checkpoint is missing seed_cursor".to_owned())?;
    let candidate_runs = generation["candidate_runs"]
        .as_u64()
        .ok_or_else(|| "checkpoint is missing candidate_runs".to_owned())?;
    Ok((seed_cursor, candidate_runs))
}

#[cfg(test)]
mod tests {
    use super::candidate_runs_consumed;
    use sudoku_solver::generator::DiverseGenerationError;

    #[test]
    fn candidate_budget_counts_generator_runs_not_internal_attempts() {
        let successful_run = Ok([[0; 9]; 9]);
        let exhausted_run = Err(DiverseGenerationError::BudgetExhausted {
            attempts: 8,
            uniqueness_nodes: 100,
            hitting_set_nodes: 20,
            neighborhood_checks: 30,
        });
        let rejected_request = Err(DiverseGenerationError::InvalidParent);

        assert_eq!(candidate_runs_consumed(&successful_run), 1);
        assert_eq!(candidate_runs_consumed(&exhausted_run), 1);
        assert_eq!(candidate_runs_consumed(&rejected_request), 0);
    }

    #[test]
    fn current_checkpoint_requires_seed_cursor_and_candidate_runs() {
        assert_eq!(
            super::checkpoint_run_counters(&serde_json::json!({
                "seed_cursor": 40,
                "candidate_runs": 4,
            })),
            Ok((40, 4))
        );
        assert!(super::checkpoint_run_counters(&serde_json::json!({"attempts": 12})).is_err());
    }
}

fn parse_record(value: &serde_json::Value) -> Result<Record, String> {
    Ok(Record {
        id: value["id"]
            .as_str()
            .ok_or("record id is missing")?
            .to_owned(),
        puzzle: value["puzzle"]
            .as_str()
            .ok_or("record puzzle is missing")?
            .to_owned(),
        solution: value["solution"]
            .as_str()
            .ok_or("record solution is missing")?
            .to_owned(),
        symmetry_class: value["symmetry_class"]
            .as_str()
            .ok_or("record symmetry_class is missing")?
            .to_owned(),
        solution_symmetry_class: value["solution_symmetry_class"]
            .as_str()
            .ok_or("record solution_symmetry_class is missing")?
            .to_owned(),
        seed: value["seed"].as_u64().ok_or("record seed is missing")?,
        uniqueness_nodes: value["uniqueness_nodes"]
            .as_u64()
            .ok_or("record uniqueness_nodes is missing")?,
        parent_id: value["parent_id"].as_str().map(str::to_owned),
        strategy: value["strategy"]
            .as_str()
            .ok_or("record strategy is missing")?
            .to_owned(),
        search_depth: u32::try_from(
            value["search_depth"]
                .as_u64()
                .ok_or("record search_depth is missing")?,
        )
        .map_err(|_| "record search_depth exceeds u32")?,
        neighborhood_checks: value["neighborhood_checks"]
            .as_u64()
            .ok_or("record neighborhood_checks is missing")?,
    })
}

fn validate_record(record: &mut Record, max_nodes: u64) -> Result<(), String> {
    let puzzle = decode(&record.puzzle)?;
    let solution = decode(&record.solution)?;
    if puzzle.iter().flatten().filter(|&&value| value != 0).count() != 17 {
        return Err(format!("{} does not contain exactly 17 clues", record.id));
    }
    if !is_complete_solution(&solution) {
        return Err(format!("{} has an invalid completed solution", record.id));
    }
    for (given_row, solution_row) in puzzle.iter().zip(solution.iter()) {
        if given_row
            .iter()
            .zip(solution_row.iter())
            .any(|(&given, &value)| given != 0 && given != value)
        {
            return Err(format!(
                "{} has a clue that conflicts with its solution",
                record.id
            ));
        }
    }
    if canonical_key(&puzzle) != record.symmetry_class {
        return Err(format!("{} has an invalid symmetry class", record.id));
    }
    let computed_solution_class = canonical_solution_key(&puzzle, &solution, max_nodes)?;
    if computed_solution_class != record.solution_symmetry_class {
        return Err(format!(
            "{} has an invalid solution symmetry class",
            record.id
        ));
    }
    let uniqueness = check_uniqueness(&puzzle, max_nodes)
        .map_err(|error| format!("{} has conflicting clues: {error:?}", record.id))?;
    if uniqueness.status != UniquenessStatus::Unique {
        return Err(format!(
            "{} did not pass unique-solution verification",
            record.id
        ));
    }
    let solved = puzzle;
    let mut solved = solved;
    if !sudoku_solver::solve(&mut solved) || solved != solution {
        return Err(format!(
            "{} stored solution differs from solver output",
            record.id
        ));
    }
    Ok(())
}

fn is_complete_solution(board: &Matrix) -> bool {
    for y in 0..9 {
        let mut row = 0u16;
        let mut column = 0u16;
        for x in 0..9 {
            let (value, other) = (board[y][x], board[x][y]);
            if !(1..=9).contains(&value) || !(1..=9).contains(&other) {
                return false;
            }
            row |= 1 << (value - 1);
            column |= 1 << (other - 1);
        }
        if row != 0x01ff || column != 0x01ff {
            return false;
        }
    }
    for block_y in 0..3 {
        for block_x in 0..3 {
            let mut block = 0u16;
            for y in block_y * 3..block_y * 3 + 3 {
                for x in block_x * 3..block_x * 3 + 3 {
                    block |= 1 << (board[y][x] - 1);
                }
            }
            if block != 0x01ff {
                return false;
            }
        }
    }
    true
}

fn encode(board: &Matrix) -> String {
    board
        .iter()
        .flatten()
        .map(|value| char::from(b'0' + *value))
        .collect()
}

fn canonical_solution_key(
    puzzle: &Matrix,
    solution: &Matrix,
    max_nodes: u64,
) -> Result<String, String> {
    let result = canonicalize_solution(puzzle, solution, max_nodes)
        .map_err(|error| format!("Solution MinLex failed: {error:?}"))?;
    match result.key {
        CanonicalKey::Solution {
            canonical_solution,
            canonical_clues,
        } => {
            let digits: String = canonical_solution
                .into_iter()
                .map(|value| char::from(b'0' + value))
                .collect();
            Ok(format!("{digits}:{canonical_clues:021x}"))
        }
        CanonicalKey::Puzzle(_) => Err("Solution MinLex returned a puzzle key".into()),
    }
}

fn default_pivot_stats_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("sudoku-pool.json");
    let mut path = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    path.push(format!("{name}.pivot-stats.jsonl"));
    path
}

fn default_random_witness_stats_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("sudoku-pool.json");
    let mut path = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    path.push(format!("{name}.random-witness-stats.jsonl"));
    path
}

fn append_random_witness_stats(
    path: &Path,
    seed: u64,
    unique_candidate: bool,
    uniqueness_nodes: u64,
    neighborhood_checks: u64,
) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let document = serde_json::json!({
        "record_type": "attempt",
        "seed": seed,
        "strategy": DiverseStrategy::Random17Witness.as_str(),
        "clue_count": 17,
        "unique_candidate": unique_candidate,
        "uniqueness_nodes": uniqueness_nodes,
        "neighborhood_checks": neighborhood_checks,
    });
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &document).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_ref()
        .sync_data()
        .map_err(|error| error.to_string())
}

fn append_pivot_stats(
    path: &Path,
    seed: u64,
    parent_id: Option<&str>,
    options: PivotOptions,
    report: &PivotSearchReport,
) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let deletion_stats: Vec<_> = report
        .deletions
        .iter()
        .map(|deletion| {
            serde_json::json!({
                "removed_position": deletion.removed_position,
                "candidate_proof_nodes": deletion.candidate_proof_nodes,
                "elapsed_micros": deletion.elapsed_micros,
                "pivot_candidates": deletion.pivot_candidates,
                "candidate_budget": deletion.candidate_budget,
                "candidate_budget_exhausted": deletion.candidate_budget_exhausted,
                "multiple_candidates": deletion.multiple_candidates,
                "unknown_candidates": deletion.unknown_candidates,
                "no_solution_candidates": deletion.no_solution_candidates,
                "witness_pruned_candidates": deletion.witness_pruned_candidates,
                "unique_candidates": deletion.unique_candidates,
                "new_canonical_classes": deletion.new_canonical_classes,
            })
        })
        .collect();
    let document = serde_json::json!({
        "seed": seed,
        "parent_id": parent_id,
        "deletion_mode": format!("{:?}", report.mode),
        "threads": options.threads,
        "max_nodes_per_candidate_check": options.max_nodes_per_candidate_check,
        "max_candidate_checks": options.max_candidate_checks,
        "prefer_solution_change": options.prioritize_different_solution,
        "max_duration_millis": options.max_duration.map(|duration| duration.as_millis()),
        "parent_uniqueness_checked": report.parent_uniqueness_checked,
        "parent_verification_nodes": report.parent_verification_nodes,
        "complete": report.complete,
        "stop_reason": report.stop_reason.map(|reason| format!("{reason:?}")),
        "candidate_checks": report.candidate_checks,
        "explored_nodes": report.explored_nodes,
        "cache_hits": report.cache_hits,
        "candidate_count": report.candidates.len(),
        "different_solution_count": report.candidates.iter().filter(|candidate| candidate.different_solution).count(),
        "different_solution_class_count": report.candidates.iter().filter(|candidate| candidate.different_solution_class).count(),
        "deletions": deletion_stats,
    });
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &document).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;
    writer
        .get_ref()
        .sync_data()
        .map_err(|error| error.to_string())
}

fn decode(encoded: &str) -> Result<Matrix, String> {
    if encoded.len() != 81 || !encoded.bytes().all(|value| (b'0'..=b'9').contains(&value)) {
        return Err("board must be exactly 81 ASCII digits from 0 through 9".into());
    }
    let values: Vec<u8> = encoded.bytes().map(|value| value - b'0').collect();
    Ok(std::array::from_fn(|y| {
        std::array::from_fn(|x| values[y * 9 + x])
    }))
}

fn save_checkpoint(
    path: &Path,
    seed: u64,
    strategy: DiverseStrategy,
    max_intermediate_clues: usize,
    candidate_runs: u64,
    seed_cursor: u64,
    uniqueness_nodes: u64,
    hitting_set_nodes: u64,
    elapsed_seconds: f64,
    records: &[Record],
) -> Result<(), String> {
    let json_records: Vec<_> = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id,
                "puzzle": record.puzzle,
                "solution": record.solution,
                "symmetry_class": record.symmetry_class,
                "solution_symmetry_class": record.solution_symmetry_class,
                "seed": record.seed,
                "uniqueness_nodes": record.uniqueness_nodes,
                "parent_id": record.parent_id,
                "strategy": record.strategy,
                "search_depth": record.search_depth,
                "neighborhood_checks": record.neighborhood_checks,
                "provenance": "Diverse",
            })
        })
        .collect();
    let document = serde_json::json!({
        "format_version": 7,
        "generation": {
            "seed": seed,
            "candidate_runs": candidate_runs,
            "seed_cursor": seed_cursor,
            "uniqueness_nodes": uniqueness_nodes,
            "search_candidate_checks": hitting_set_nodes,
            "strategy": strategy.as_str(),
            "max_intermediate_clues": max_intermediate_clues,
            "elapsed_seconds": elapsed_seconds,
        },
        "records": json_records,
    });
    super::atomic_file::write_json(path, &document, true)
}

fn report(
    options: &Options,
    records: &[Record],
    candidate_runs: u64,
    nodes: u64,
    hitting_nodes: u64,
    seconds: f64,
) {
    eprintln!(
        "records={}/{} candidate_runs={} uniqueness_nodes={} candidate_checks={} elapsed_seconds={:.3} output={}",
        records.len(),
        options.target,
        candidate_runs,
        nodes,
        hitting_nodes,
        seconds,
        options.output.display()
    );
}
