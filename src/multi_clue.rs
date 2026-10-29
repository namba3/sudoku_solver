//! Shared N-clue frontier operations and scheduling models.

use crate::archive::{
    analyze_minimality, validate_minimality_evidence, MinimalPuzzleAnalysis, VerificationStatus,
};
use crate::solver::{find_alternative_solution, AlternativeSolutionStatus};
use crate::symmetry::canonical_key;
use crate::Matrix;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
static CHECKPOINT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub const FRONTIER_CHECKPOINT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FrontierOperation {
    SameCountMutation,
    AddOne,
    RemoveOne,
    TwoDropOneAdd,
    RemoveAdd { remove_count: u8, add_count: u8 },
    IntermediateBridge,
    SolutionSpacePivot,
}

impl FrontierOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SameCountMutation => "n-to-n-mutation",
            Self::AddOne => "n-to-n-plus-one",
            Self::RemoveOne => "n-to-n-minus-one",
            Self::TwoDropOneAdd => "n-to-n-minus-two-plus-one",
            Self::RemoveAdd { .. } => "n-to-n-remove-add",
            Self::IntermediateBridge => "n-to-n-plus-one-to-n",
            Self::SolutionSpacePivot => "solution-space-pivot",
        }
    }

    pub fn checkpoint_id(self) -> String {
        match self {
            Self::RemoveAdd {
                remove_count,
                add_count,
            } => format!("remove-add-{remove_count}-{add_count}"),
            operation => operation.as_str().to_owned(),
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "n-to-n-mutation" => Some(Self::SameCountMutation),
            "n-to-n-plus-one" => Some(Self::AddOne),
            "n-to-n-minus-one" => Some(Self::RemoveOne),
            "n-to-n-minus-two-plus-one" => Some(Self::TwoDropOneAdd),
            "n-to-n-plus-one-to-n" => Some(Self::IntermediateBridge),
            "solution-space-pivot" => Some(Self::SolutionSpacePivot),
            _ => {
                let value = value.strip_prefix("remove-add-")?;
                let (remove_count, add_count) = value.split_once('-')?;
                Some(Self::RemoveAdd {
                    remove_count: remove_count.parse().ok()?,
                    add_count: add_count.parse().ok()?,
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SameCountMutation {
    pub puzzle: Matrix,
    pub removed_position: u8,
    pub added_position: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoveAddSearchOptions {
    pub max_nodes_per_candidate: u64,
    pub max_candidate_checks: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveAddCandidate {
    pub puzzle: Matrix,
    pub solution: Matrix,
    pub clue_count: u8,
    pub puzzle_key: String,
    pub solution_class: String,
    pub removed_positions: Vec<u8>,
    pub added_positions: Vec<u8>,
    pub proof_nodes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoveAddSearchStats {
    pub deletion_sets: u64,
    pub generated_add_sets: u64,
    pub candidate_checks: u64,
    pub proof_nodes: u64,
    pub multiple_candidates: u64,
    pub unknown_candidates: u64,
    pub unique_candidates: u64,
    pub duplicate_classes: u64,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveAddSearchReport {
    pub candidates: Vec<RemoveAddCandidate>,
    pub stats: RemoveAddSearchStats,
}

/// Search the generalized `-remove_count + add_count` neighborhood.
/// Added clues always agree with the source completion. Deletion witnesses
/// constrain added-position sets, and each found counterexample further
/// prunes unexamined sets for that deletion core.
pub fn search_n_remove_add(
    analysis: &MinimalPuzzleAnalysis,
    remove_count: u8,
    add_count: u8,
    options: RemoveAddSearchOptions,
) -> Result<RemoveAddSearchReport, String> {
    if options.max_nodes_per_candidate == 0 || options.max_candidate_checks == 0 {
        return Err("search budgets must be non-zero".into());
    }
    if remove_count == 0
        || remove_count > analysis.clue_count
        || add_count > 81 - analysis.clue_count + remove_count
    {
        return Err("remove/add counts are outside the source puzzle bounds".into());
    }
    if analysis.status != VerificationStatus::VerifiedMinimal
        || !analysis.evidence.input_unique
        || !validate_minimality_evidence(&analysis.puzzle, &analysis.solution, &analysis.evidence)
    {
        return Err("source analysis is not a verified minimal puzzle".into());
    }
    if find_alternative_solution(
        &analysis.puzzle,
        &analysis.solution,
        options.max_nodes_per_candidate,
    )
    .status
        != AlternativeSolutionStatus::Unique
    {
        return Err("source uniqueness proof could not be repeated".into());
    }

    let result_clue_count = analysis.clue_count - remove_count + add_count;
    if result_clue_count == 0 {
        return Err("remove/add counts produce an empty puzzle".into());
    }
    let clue_positions = analysis
        .evidence
        .deletions
        .iter()
        .fold(0u128, |mask, witness| mask | (1u128 << witness.position));
    let deletion_sets = collect_position_combinations(
        clue_positions,
        usize::from(remove_count),
        &[],
        options.max_candidate_checks as usize,
    );
    let mut report = RemoveAddSearchReport {
        candidates: Vec::new(),
        stats: RemoveAddSearchStats {
            complete: !deletion_sets.truncated,
            ..RemoveAddSearchStats::default()
        },
    };
    let mut seen_raw = HashSet::new();
    let mut seen_classes = HashSet::new();

    'deletions: for removed_positions in deletion_sets.combinations {
        if report.stats.candidate_checks >= options.max_candidate_checks {
            report.stats.complete = false;
            break;
        }
        report.stats.deletion_sets = report.stats.deletion_sets.saturating_add(1);
        let removed_mask = removed_positions
            .iter()
            .fold(0u128, |mask, &position| mask | (1u128 << position));
        let mut core = analysis.puzzle;
        for &position in &removed_positions {
            core[position as usize / 9][position as usize % 9] = 0;
        }
        let witness_masks: Vec<_> = analysis
            .evidence
            .deletions
            .iter()
            .filter(|witness| removed_mask & (1u128 << witness.position) != 0)
            .map(|witness| witness.difference_mask)
            .collect();
        if witness_masks.len() != usize::from(remove_count) {
            return Err("source minimality evidence is missing a deletion witness".into());
        }

        let addition_sets = collect_position_combinations(
            empty_mask(&analysis.puzzle),
            usize::from(add_count),
            &witness_masks,
            options.max_candidate_checks as usize,
        );
        report.stats.complete &= !addition_sets.truncated;
        report.stats.generated_add_sets = report
            .stats
            .generated_add_sets
            .saturating_add(addition_sets.combinations.len() as u64);
        let mut candidates = addition_sets.combinations;
        let mut index = 0;
        while index < candidates.len() {
            if report.stats.candidate_checks >= options.max_candidate_checks {
                report.stats.complete = false;
                break 'deletions;
            }
            let added_positions = candidates[index].clone();
            index += 1;
            let mut candidate = core;
            for &position in &added_positions {
                let position = position as usize;
                candidate[position / 9][position % 9] =
                    analysis.solution[position / 9][position % 9];
            }
            if !seen_raw.insert(encode(&candidate)) {
                continue;
            }
            report.stats.candidate_checks = report.stats.candidate_checks.saturating_add(1);
            let check = find_alternative_solution(
                &candidate,
                &analysis.solution,
                options.max_nodes_per_candidate,
            );
            report.stats.proof_nodes = report
                .stats
                .proof_nodes
                .saturating_add(check.explored_nodes);
            match check.status {
                AlternativeSolutionStatus::Unique => {
                    let puzzle_key = canonical_key(&candidate);
                    if !seen_classes.insert(puzzle_key.clone()) {
                        report.stats.duplicate_classes =
                            report.stats.duplicate_classes.saturating_add(1);
                        continue;
                    }
                    report.stats.unique_candidates =
                        report.stats.unique_candidates.saturating_add(1);
                    report.candidates.push(RemoveAddCandidate {
                        puzzle: candidate,
                        solution: analysis.solution,
                        clue_count: result_clue_count,
                        puzzle_key,
                        solution_class: canonical_key(&analysis.solution),
                        removed_positions: removed_positions.clone(),
                        added_positions,
                        proof_nodes: check.explored_nodes,
                    });
                }
                AlternativeSolutionStatus::AlternativeFound => {
                    report.stats.multiple_candidates =
                        report.stats.multiple_candidates.saturating_add(1);
                    let Some(alternative) = check.alternative else {
                        report.stats.complete = false;
                        continue;
                    };
                    let difference = difference_mask(&analysis.solution, &alternative);
                    let mut pending = candidates[index..].to_vec();
                    pending.retain(|set| {
                        set.iter()
                            .any(|&position| difference & (1u128 << position) != 0)
                    });
                    candidates.truncate(index);
                    candidates.extend(pending);
                }
                AlternativeSolutionStatus::Unknown => {
                    report.stats.unknown_candidates =
                        report.stats.unknown_candidates.saturating_add(1);
                    report.stats.complete = false;
                }
                AlternativeSolutionStatus::InvalidInput => {
                    return Err("candidate did not preserve the known completion".into())
                }
            }
        }
    }
    Ok(report)
}

struct PositionCombinations {
    combinations: Vec<Vec<u8>>,
    truncated: bool,
}

fn collect_position_combinations(
    available: u128,
    count: usize,
    required_masks: &[u128],
    limit: usize,
) -> PositionCombinations {
    fn visit(
        available: u128,
        start: usize,
        remaining: usize,
        chosen_mask: u128,
        chosen: &mut Vec<u8>,
        required_masks: &[u128],
        limit: usize,
        output: &mut Vec<Vec<u8>>,
        truncated: &mut bool,
    ) {
        if output.len() >= limit {
            *truncated = true;
            return;
        }
        if remaining == 0 {
            if required_masks
                .iter()
                .all(|required| required & chosen_mask != 0)
            {
                output.push(chosen.clone());
            }
            return;
        }

        let suffix = if start >= 81 {
            0
        } else {
            available & (u128::MAX << start)
        };
        if suffix.count_ones() < remaining as u32
            || required_masks
                .iter()
                .any(|required| required & (chosen_mask | suffix) == 0)
        {
            return;
        }

        let mut positions = suffix;
        while positions != 0 {
            if output.len() >= limit {
                *truncated = true;
                return;
            }
            let position = positions.trailing_zeros() as usize;
            positions &= positions - 1;
            chosen.push(position as u8);
            visit(
                available,
                position + 1,
                remaining - 1,
                chosen_mask | (1u128 << position),
                chosen,
                required_masks,
                limit,
                output,
                truncated,
            );
            chosen.pop();
        }
    }

    let mut result = PositionCombinations {
        combinations: Vec::new(),
        truncated: false,
    };
    if limit == 0 {
        result.truncated = true;
        return result;
    }
    visit(
        available,
        0,
        count,
        0,
        &mut Vec::new(),
        required_masks,
        limit,
        &mut result.combinations,
        &mut result.truncated,
    );
    result
}

/// Swap one clue for one empty cell while retaining the source completion.
/// Callers should start from a verified minimal archive record and re-run the
/// generic analyzer on the returned candidate before registering it.
pub fn mutate_same_count(puzzle: Matrix, solution: Matrix, seed: u64) -> Option<SameCountMutation> {
    let mut checked_solution = solution;
    if !crate::solve(&mut checked_solution)
        || checked_solution != solution
        || (0..81).any(|position| {
            let clue = puzzle[position / 9][position % 9];
            clue != 0 && clue != solution[position / 9][position % 9]
        })
    {
        return None;
    }
    let clues: Vec<_> = (0..81)
        .filter(|&position| (1..=9).contains(&puzzle[position / 9][position % 9]))
        .collect();
    let empty: Vec<_> = (0..81)
        .filter(|&position| !(1..=9).contains(&puzzle[position / 9][position % 9]))
        .collect();
    if clues.is_empty() || empty.is_empty() {
        return None;
    }
    let removed_position = clues[(splitmix64(seed) as usize) % clues.len()];
    let added_position = empty[(splitmix64(seed ^ 0x9e37_79b9_7f4a_7c15) as usize) % empty.len()];
    let mut mutated = puzzle;
    mutated[removed_position / 9][removed_position % 9] = 0;
    mutated[added_position / 9][added_position % 9] =
        solution[added_position / 9][added_position % 9];
    Some(SameCountMutation {
        puzzle: mutated,
        removed_position: removed_position as u8,
        added_position: added_position as u8,
    })
}

/// Prefer a replacement clue that eliminates the stored counterexample for
/// the removed clue. Every returned mutation therefore clears at least one
/// known alternative; uniqueness still requires a fresh solver proof.
pub fn mutate_same_count_with_witness(
    analysis: &MinimalPuzzleAnalysis,
    seed: u64,
) -> Option<SameCountMutation> {
    if analysis.status != VerificationStatus::VerifiedMinimal
        || !validate_minimality_evidence(&analysis.puzzle, &analysis.solution, &analysis.evidence)
    {
        return None;
    }
    let available = empty_mask(&analysis.puzzle);
    let length = analysis.evidence.deletions.len();
    if length == 0 {
        return None;
    }
    let start = (splitmix64(seed) as usize) % length;
    for offset in 0..length {
        let witness = &analysis.evidence.deletions[(start + offset) % length];
        let removed = witness.position as usize;
        let eligible = witness.difference_mask & available & !(1u128 << removed);
        if eligible == 0 {
            continue;
        }
        let candidate_count = eligible.count_ones() as usize;
        let mut selected =
            (splitmix64(seed ^ u64::from(witness.position)) as usize) % candidate_count;
        let mut positions = eligible;
        let mut added = positions.trailing_zeros() as usize;
        while selected > 0 {
            positions &= positions - 1;
            added = positions.trailing_zeros() as usize;
            selected -= 1;
        }
        let mut puzzle = analysis.puzzle;
        puzzle[removed / 9][removed % 9] = 0;
        puzzle[added / 9][added % 9] = analysis.solution[added / 9][added % 9];
        return Some(SameCountMutation {
            puzzle,
            removed_position: removed as u8,
            added_position: added as u8,
        });
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontierNode {
    pub id: String,
    pub analysis: MinimalPuzzleAnalysis,
    pub parent_id: Option<String>,
    pub incoming_operation: Option<FrontierOperation>,
    pub explored_operations: Vec<FrontierOperation>,
    pub solver_nodes: u64,
    pub visits: u64,
    pub discoveries: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MultiClueFrontier {
    nodes: Vec<FrontierNode>,
    pub next_seed: u64,
    pub attempts: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FrontierStats {
    pub nodes: usize,
    pub unique_states: usize,
    pub by_clue_count: Vec<(u8, usize)>,
    pub explored_operations: usize,
}

impl MultiClueFrontier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn nodes(&self) -> &[FrontierNode] {
        &self.nodes
    }

    /// Add only a fully uniqueness-checked state. Minimality may be either
    /// proved or disproved; Unknown and invalid states are not frontier nodes.
    pub fn register(
        &mut self,
        analysis: MinimalPuzzleAnalysis,
        parent_id: Option<String>,
        operation: Option<FrontierOperation>,
    ) -> Result<bool, String> {
        if !matches!(
            analysis.status,
            VerificationStatus::VerifiedMinimal | VerificationStatus::VerifiedNonMinimal
        ) || !analysis.evidence.input_unique
        {
            return Err("frontier accepts only proven unique states".into());
        }
        let id = format!("{}:{}", analysis.clue_count, analysis.puzzle_key);
        if self.nodes.iter().any(|node| node.id == id) {
            return Ok(false);
        }
        let solver_nodes = analysis.evidence.total_nodes;
        self.nodes.push(FrontierNode {
            id,
            analysis,
            parent_id,
            incoming_operation: operation,
            explored_operations: Vec::new(),
            solver_nodes,
            visits: 0,
            discoveries: 0,
        });
        Ok(true)
    }

    pub fn mark_explored(&mut self, id: &str, operation: FrontierOperation) -> bool {
        let Some(node) = self.nodes.iter_mut().find(|node| node.id == id) else {
            return false;
        };
        if node.explored_operations.contains(&operation) {
            return false;
        }
        node.explored_operations.push(operation);
        node.visits = node.visits.saturating_add(1);
        true
    }

    /// Fixed-priority scheduler. N=17 gets the strongest primary-objective
    /// weight; underrepresented 18..=20 buckets receive the next preference.
    pub fn select_next(&self, min_clues: u8, max_clues: u8) -> Option<&FrontierNode> {
        self.nodes
            .iter()
            .filter(|node| (min_clues..=max_clues).contains(&node.analysis.clue_count))
            .min_by_key(|node| {
                let n = node.analysis.clue_count;
                let bucket_population = self
                    .nodes
                    .iter()
                    .filter(|other| other.analysis.clue_count == n)
                    .count();
                let target_weight = if n == 17 {
                    0
                } else if (18..=20).contains(&n) {
                    1
                } else {
                    2
                };
                (
                    target_weight,
                    bucket_population,
                    node.visits,
                    node.solver_nodes,
                    node.id.as_str(),
                )
            })
    }

    pub fn stats(&self) -> FrontierStats {
        let mut buckets = std::collections::BTreeMap::<u8, usize>::new();
        for node in &self.nodes {
            *buckets.entry(node.analysis.clue_count).or_default() += 1;
        }
        FrontierStats {
            nodes: self.nodes.len(),
            unique_states: self.nodes.len(),
            by_clue_count: buckets.into_iter().collect(),
            explored_operations: self
                .nodes
                .iter()
                .map(|node| node.explored_operations.len())
                .sum(),
        }
    }

    /// Save unverified search history separately from the verified archive.
    /// Frontier nodes are known-unique states; load re-proves them before use.
    pub fn save_checkpoint(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| {
                serde_json::json!({
                    "puzzle": encode(&node.analysis.puzzle),
                    "solution": encode(&node.analysis.solution),
                    "puzzle_key": &node.analysis.puzzle_key,
                    "solution_class": &node.analysis.solution_class,
                    "minimality": match node.analysis.status {
                        VerificationStatus::VerifiedMinimal => "minimal",
                        VerificationStatus::VerifiedNonMinimal => "nonminimal",
                        _ => "invalid",
                    },
                    "parent_id": &node.parent_id,
                    "incoming_operation": node.incoming_operation.map(FrontierOperation::checkpoint_id),
                    "explored_operations": node.explored_operations.iter().map(|op| op.checkpoint_id()).collect::<Vec<_>>(),
                    "solver_nodes": node.solver_nodes,
                    "visits": node.visits,
                    "discoveries": node.discoveries,
                })
            })
            .collect();
        let nodes_value = serde_json::Value::Array(nodes);
        let checksum = checkpoint_checksum(&nodes_value)?;
        let document = serde_json::json!({
            "format_version": FRONTIER_CHECKPOINT_VERSION,
            "next_seed": self.next_seed,
            "attempts": self.attempts,
            "nodes_checksum_fnv1a64": checksum,
            "nodes": nodes_value,
        });
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
        crate::persistence::write_atomic(path, &bytes).map_err(|error| error.to_string())
    }

    pub fn load_checkpoint(
        path: impl AsRef<Path>,
        max_nodes_per_check: u64,
    ) -> Result<Self, String> {
        if max_nodes_per_check == 0 {
            return Err("node budget must be non-zero".into());
        }
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let document: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if document
            .get("format_version")
            .and_then(serde_json::Value::as_u64)
            != Some(FRONTIER_CHECKPOINT_VERSION as u64)
        {
            return Err("unsupported frontier checkpoint version".into());
        }
        let nodes_value = document
            .get("nodes")
            .ok_or_else(|| "frontier checkpoint has no nodes".to_owned())?;
        let expected = document
            .get("nodes_checksum_fnv1a64")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "frontier checkpoint has no checksum".to_owned())?;
        if checkpoint_checksum(nodes_value)? != expected {
            return Err("frontier checkpoint checksum mismatch".into());
        }
        let values = nodes_value
            .as_array()
            .ok_or_else(|| "frontier checkpoint nodes must be an array".to_owned())?;
        let next_seed = document
            .get("next_seed")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "frontier checkpoint has no valid next_seed".to_owned())?;
        let attempts = document
            .get("attempts")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "frontier checkpoint has no valid attempts".to_owned())?;
        let mut frontier = Self {
            nodes: Vec::new(),
            next_seed,
            attempts,
        };
        for (index, value) in values.iter().enumerate() {
            let puzzle = checkpoint_board(value.get("puzzle"), index)?;
            let solution = checkpoint_board(value.get("solution"), index)?;
            let analysis = analyze_minimality(puzzle, solution, max_nodes_per_check)
                .map_err(|error| format!("frontier node {index}: {error}"))?;
            let stored_status = value
                .get("minimality")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("frontier node {index} has no minimality status"))?;
            let expected_status = match analysis.status {
                VerificationStatus::VerifiedMinimal => "minimal",
                VerificationStatus::VerifiedNonMinimal => "nonminimal",
                _ => return Err(format!("frontier node {index} is not proven unique")),
            };
            if stored_status != expected_status
                || value.get("puzzle_key").and_then(serde_json::Value::as_str)
                    != Some(analysis.puzzle_key.as_str())
                || value
                    .get("solution_class")
                    .and_then(serde_json::Value::as_str)
                    != Some(analysis.solution_class.as_str())
            {
                return Err(format!(
                    "frontier node {index} verification metadata mismatch"
                ));
            }
            let parent_id = value
                .get("parent_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let incoming_operation = match value
                .get("incoming_operation")
                .and_then(serde_json::Value::as_str)
            {
                Some(operation) => Some(
                    FrontierOperation::from_str(operation)
                        .ok_or_else(|| format!("frontier node {index} has an unknown operation"))?,
                ),
                None => None,
            };
            if !frontier.register(analysis, parent_id, incoming_operation)? {
                return Err(format!("frontier node {index} duplicates a state"));
            }
            let node = frontier.nodes.last_mut().expect("registered frontier node");
            let operations = value
                .get("explored_operations")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("frontier node {index} has no explored_operations"))?;
            for operation in operations {
                let operation = operation
                    .as_str()
                    .and_then(FrontierOperation::from_str)
                    .ok_or_else(|| {
                        format!("frontier node {index} has an unknown explored operation")
                    })?;
                if node.explored_operations.contains(&operation) {
                    return Err(format!(
                        "frontier node {index} duplicates an explored operation"
                    ));
                }
                node.explored_operations.push(operation);
            }
            node.solver_nodes = value
                .get("solver_nodes")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("frontier node {index} has invalid solver_nodes"))?;
            node.visits = value
                .get("visits")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("frontier node {index} has invalid visits"))?;
            node.discoveries = value
                .get("discoveries")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("frontier node {index} has invalid discoveries"))?;
        }
        Ok(frontier)
    }
}

fn empty_mask(board: &Matrix) -> u128 {
    let mut mask = 0u128;
    for position in 0..81 {
        if !(1..=9).contains(&board[position / 9][position % 9]) {
            mask |= 1u128 << position;
        }
    }
    mask
}

fn splitmix64(seed: u64) -> u64 {
    let mut value = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn difference_mask(solution: &Matrix, alternative: &Matrix) -> u128 {
    let mut mask = 0u128;
    for position in 0..81 {
        if solution[position / 9][position % 9] != alternative[position / 9][position % 9] {
            mask |= 1u128 << position;
        }
    }
    mask
}

fn encode(board: &Matrix) -> String {
    board
        .iter()
        .flatten()
        .map(|value| char::from(b'0' + *value))
        .collect()
}

fn checkpoint_checksum(value: &serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

fn checkpoint_board(value: Option<&serde_json::Value>, index: usize) -> Result<Matrix, String> {
    let encoded = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("frontier node {index} has no board"))?;
    if encoded.len() != 81 || !encoded.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("frontier node {index} has an invalid board"));
    }
    let mut board = [[0; 9]; 9];
    for (position, byte) in encoded.bytes().enumerate() {
        board[position / 9][position % 9] = byte - b'0';
    }
    Ok(board)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontier_accepts_only_proven_unique_states_and_schedules_17_first() {
        let (puzzle17, solution) = crate::generator::verified_17_clue_base(1);
        let analysis17 = crate::archive::analyze_minimality(puzzle17, solution, 100_000).unwrap();
        let mut frontier = MultiClueFrontier::new();
        assert!(frontier.register(analysis17.clone(), None, None).unwrap());

        let mut puzzle18 = puzzle17;
        let extra = (0..81)
            .find(|&position| puzzle18[position / 9][position % 9] == 0)
            .unwrap();
        puzzle18[extra / 9][extra % 9] = solution[extra / 9][extra % 9];
        let analysis18 = crate::archive::analyze_minimality(puzzle18, solution, 100_000).unwrap();
        assert_eq!(analysis18.status, VerificationStatus::VerifiedNonMinimal);
        assert!(frontier
            .register(
                analysis18,
                Some(analysis17.puzzle_key.clone()),
                Some(FrontierOperation::AddOne)
            )
            .unwrap());
        assert_eq!(frontier.stats().by_clue_count, vec![(17, 1), (18, 1)]);
        assert_eq!(
            frontier.select_next(17, 20).unwrap().analysis.clue_count,
            17
        );
        let id = frontier.select_next(17, 17).unwrap().id.clone();
        assert!(frontier.mark_explored(&id, FrontierOperation::TwoDropOneAdd));
        assert!(!frontier.mark_explored(&id, FrontierOperation::TwoDropOneAdd));
    }

    #[test]
    fn generalized_remove_add_search_uses_the_source_witness_intersection() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(3);
        let analysis = crate::archive::analyze_minimality(puzzle, solution, 100_000).unwrap();
        let report = search_n_remove_add(
            &analysis,
            2,
            1,
            RemoveAddSearchOptions {
                max_nodes_per_candidate: 20_000,
                max_candidate_checks: 1,
            },
        )
        .unwrap();
        assert!(report.stats.deletion_sets <= 136);
        assert!(report.stats.candidate_checks <= 1);
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.clue_count == 16
                && candidate.removed_positions.len() == 2
                && candidate.added_positions.len() == 1));
    }

    #[test]
    fn frontier_checkpoint_restores_only_revalidated_unique_nodes() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(11);
        let analysis = crate::archive::analyze_minimality(puzzle, solution, 100_000).unwrap();
        let mut frontier = MultiClueFrontier::new();
        let id = format!("17:{}", analysis.puzzle_key);
        frontier
            .register(analysis, None, Some(FrontierOperation::SameCountMutation))
            .unwrap();
        frontier.mark_explored(&id, FrontierOperation::TwoDropOneAdd);
        frontier.next_seed = 1234;
        frontier.attempts = 56;
        let path = std::env::temp_dir().join(format!(
            "sudoku-multi-frontier-{}-{}.json",
            std::process::id(),
            CHECKPOINT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        frontier.save_checkpoint(&path).unwrap();
        let restored = MultiClueFrontier::load_checkpoint(&path, 100_000).unwrap();
        assert_eq!(restored, frontier);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn frontier_checkpoint_rejects_missing_resume_counters() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(13);
        let analysis = crate::archive::analyze_minimality(puzzle, solution, 100_000).unwrap();
        let mut frontier = MultiClueFrontier::new();
        frontier.register(analysis, None, None).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sudoku-multi-frontier-invalid-{}-{}.json",
            std::process::id(),
            CHECKPOINT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        frontier.save_checkpoint(&path).unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        for field in ["next_seed", "attempts"] {
            let mut malformed = original.clone();
            malformed.as_object_mut().unwrap().remove(field);
            fs::write(&path, serde_json::to_vec(&malformed).unwrap()).unwrap();
            assert!(MultiClueFrontier::load_checkpoint(&path, 100_000).is_err());
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn same_count_mutation_replaces_exactly_one_clue_and_is_reproducible() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(37);
        let first = mutate_same_count(puzzle, solution, 909).unwrap();
        let second = mutate_same_count(puzzle, solution, 909).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first
                .puzzle
                .iter()
                .flatten()
                .filter(|&&value| value != 0)
                .count(),
            17
        );
        assert_eq!(
            first.puzzle[first.removed_position as usize / 9][first.removed_position as usize % 9],
            0
        );
        assert_eq!(
            first.puzzle[first.added_position as usize / 9][first.added_position as usize % 9],
            solution[first.added_position as usize / 9][first.added_position as usize % 9]
        );
        assert_ne!(first.removed_position, first.added_position);
    }

    #[test]
    fn witnessed_same_count_mutation_eliminates_the_selected_deletion_witness() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(43);
        let analysis = crate::archive::analyze_minimality(puzzle, solution, 100_000).unwrap();
        let mutation = mutate_same_count_with_witness(&analysis, 909).unwrap();
        let witness = analysis
            .evidence
            .deletions
            .iter()
            .find(|witness| witness.position == mutation.removed_position)
            .unwrap();
        assert_ne!(
            witness.difference_mask & (1u128 << mutation.added_position),
            0
        );
        assert_eq!(
            mutation
                .puzzle
                .iter()
                .flatten()
                .filter(|&&value| value != 0)
                .count(),
            17
        );
    }
}
