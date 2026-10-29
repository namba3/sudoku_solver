use std::collections::HashSet;
use std::path::PathBuf;

use sudoku_solver::archive::{
    analyze_minimality, MinimalPuzzleArchive, Provenance, VerificationStatus,
};
use sudoku_solver::multi_clue::{search_n_remove_add, RemoveAddSearchOptions};
use sudoku_solver::Matrix;

const COLLECT_CHECKPOINT_STRATEGY: &str = "cegar-witness-intersection-same-n-v1";
const COLLECT_TWO_FOR_TWO_CHECKPOINT_STRATEGY: &str = "cegar-witness-intersection-same-n-2x2-v1";
const COLLECT_THREE_FOR_THREE_CHECKPOINT_STRATEGY: &str =
    "cegar-witness-intersection-same-n-3x3-v1";

pub(crate) fn parse_board(encoded: &str) -> Result<Matrix, String> {
    if encoded.len() != 81 || !encoded.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("--puzzle must be 81 ASCII digits (0 means empty)".into());
    }
    let mut puzzle = [[0; 9]; 9];
    for (position, byte) in encoded.bytes().enumerate() {
        puzzle[position / 9][position % 9] = byte - b'0';
    }
    Ok(puzzle)
}

pub(crate) fn collect_minimal(
    archive_path: PathBuf,
    checkpoint_path: PathBuf,
    source_clues: u8,
    target_clues: u8,
    remove_count: u8,
    add_count: u8,
    target_count: usize,
    attempt_limit: u64,
    initial_seed: u64,
    max_nodes_per_check: u64,
    max_candidate_checks: u64,
) -> Result<(), String> {
    let checkpoint_strategy = match (source_clues, target_clues, remove_count, add_count) {
        (source, target, 1, 1) if source == target => COLLECT_CHECKPOINT_STRATEGY.to_owned(),
        (source, target, 2, 2) if source == target => {
            COLLECT_TWO_FOR_TWO_CHECKPOINT_STRATEGY.to_owned()
        }
        (source, target, 3, 3) if source == target => {
            COLLECT_THREE_FOR_THREE_CHECKPOINT_STRATEGY.to_owned()
        }
        (source, target, remove, add) => {
            format!("cegar-witness-intersection-remove-add-{source}-{target}-{remove}-{add}-v1")
        }
    };
    let algorithm =
        format!("cegar-witness-intersection-remove-{remove_count}-add-{add_count}-collector");
    let mut archive = if archive_path.exists() {
        MinimalPuzzleArchive::load(&archive_path, max_nodes_per_check)
            .map_err(|error| error.to_string())?
    } else {
        MinimalPuzzleArchive::new()
    };
    if archive.count(source_clues) == 0 && target_count > 0 {
        return Err(format!(
            "archive has no {source_clues}-clue minimal parent; import or generate a verified seed first"
        ));
    }
    let (mut next_seed, mut total_attempts, mut completed_parent_keys) = load_collect_checkpoint(
        &checkpoint_path,
        source_clues,
        initial_seed,
        &checkpoint_strategy,
    )?;
    let initial_count = archive.count(target_clues);
    let mut attempts_run = 0u64;
    let mut minimal_found = 0u64;
    let mut duplicates = 0u64;
    let mut nonminimal = 0u64;
    let mut unknown = 0u64;
    let mut nonunique = 0u64;
    let mut multiple = 0u64;
    let mut proof_nodes = 0u64;
    let mut deletion_sets = 0u64;
    let mut add_sets = 0u64;

    while attempts_run < attempt_limit && archive.count(target_clues) < target_count {
        let available_parents: Vec<_> = archive
            .records()
            .iter()
            .filter(|record| {
                record.clue_count == source_clues
                    && !completed_parent_keys.contains(&record.puzzle_key)
            })
            .cloned()
            .collect();
        if available_parents.is_empty() {
            break;
        }
        let attempt_seed = next_seed;
        next_seed = next_seed.wrapping_add(1);
        total_attempts = total_attempts.saturating_add(1);
        attempts_run = attempts_run.saturating_add(1);

        let parent_index =
            (mix_seed(attempt_seed ^ 0xa076_1d64_78bd_642f) as usize) % available_parents.len();
        let parent = &available_parents[parent_index];
        let options = RemoveAddSearchOptions {
            max_nodes_per_candidate: max_nodes_per_check,
            max_candidate_checks,
        };
        let report = search_n_remove_add(&parent.as_analysis(), remove_count, add_count, options)?;
        proof_nodes = proof_nodes.saturating_add(report.stats.proof_nodes);
        multiple = multiple.saturating_add(report.stats.multiple_candidates);
        unknown = unknown.saturating_add(report.stats.unknown_candidates);
        deletion_sets = deletion_sets.saturating_add(report.stats.deletion_sets);
        add_sets = add_sets.saturating_add(report.stats.generated_add_sets);
        let candidates = report
            .candidates
            .into_iter()
            .map(|candidate| {
                let removed = candidate
                    .removed_positions
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join("-");
                let added = candidate
                    .added_positions
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join("-");
                (
                    candidate.puzzle,
                    candidate.solution,
                    format!("remove-{removed}-add-{added}"),
                )
            })
            .collect::<Vec<_>>();
        let complete = report.stats.complete;
        let mut retry_with_more_nodes = !complete;
        for (candidate_puzzle, candidate_solution, operation) in candidates {
            let analysis =
                analyze_minimality(candidate_puzzle, candidate_solution, max_nodes_per_check)
                    .map_err(|error| error.to_string())?;
            proof_nodes = proof_nodes.saturating_add(analysis.evidence.total_nodes);
            match analysis.status {
                VerificationStatus::VerifiedMinimal => {
                    if archive.find(target_clues, &analysis.puzzle_key).is_some() {
                        duplicates = duplicates.saturating_add(1);
                    } else if archive
                        .insert_analysis(
                            &analysis,
                            Provenance {
                                source: "minimal-puzzle-archive".into(),
                                algorithm: algorithm.clone(),
                                seed: Some(attempt_seed),
                                parent_id: Some(parent.puzzle_key.clone()),
                                operation: Some(operation),
                                search_depth: 1,
                            },
                            max_nodes_per_check,
                        )
                        .map_err(|error| error.to_string())?
                    {
                        minimal_found = minimal_found.saturating_add(1);
                        archive
                            .save_atomic(&archive_path)
                            .map_err(|error| error.to_string())?;
                    }
                }
                VerificationStatus::VerifiedNonMinimal => nonminimal = nonminimal.saturating_add(1),
                VerificationStatus::BudgetExceeded | VerificationStatus::Cancelled => {
                    unknown = unknown.saturating_add(1);
                    retry_with_more_nodes = true;
                }
                VerificationStatus::InvalidPuzzle => nonunique = nonunique.saturating_add(1),
            }
            if archive.count(target_clues) >= target_count {
                break;
            }
        }
        if retry_with_more_nodes {
            next_seed = attempt_seed;
        } else {
            completed_parent_keys.insert(parent.puzzle_key.clone());
        }
        save_collect_checkpoint(
            &checkpoint_path,
            source_clues,
            next_seed,
            total_attempts,
            &completed_parent_keys,
            &checkpoint_strategy,
        )?;
        if retry_with_more_nodes {
            break;
        }
    }
    archive
        .save_atomic(&archive_path)
        .map_err(|error| error.to_string())?;
    println!(
        "from={source_clues} to={target_clues} remove={remove_count} add={add_count} initial={initial_count} final={} target_count={target_count} attempts_run={attempts_run} total_attempts={total_attempts} new_minimal={minimal_found} deletion_sets={deletion_sets} add_sets={add_sets} duplicates={duplicates} nonminimal={nonminimal} multiple={multiple} nonunique={nonunique} unknown={unknown} proof_nodes={proof_nodes} next_seed={next_seed} checkpoint={}",
        archive.count(target_clues),
        checkpoint_path.display()
    );
    Ok(())
}

