use super::args::parse_value;
use std::path::PathBuf;

use sudoku_solver::archive::{
    analyze_minimality, MinimalPuzzleArchive, Provenance, VerificationStatus,
};
use sudoku_solver::multi_clue::{
    search_n_remove_add, FrontierOperation, MultiClueFrontier, RemoveAddSearchOptions,
};

pub(super) struct Options {
    pub(super) archive_path: PathBuf,
    pub(super) checkpoint_path: PathBuf,
    pub(super) from_clues: Option<u8>,
    pub(super) to_clues: Option<u8>,
    pub(super) remove_count: Option<u8>,
    pub(super) add_count: Option<u8>,
    pub(super) max_sources: Option<usize>,
    pub(super) nodes: u64,
    pub(super) checks: u64,
}

pub(crate) fn run_cli(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next().ok_or_else(|| usage().to_owned())?;
    let mut archive_path = PathBuf::from("data/minimal-puzzle-archive.json");
    let mut checkpoint_path = PathBuf::from("data/multi-clue-frontier.json");
    let mut from_clues = None;
    let mut to_clues = None;
    let mut remove_count = None;
    let mut add_count = None;
    let mut max_sources = None;
    let mut nodes = 100_000u64;
    let mut checks = 10_000u64;

    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value after {option}\n{}", usage()))?;
        match option.as_str() {
            "--archive" => archive_path = PathBuf::from(value),
            "--checkpoint" => checkpoint_path = PathBuf::from(value),
            "--from" => from_clues = Some(parse_value(&value, "--from")?),
            "--to" => to_clues = Some(parse_value(&value, "--to")?),
            "--remove" => remove_count = Some(parse_value(&value, "--remove")?),
            "--add" => add_count = Some(parse_value(&value, "--add")?),
            "--max-sources" => max_sources = Some(parse_value(&value, "--max-sources")?),
            "--nodes" => nodes = parse_value(&value, "--nodes")?,
            "--checks" => checks = parse_value(&value, "--checks")?,
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
    }
    if command != "descend" {
        return Err(format!("unknown frontier command {command:?}\n{}", usage()));
    }
    if nodes == 0 || checks == 0 {
        return Err("--nodes and --checks must be non-zero".into());
    }
    if max_sources == Some(0) {
        return Err("--max-sources must be non-zero".into());
    }

    run(Options {
        archive_path,
        checkpoint_path,
        from_clues,
        to_clues,
        remove_count,
        add_count,
        max_sources,
        nodes,
        checks,
    })
}

pub(crate) fn usage() -> &'static str {
    "usage: sudoku frontier descend --from N --to M [--remove R --add S] [--archive PATH] [--checkpoint PATH] [--max-sources N] [--nodes N] [--checks N]"
}