fn load_collect_checkpoint(
    path: &PathBuf,
    clue_count: u8,
    initial_seed: u64,
    expected_strategy: &str,
) -> Result<(u64, u64, HashSet<String>), String> {
    if !path.exists() {
        return Ok((initial_seed, 0, HashSet::new()));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if value["format_version"].as_u64() != Some(1)
        || value["clue_count"].as_u64() != Some(u64::from(clue_count))
    {
        return Err("unsupported or mismatched minimal-collect checkpoint".into());
    }
    let next_seed = value["next_seed"]
        .as_u64()
        .ok_or_else(|| "checkpoint has no next_seed".to_owned())?;
    let attempts = value["attempts"]
        .as_u64()
        .ok_or_else(|| "checkpoint has no attempts".to_owned())?;
    let stored_strategy = value["strategy"]
        .as_str()
        .ok_or_else(|| "checkpoint has no strategy".to_owned())?;
    if stored_strategy != expected_strategy {
        return Err("minimal-collect checkpoint strategy mismatch".into());
    }
    let completed_parent_keys: HashSet<String> = value["completed_parent_keys"]
        .as_array()
        .ok_or_else(|| "checkpoint has no completed_parent_keys".to_owned())?
        .iter()
        .map(|key| {
            key.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "checkpoint contains an invalid parent key".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let mut sorted_parent_keys: Vec<_> = completed_parent_keys.iter().cloned().collect();
    sorted_parent_keys.sort_unstable();
    let payload = serde_json::json!({
        "clue_count": clue_count,
        "next_seed": next_seed,
        "attempts": attempts,
        "strategy": expected_strategy,
        "completed_parent_keys": sorted_parent_keys,
    });
    if value["state_checksum_fnv1a64"].as_str() != Some(checksum_json(&payload)?.as_str()) {
        return Err("minimal-collect checkpoint checksum mismatch".into());
    }
    Ok((next_seed, attempts, completed_parent_keys))
}

fn save_collect_checkpoint(
    path: &PathBuf,
    clue_count: u8,
    next_seed: u64,
    attempts: u64,
    completed_parent_keys: &HashSet<String>,
    strategy: &str,
) -> Result<(), String> {
    let mut sorted_parent_keys: Vec<_> = completed_parent_keys.iter().cloned().collect();
    sorted_parent_keys.sort_unstable();
    let payload = serde_json::json!({
        "clue_count": clue_count,
        "next_seed": next_seed,
        "attempts": attempts,
        "strategy": strategy,
        "completed_parent_keys": sorted_parent_keys,
    });
    let document = serde_json::json!({
        "format_version": 1,
        "clue_count": clue_count,
        "next_seed": next_seed,
        "attempts": attempts,
        "strategy": strategy,
        "completed_parent_keys": sorted_parent_keys,
        "state_checksum_fnv1a64": checksum_json(&payload)?,
    });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    super::atomic_file::write_bytes(path, &bytes)
}

fn checksum_json(value: &serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

fn mix_seed(seed: u64) -> u64 {
    let mut value = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_checkpoint_round_trips_and_detects_tampering() {
        let path = std::env::temp_dir().join(format!(
            "sudoku-collect-checkpoint-{}.json",
            std::process::id()
        ));
        let completed = HashSet::from(["known-parent-key".to_owned()]);
        save_collect_checkpoint(&path, 17, 1234, 56, &completed, COLLECT_CHECKPOINT_STRATEGY)
            .unwrap();
        assert_eq!(
            load_collect_checkpoint(&path, 17, 1, COLLECT_CHECKPOINT_STRATEGY).unwrap(),
            (1234, 56, completed)
        );

        let mut corrupted =
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap();
        corrupted["next_seed"] = serde_json::json!(1235);
        std::fs::write(&path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(load_collect_checkpoint(&path, 17, 1, COLLECT_CHECKPOINT_STRATEGY).is_err());
        let _ = std::fs::remove_file(path);
    }
}