pub(super) fn run(options: Options) -> Result<(), String> {
    let Options {
        archive_path,
        checkpoint_path,
        from_clues,
        to_clues,
        remove_count,
        add_count,
        max_sources,
        nodes,
        checks,
    } = options;

    let from = from_clues.ok_or_else(|| "missing --from".to_owned())?;
    let to = to_clues.ok_or_else(|| "missing --to".to_owned())?;
    let (remove_count, add_count) = match (remove_count, add_count) {
        (Some(remove), Some(add)) => (remove, add),
        (None, None) => (2, 1),
        _ => return Err("--remove and --add must be specified together".into()),
    };
    if remove_count == 0 || remove_count > from {
        return Err("--remove must be in 1..=--from".into());
    }
    let available_after_removal = 81 - from + remove_count;
    if add_count > available_after_removal {
        return Err("--add exceeds the number of empty cells after removal".into());
    }
    if from - remove_count + add_count != to || to >= from {
        return Err(
            "frontier descend requires --to = --from - --remove + --add and --to < --from".into(),
        );
    }
    let operation = if remove_count == 2 && add_count == 1 {
        FrontierOperation::TwoDropOneAdd
    } else {
        FrontierOperation::RemoveAdd {
            remove_count,
            add_count,
        }
    };
    let operation_id = operation.checkpoint_id();
    let mut archive =
        MinimalPuzzleArchive::load(&archive_path, nodes).map_err(|error| error.to_string())?;
    let mut frontier = if checkpoint_path.exists() {
        MultiClueFrontier::load_checkpoint(&checkpoint_path, nodes)?
    } else {
        MultiClueFrontier::new()
    };
    let source_records: Vec<_> = archive
        .records()
        .iter()
        .filter(|record| record.clue_count == from)
        .cloned()
        .collect();
    if source_records.is_empty() {
        return Err(format!("archive has no verified {from}-clue records"));
    }
    for record in source_records {
        frontier.register(record.as_analysis(), None, None)?;
    }

    let sources: Vec<_> = frontier
        .nodes()
        .iter()
        .filter(|node| {
            node.analysis.clue_count == from
                && node.analysis.status == VerificationStatus::VerifiedMinimal
                && !node.explored_operations.contains(&operation)
        })
        .take(max_sources.unwrap_or(usize::MAX))
        .map(|node| {
            (
                node.id.clone(),
                node.analysis.clone(),
                node.explored_operations.clone(),
            )
        })
        .collect();
    let mut completed_sources = 0usize;
    let mut incomplete_sources = 0usize;
    let mut inserted_minimal = 0usize;
    let mut verified_unique = 0usize;
    let mut total_checks = 0u64;
    let mut total_proof_nodes = 0u64;
    let mut total_deletion_sets = 0u64;
    let mut total_add_sets = 0u64;
    let sources_selected = sources.len();
    for (source_id, analysis, explored) in sources {
        if explored.contains(&operation) {
            continue;
        }
        let report = search_n_remove_add(
            &analysis,
            remove_count,
            add_count,
            RemoveAddSearchOptions {
                max_nodes_per_candidate: nodes,
                max_candidate_checks: checks,
            },
        )?;
        frontier.attempts = frontier.attempts.saturating_add(1);
        total_checks = total_checks.saturating_add(report.stats.candidate_checks);
        total_proof_nodes = total_proof_nodes.saturating_add(report.stats.proof_nodes);
        total_deletion_sets = total_deletion_sets.saturating_add(report.stats.deletion_sets);
        total_add_sets = total_add_sets.saturating_add(report.stats.generated_add_sets);
        let complete = report.stats.complete;
        for candidate in report.candidates {
            let child = analyze_minimality(candidate.puzzle, candidate.solution, nodes)
                .map_err(|error| error.to_string())?;
            if !matches!(
                child.status,
                VerificationStatus::VerifiedMinimal | VerificationStatus::VerifiedNonMinimal
            ) {
                continue;
            }
            verified_unique += 1;
            let is_minimal = child.status == VerificationStatus::VerifiedMinimal;
            if is_minimal {
                let inserted = archive
                    .insert_analysis(
                        &child,
                        Provenance {
                            source: "minimal-puzzle-archive".into(),
                            algorithm: format!("multi-clue-remove-{remove_count}-add-{add_count}"),
                            seed: None,
                            parent_id: Some(source_id.clone()),
                            operation: Some(operation_id.clone()),
                            search_depth: 1,
                        },
                        nodes,
                    )
                    .map_err(|error| error.to_string())?;
                if inserted {
                    inserted_minimal += 1;
                }
            }
            frontier.register(child, Some(source_id.clone()), Some(operation))?;
        }
        if complete {
            frontier.mark_explored(&source_id, operation);
            completed_sources += 1;
        } else {
            incomplete_sources += 1;
        }
        archive
            .save_atomic(&archive_path)
            .map_err(|error| error.to_string())?;
        frontier.save_checkpoint(&checkpoint_path)?;
    }
    println!(
                "from={from} to={to} remove={remove_count} add={add_count} sources_selected={sources_selected} sources_completed={completed_sources} sources_incomplete={incomplete_sources} unique_children={verified_unique} new_minimal={inserted_minimal} deletion_sets={total_deletion_sets} add_sets={total_add_sets} candidate_checks={total_checks} proof_nodes={total_proof_nodes} checkpoint={}",
                checkpoint_path.display()
            );

    Ok(())
}
