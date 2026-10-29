//! N-independent minimal-puzzle verification and verified archive storage.

use crate::solver::{
    find_alternative_solution, AlternativeSolutionStatus, SearchEvent, SolutionSearch,
};
use crate::symmetry::canonical_key;
use crate::{solve, Matrix};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

pub const MINIMAL_ARCHIVE_FORMAT_VERSION: u32 = 1;
pub const CANONICAL_KEY_VERSION: u32 = 1;
#[cfg(test)]
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationStatus {
    VerifiedMinimal,
    VerifiedNonMinimal,
    InvalidPuzzle,
    BudgetExceeded,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeletionStatus {
    AlternativeFound,
    UniqueAfterDeletion,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletionWitness {
    pub position: u8,
    pub status: DeletionStatus,
    pub alternative: Option<Matrix>,
    pub difference_mask: u128,
    pub explored_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimalityEvidence {
    pub input_unique: bool,
    pub input_proof_nodes: u64,
    pub deletions: Vec<DeletionWitness>,
    pub total_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimalPuzzleAnalysis {
    pub puzzle: Matrix,
    pub solution: Matrix,
    pub clue_count: u8,
    pub puzzle_key: String,
    pub solution_class: String,
    pub status: VerificationStatus,
    pub input_status: AlternativeSolutionStatus,
    pub evidence: MinimalityEvidence,
    pub direct_reductions: Vec<Matrix>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MinimalityError {
    InvalidNodeBudget,
    InvalidPuzzleValues,
    InvalidSolution,
    SolutionDoesNotMatchPuzzle,
}

impl fmt::Display for MinimalityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

/// Verify uniqueness and then test every clue deletion. A clue is necessary
/// only when an alternative completion is found; Unknown never proves it.
pub fn analyze_minimality(
    puzzle: Matrix,
    solution: Matrix,
    max_nodes_per_check: u64,
) -> Result<MinimalPuzzleAnalysis, MinimalityError> {
    if max_nodes_per_check == 0 {
        return Err(MinimalityError::InvalidNodeBudget);
    }
    if puzzle
        .iter()
        .flatten()
        .any(|&value| value != 0 && !(1..=9).contains(&value))
    {
        return Err(MinimalityError::InvalidPuzzleValues);
    }
    if !is_complete_solution(&solution) {
        return Err(MinimalityError::InvalidSolution);
    }
    if !matches_clues(&solution, &puzzle) {
        return Err(MinimalityError::SolutionDoesNotMatchPuzzle);
    }

    let clue_count = puzzle
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count() as u8;
    let input_check = find_alternative_solution(&puzzle, &solution, max_nodes_per_check);
    let mut evidence = MinimalityEvidence {
        input_unique: input_check.status == AlternativeSolutionStatus::Unique,
        input_proof_nodes: input_check.explored_nodes,
        deletions: Vec::with_capacity(clue_count as usize),
        total_nodes: input_check.explored_nodes,
    };
    let status = match input_check.status {
        AlternativeSolutionStatus::AlternativeFound | AlternativeSolutionStatus::InvalidInput => {
            VerificationStatus::InvalidPuzzle
        }
        AlternativeSolutionStatus::Unknown => VerificationStatus::BudgetExceeded,
        AlternativeSolutionStatus::Unique => {
            let mut redundant_clue = false;
            let mut unknown_deletion = false;
            let mut direct_reductions = Vec::new();
            for position in 0..81 {
                if !(1..=9).contains(&puzzle[position / 9][position % 9]) {
                    continue;
                }
                let mut reduced = puzzle;
                reduced[position / 9][position % 9] = 0;
                let check = find_alternative_solution(&reduced, &solution, max_nodes_per_check);
                evidence.total_nodes = evidence.total_nodes.saturating_add(check.explored_nodes);
                let (deletion_status, alternative, difference_mask) = match check.status {
                    AlternativeSolutionStatus::AlternativeFound => {
                        let alternative = check.alternative;
                        let difference_mask = alternative
                            .as_ref()
                            .map_or(0, |other| solution_difference_mask(&solution, other));
                        (
                            DeletionStatus::AlternativeFound,
                            alternative,
                            difference_mask,
                        )
                    }
                    AlternativeSolutionStatus::Unique => {
                        redundant_clue = true;
                        direct_reductions.push(reduced);
                        (DeletionStatus::UniqueAfterDeletion, None, 0)
                    }
                    AlternativeSolutionStatus::Unknown => {
                        unknown_deletion = true;
                        (DeletionStatus::Unknown, None, 0)
                    }
                    AlternativeSolutionStatus::InvalidInput => {
                        return Err(MinimalityError::SolutionDoesNotMatchPuzzle)
                    }
                };
                evidence.deletions.push(DeletionWitness {
                    position: position as u8,
                    status: deletion_status,
                    alternative,
                    difference_mask,
                    explored_nodes: check.explored_nodes,
                });
            }
            let status = if redundant_clue {
                VerificationStatus::VerifiedNonMinimal
            } else if unknown_deletion {
                VerificationStatus::BudgetExceeded
            } else {
                VerificationStatus::VerifiedMinimal
            };
            return Ok(MinimalPuzzleAnalysis {
                puzzle,
                solution,
                clue_count,
                puzzle_key: canonical_key(&puzzle),
                solution_class: canonical_key(&solution),
                status,
                input_status: AlternativeSolutionStatus::Unique,
                evidence,
                direct_reductions,
            });
        }
    };
    Ok(MinimalPuzzleAnalysis {
        puzzle,
        solution,
        clue_count,
        puzzle_key: canonical_key(&puzzle),
        solution_class: canonical_key(&solution),
        status,
        input_status: input_check.status,
        evidence,
        direct_reductions: Vec::new(),
    })
}

/// Verify a puzzle when its completion is not supplied. The first completion
/// is found with the shared solver, then all uniqueness and deletion checks
/// use the bounded alternative-solution API.
pub fn analyze_minimality_from_puzzle(
    puzzle: Matrix,
    max_nodes_per_check: u64,
) -> Result<MinimalPuzzleAnalysis, MinimalityError> {
    if max_nodes_per_check == 0 {
        return Err(MinimalityError::InvalidNodeBudget);
    }
    if puzzle
        .iter()
        .flatten()
        .any(|&value| value != 0 && !(1..=9).contains(&value))
    {
        return Err(MinimalityError::InvalidPuzzleValues);
    }
    let first_solution = match find_first_solution_bounded(&puzzle, max_nodes_per_check) {
        FirstSolution::Found(solution) => solution,
        FirstSolution::NoSolution(nodes) => {
            let empty_solution = [[0; 9]; 9];
            return Ok(MinimalPuzzleAnalysis {
                puzzle,
                solution: empty_solution,
                clue_count: count_clues(&puzzle),
                puzzle_key: canonical_key(&puzzle),
                solution_class: String::new(),
                status: VerificationStatus::InvalidPuzzle,
                input_status: AlternativeSolutionStatus::InvalidInput,
                evidence: MinimalityEvidence {
                    input_unique: false,
                    input_proof_nodes: nodes,
                    deletions: Vec::new(),
                    total_nodes: nodes,
                },
                direct_reductions: Vec::new(),
            });
        }
        FirstSolution::BudgetExceeded(nodes) => {
            let empty_solution = [[0; 9]; 9];
            return Ok(MinimalPuzzleAnalysis {
                puzzle,
                solution: empty_solution,
                clue_count: count_clues(&puzzle),
                puzzle_key: canonical_key(&puzzle),
                solution_class: String::new(),
                status: VerificationStatus::BudgetExceeded,
                input_status: AlternativeSolutionStatus::Unknown,
                evidence: MinimalityEvidence {
                    input_unique: false,
                    input_proof_nodes: nodes,
                    deletions: Vec::new(),
                    total_nodes: nodes,
                },
                direct_reductions: Vec::new(),
            });
        }
    };
    analyze_minimality(puzzle, first_solution, max_nodes_per_check)
}

enum FirstSolution {
    Found(Matrix),
    NoSolution(u64),
    BudgetExceeded(u64),
}

fn find_first_solution_bounded(puzzle: &Matrix, max_nodes: u64) -> FirstSolution {
    let Ok(mut search) = SolutionSearch::new(puzzle) else {
        return FirstSolution::NoSolution(0);
    };
    loop {
        if search.explored_nodes() >= max_nodes {
            return FirstSolution::BudgetExceeded(search.explored_nodes());
        }
        match search.next() {
            Some(SearchEvent::SolutionFound { board, .. }) => {
                return FirstSolution::Found(board);
            }
            Some(SearchEvent::Progress { .. }) => {}
            None => return FirstSolution::NoSolution(search.explored_nodes()),
        }
    }
}

/// Verify that stored deletion witnesses are valid proofs for this exact
/// puzzle and completion. The original puzzle's uniqueness must be proven
/// separately by [`analyze_minimality`].
pub fn validate_minimality_evidence(
    puzzle: &Matrix,
    solution: &Matrix,
    evidence: &MinimalityEvidence,
) -> bool {
    if !evidence.input_unique || !is_complete_solution(solution) || !matches_clues(solution, puzzle)
    {
        return false;
    }
    let mut expected = HashSet::new();
    for position in 0..81 {
        if (1..=9).contains(&puzzle[position / 9][position % 9]) {
            expected.insert(position as u8);
        }
    }
    if expected.len() != evidence.deletions.len() {
        return false;
    }
    let mut seen = HashSet::new();
    for witness in &evidence.deletions {
        let position = witness.position as usize;
        if position >= 81
            || !expected.contains(&witness.position)
            || !seen.insert(witness.position)
            || witness.status != DeletionStatus::AlternativeFound
        {
            return false;
        }
        let Some(alternative) = witness.alternative.as_ref() else {
            return false;
        };
        if !is_complete_solution(alternative)
            || alternative == solution
            || solution_difference_mask(solution, alternative) != witness.difference_mask
        {
            return false;
        }
        let mut reduced = *puzzle;
        reduced[position / 9][position % 9] = 0;
        if !matches_clues(alternative, &reduced) {
            return false;
        }
    }
    seen == expected
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Provenance {
    pub source: String,
    pub algorithm: String,
    pub seed: Option<u64>,
    pub parent_id: Option<String>,
    pub operation: Option<String>,
    pub search_depth: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimalPuzzleRecord {
    pub puzzle: Matrix,
    pub solution: Matrix,
    pub clue_count: u8,
    pub puzzle_key: String,
    pub solution_class: String,
    pub evidence: MinimalityEvidence,
    pub provenance: Provenance,
}

impl MinimalPuzzleRecord {
    /// Rebuild the analysis value from a record already admitted to a
    /// verified archive. Archive loading and insertion revalidate this proof.
    pub fn as_analysis(&self) -> MinimalPuzzleAnalysis {
        MinimalPuzzleAnalysis {
            puzzle: self.puzzle,
            solution: self.solution,
            clue_count: self.clue_count,
            puzzle_key: self.puzzle_key.clone(),
            solution_class: self.solution_class.clone(),
            status: VerificationStatus::VerifiedMinimal,
            input_status: AlternativeSolutionStatus::Unique,
            evidence: self.evidence.clone(),
            direct_reductions: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MinimalPuzzleArchive {
    records: Vec<MinimalPuzzleRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveError(pub String);

impl fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArchiveStats {
    pub total_records: usize,
    pub by_clue_count: Vec<(u8, usize)>,
    pub distinct_solution_classes: usize,
}

impl MinimalPuzzleArchive {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn records(&self) -> &[MinimalPuzzleRecord] {
        &self.records
    }

    /// Merge records from another archive that has already passed full load
    /// verification. Returns the number of new MinLex puzzle classes added.
    pub fn merge_verified(&mut self, source: &Self) -> usize {
        let mut inserted = 0usize;
        for record in &source.records {
            if self.find(record.clue_count, &record.puzzle_key).is_none() {
                self.records.push(record.clone());
                inserted += 1;
            }
        }
        inserted
    }

    pub fn count(&self, clue_count: u8) -> usize {
        self.records
            .iter()
            .filter(|record| record.clue_count == clue_count)
            .count()
    }

    pub fn find(&self, clue_count: u8, puzzle_key: &str) -> Option<&MinimalPuzzleRecord> {
        self.records
            .iter()
            .find(|record| record.clue_count == clue_count && record.puzzle_key == puzzle_key)
    }

    pub fn by_solution_class(&self, solution_class: &str) -> Vec<&MinimalPuzzleRecord> {
        self.records
            .iter()
            .filter(|record| record.solution_class == solution_class)
            .collect()
    }

    /// Deterministically select a seed record from the requested N bucket.
    pub fn random_start(&self, clue_count: u8, seed: u64) -> Option<&MinimalPuzzleRecord> {
        let bucket: Vec<_> = self
            .records
            .iter()
            .filter(|record| record.clue_count == clue_count)
            .collect();
        (!bucket.is_empty()).then(|| bucket[(splitmix64(seed) as usize) % bucket.len()])
    }

    pub fn stats(&self) -> ArchiveStats {
        let mut counts = HashMap::<u8, usize>::new();
        let mut solution_classes = HashSet::new();
        for record in &self.records {
            *counts.entry(record.clue_count).or_default() += 1;
            solution_classes.insert(record.solution_class.as_str());
        }
        let mut by_clue_count: Vec<_> = counts.into_iter().collect();
        by_clue_count.sort_unstable_by_key(|(count, _)| *count);
        ArchiveStats {
            total_records: self.records.len(),
            by_clue_count,
            distinct_solution_classes: solution_classes.len(),
        }
    }

    pub fn insert_analysis(
        &mut self,
        analysis: &MinimalPuzzleAnalysis,
        provenance: Provenance,
        max_nodes_per_check: u64,
    ) -> Result<bool, ArchiveError> {
        if analysis.status != VerificationStatus::VerifiedMinimal
            || analysis.clue_count == 0
            || !validate_minimality_evidence(
                &analysis.puzzle,
                &analysis.solution,
                &analysis.evidence,
            )
        {
            return Err(ArchiveError(
                "only fully verified minimal puzzles may be registered".into(),
            ));
        }
        let rechecked = analyze_minimality(analysis.puzzle, analysis.solution, max_nodes_per_check)
            .map_err(|error| ArchiveError(error.to_string()))?;
        if rechecked.status != VerificationStatus::VerifiedMinimal
            || rechecked.clue_count != analysis.clue_count
            || rechecked.puzzle_key != analysis.puzzle_key
            || rechecked.solution_class != analysis.solution_class
        {
            return Err(ArchiveError(
                "independent archive verification failed".into(),
            ));
        }
        self.insert_verified_analysis(analysis, provenance)
    }

    fn insert_verified_analysis(
        &mut self,
        analysis: &MinimalPuzzleAnalysis,
        provenance: Provenance,
    ) -> Result<bool, ArchiveError> {
        if self
            .find(analysis.clue_count, &analysis.puzzle_key)
            .is_some()
        {
            return Ok(false);
        }
        self.records.push(MinimalPuzzleRecord {
            puzzle: analysis.puzzle,
            solution: analysis.solution,
            clue_count: analysis.clue_count,
            puzzle_key: analysis.puzzle_key.clone(),
            solution_class: analysis.solution_class.clone(),
            evidence: analysis.evidence.clone(),
            provenance,
        });
        Ok(true)
    }

    pub fn import_puzzle(
        &mut self,
        puzzle: Matrix,
        solution: Matrix,
        provenance: Provenance,
        max_nodes_per_check: u64,
    ) -> Result<(MinimalPuzzleAnalysis, bool), ArchiveError> {
        let analysis = analyze_minimality(puzzle, solution, max_nodes_per_check)
            .map_err(|error| ArchiveError(error.to_string()))?;
        if analysis.status != VerificationStatus::VerifiedMinimal {
            return Err(ArchiveError(format!(
                "import candidate is {:?}, not verified minimal",
                analysis.status
            )));
        }
        if analysis.status != VerificationStatus::VerifiedMinimal
            || !validate_minimality_evidence(
                &analysis.puzzle,
                &analysis.solution,
                &analysis.evidence,
            )
        {
            return Err(ArchiveError(
                "import candidate is not verified minimal".into(),
            ));
        }
        let inserted = self.insert_verified_analysis(&analysis, provenance)?;
        Ok((analysis, inserted))
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), ArchiveError> {
        let path = path.as_ref();
        let records = self.records.iter().map(record_to_json).collect::<Vec<_>>();
        let records_value = serde_json::Value::Array(records);
        let checksum = checksum(&records_value)?;
        let document = serde_json::json!({
            "format_version": MINIMAL_ARCHIVE_FORMAT_VERSION,
            "canonical_key_version": CANONICAL_KEY_VERSION,
            "records_checksum_fnv1a64": checksum,
            "records": records_value,
        });
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| ArchiveError(error.to_string()))?;
        crate::persistence::write_atomic(path, &bytes)
            .map_err(|error| ArchiveError(error.to_string()))
    }

    /// Loading never trusts serialized status or evidence. It independently
    /// proves uniqueness and validates every persisted deletion witness.
    pub fn load(path: impl AsRef<Path>, max_nodes_per_check: u64) -> Result<Self, ArchiveError> {
        if max_nodes_per_check == 0 {
            return Err(ArchiveError("node budget must be non-zero".into()));
        }
        let bytes = fs::read(path).map_err(|error| ArchiveError(error.to_string()))?;
        let document: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| ArchiveError(error.to_string()))?;
        if document
            .get("format_version")
            .and_then(|value| value.as_u64())
            != Some(MINIMAL_ARCHIVE_FORMAT_VERSION as u64)
            || document
                .get("canonical_key_version")
                .and_then(|value| value.as_u64())
                != Some(CANONICAL_KEY_VERSION as u64)
        {
            return Err(ArchiveError(
                "unsupported archive or canonical-key version".into(),
            ));
        }
        let records_value = document
            .get("records")
            .ok_or_else(|| ArchiveError("archive has no records".into()))?;
        let expected_checksum = document
            .get("records_checksum_fnv1a64")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ArchiveError("archive has no records checksum".into()))?;
        if checksum(records_value)? != expected_checksum {
            return Err(ArchiveError("archive records checksum mismatch".into()));
        }
        let entries = records_value
            .as_array()
            .ok_or_else(|| ArchiveError("archive records must be an array".into()))?;
        let mut archive = Self::new();
        let mut seen = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            let puzzle = decode_board(entry.get("puzzle"), "puzzle", index)?;
            let solution = decode_board(entry.get("solution"), "solution", index)?;
            let analysis = analyze_minimality(puzzle, solution, max_nodes_per_check)
                .map_err(|error| ArchiveError(format!("record {index}: {error}")))?;
            if analysis.status != VerificationStatus::VerifiedMinimal {
                return Err(ArchiveError(format!(
                    "record {index} failed minimality proof"
                )));
            }
            let puzzle_key = required_str(entry, "puzzle_key", index)?;
            let solution_class = required_str(entry, "solution_class", index)?;
            if puzzle_key != analysis.puzzle_key || solution_class != analysis.solution_class {
                return Err(ArchiveError(format!(
                    "record {index} canonical key mismatch"
                )));
            }
            let clue_count = entry
                .get("clue_count")
                .and_then(serde_json::Value::as_u64)
                .filter(|&count| count <= 81)
                .ok_or_else(|| ArchiveError(format!("record {index} has invalid clue_count")))?
                as u8;
            if clue_count != analysis.clue_count {
                return Err(ArchiveError(format!("record {index} clue count mismatch")));
            }
            let evidence = evidence_from_json(entry.get("evidence"), index)?;
            if !validate_minimality_evidence(&puzzle, &solution, &evidence) {
                return Err(ArchiveError(format!(
                    "record {index} has invalid minimality evidence"
                )));
            }
            let id = format!("{clue_count}:{puzzle_key}");
            if !seen.insert(id) {
                return Err(ArchiveError(format!(
                    "record {index} duplicates a puzzle class"
                )));
            }
            archive.records.push(MinimalPuzzleRecord {
                puzzle,
                solution,
                clue_count,
                puzzle_key: puzzle_key.to_owned(),
                solution_class: solution_class.to_owned(),
                evidence,
                provenance: provenance_from_json(entry.get("provenance"), index)?,
            });
        }
        Ok(archive)
    }
}

fn record_to_json(record: &MinimalPuzzleRecord) -> serde_json::Value {
    serde_json::json!({
        "puzzle": encode(&record.puzzle),
        "solution": encode(&record.solution),
        "clue_count": record.clue_count,
        "puzzle_key": &record.puzzle_key,
        "solution_class": &record.solution_class,
        "evidence": evidence_to_json(&record.evidence),
        "provenance": {
            "source": &record.provenance.source,
            "algorithm": &record.provenance.algorithm,
            "seed": record.provenance.seed,
            "parent_id": &record.provenance.parent_id,
            "operation": &record.provenance.operation,
            "search_depth": record.provenance.search_depth,
        }
    })
}

fn evidence_to_json(evidence: &MinimalityEvidence) -> serde_json::Value {
    serde_json::json!({
        "input_unique": evidence.input_unique,
        "input_proof_nodes": evidence.input_proof_nodes,
        "total_nodes": evidence.total_nodes,
        "deletions": evidence.deletions.iter().map(|witness| serde_json::json!({
            "position": witness.position,
            "status": match witness.status {
                DeletionStatus::AlternativeFound => "alternative_found",
                DeletionStatus::UniqueAfterDeletion => "unique_after_deletion",
                DeletionStatus::Unknown => "unknown",
            },
            "alternative": witness.alternative.as_ref().map(encode),
            "difference_mask": format!("{:032x}", witness.difference_mask),
            "explored_nodes": witness.explored_nodes,
        })).collect::<Vec<_>>(),
    })
}

fn evidence_from_json(
    value: Option<&serde_json::Value>,
    index: usize,
) -> Result<MinimalityEvidence, ArchiveError> {
    let value = value.ok_or_else(|| ArchiveError(format!("record {index} has no evidence")))?;
    let deletions = value
        .get("deletions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ArchiveError(format!("record {index} evidence has no deletions")))?
        .iter()
        .map(|deletion| {
            let position = deletion
                .get("position")
                .and_then(serde_json::Value::as_u64)
                .filter(|&position| position < 81)
                .ok_or_else(|| {
                    ArchiveError(format!("record {index} has invalid witness position"))
                })? as u8;
            let status = match required_str(deletion, "status", index)? {
                "alternative_found" => DeletionStatus::AlternativeFound,
                "unique_after_deletion" => DeletionStatus::UniqueAfterDeletion,
                "unknown" => DeletionStatus::Unknown,
                _ => {
                    return Err(ArchiveError(format!(
                        "record {index} has unknown deletion status"
                    )))
                }
            };
            let alternative = match deletion.get("alternative") {
                Some(serde_json::Value::String(encoded)) => {
                    Some(decode_board_value(encoded, index)?)
                }
                Some(serde_json::Value::Null) | None => None,
                _ => {
                    return Err(ArchiveError(format!(
                        "record {index} has invalid alternative"
                    )))
                }
            };
            let mask = required_str(deletion, "difference_mask", index)?;
            let difference_mask = u128::from_str_radix(mask, 16)
                .map_err(|_| ArchiveError(format!("record {index} has invalid difference mask")))?;
            let explored_nodes = deletion
                .get("explored_nodes")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| ArchiveError(format!("record {index} has invalid node count")))?;
            Ok(DeletionWitness {
                position,
                status,
                alternative,
                difference_mask,
                explored_nodes,
            })
        })
        .collect::<Result<Vec<_>, ArchiveError>>()?;
    let input_unique = value
        .get("input_unique")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| ArchiveError(format!("record {index} has invalid uniqueness evidence")))?;
    let input_proof_nodes = value
        .get("input_proof_nodes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ArchiveError(format!("record {index} has invalid input node count")))?;
    let total_nodes = value
        .get("total_nodes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ArchiveError(format!("record {index} has invalid total node count")))?;
    Ok(MinimalityEvidence {
        input_unique,
        input_proof_nodes,
        deletions,
        total_nodes,
    })
}

fn provenance_from_json(
    value: Option<&serde_json::Value>,
    index: usize,
) -> Result<Provenance, ArchiveError> {
    let value = value.ok_or_else(|| ArchiveError(format!("record {index} has no provenance")))?;
    Ok(Provenance {
        source: required_str(value, "source", index)?.to_owned(),
        algorithm: required_str(value, "algorithm", index)?.to_owned(),
        seed: value.get("seed").and_then(serde_json::Value::as_u64),
        parent_id: value
            .get("parent_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        operation: value
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        search_depth: u32::try_from(
            value
                .get("search_depth")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| ArchiveError(format!("record {index} has invalid search depth")))?,
        )
        .map_err(|_| ArchiveError(format!("record {index} search depth exceeds u32")))?,
    })
}

fn required_str<'a>(
    value: &'a serde_json::Value,
    field: &str,
    index: usize,
) -> Result<&'a str, ArchiveError> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ArchiveError(format!("record {index} has no valid {field}")))
}

fn decode_board(
    value: Option<&serde_json::Value>,
    field: &str,
    index: usize,
) -> Result<Matrix, ArchiveError> {
    let encoded = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ArchiveError(format!("record {index} has no {field}")))?;
    decode_board_value(encoded, index)
}

fn decode_board_value(encoded: &str, index: usize) -> Result<Matrix, ArchiveError> {
    if encoded.len() != 81 || !encoded.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ArchiveError(format!(
            "record {index} has invalid board encoding"
        )));
    }
    let mut board = [[0; 9]; 9];
    for (position, byte) in encoded.bytes().enumerate() {
        board[position / 9][position % 9] = byte - b'0';
    }
    Ok(board)
}

fn checksum(value: &serde_json::Value) -> Result<String, ArchiveError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ArchiveError(error.to_string()))?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

fn encode(board: &Matrix) -> String {
    board
        .iter()
        .flatten()
        .map(|value| char::from(b'0' + *value))
        .collect()
}

fn count_clues(puzzle: &Matrix) -> u8 {
    puzzle
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count() as u8
}

fn is_complete_solution(solution: &Matrix) -> bool {
    if !solution
        .iter()
        .flatten()
        .all(|&value| (1..=9).contains(&value))
    {
        return false;
    }
    let mut checked = *solution;
    solve(&mut checked) && checked == *solution
}

fn matches_clues(solution: &Matrix, puzzle: &Matrix) -> bool {
    solution
        .iter()
        .flatten()
        .zip(puzzle.iter().flatten())
        .all(|(&digit, &clue)| !(1..=9).contains(&clue) || digit == clue)
}

pub fn solution_difference_mask(solution: &Matrix, alternative: &Matrix) -> u128 {
    let mut mask = 0u128;
    for position in 0..81 {
        if solution[position / 9][position % 9] != alternative[position / 9][position % 9] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_verifier_proves_17_minimal_and_18_nonminimal() {
        let (puzzle17, solution) = crate::generator::verified_17_clue_base(1);
        let minimal = analyze_minimality(puzzle17, solution, 100_000).unwrap();
        assert_eq!(minimal.clue_count, 17);
        assert_eq!(minimal.status, VerificationStatus::VerifiedMinimal);
        assert_eq!(minimal.evidence.deletions.len(), 17);
        assert!(validate_minimality_evidence(
            &minimal.puzzle,
            &minimal.solution,
            &minimal.evidence
        ));

        let extra = (0..81)
            .find(|&position| puzzle17[position / 9][position % 9] == 0)
            .unwrap();
        let mut puzzle18 = puzzle17;
        puzzle18[extra / 9][extra % 9] = solution[extra / 9][extra % 9];
        let nonminimal = analyze_minimality(puzzle18, solution, 100_000).unwrap();
        assert_eq!(nonminimal.clue_count, 18);
        assert_eq!(nonminimal.status, VerificationStatus::VerifiedNonMinimal);
        assert!(nonminimal.direct_reductions.contains(&puzzle17));
    }

    #[test]
    fn puzzle_only_analysis_respects_node_budget_while_finding_first_solution() {
        assert_eq!(
            analyze_minimality_from_puzzle([[0; 9]; 9], 0).unwrap_err(),
            MinimalityError::InvalidNodeBudget
        );
        let analysis = analyze_minimality_from_puzzle([[0; 9]; 9], 1).unwrap();
        assert_eq!(analysis.status, VerificationStatus::BudgetExceeded);
        assert_eq!(analysis.input_status, AlternativeSolutionStatus::Unknown);
        assert!(analysis.evidence.input_proof_nodes >= 1);
        assert_eq!(
            analysis.evidence.total_nodes,
            analysis.evidence.input_proof_nodes
        );
    }

    #[test]
    fn archive_provenance_requires_a_valid_nontruncated_search_depth() {
        let base = serde_json::json!({
            "source": "test",
            "algorithm": "test",
            "search_depth": 0,
        });
        assert_eq!(
            provenance_from_json(Some(&base), 0).unwrap().search_depth,
            0
        );

        for depth in [
            serde_json::Value::Null,
            serde_json::json!("1"),
            serde_json::json!(u64::from(u32::MAX) + 1),
        ] {
            let mut invalid = base.clone();
            invalid["search_depth"] = depth;
            assert!(provenance_from_json(Some(&invalid), 0).is_err());
        }

        let mut missing = base;
        missing.as_object_mut().unwrap().remove("search_depth");
        assert!(provenance_from_json(Some(&missing), 0).is_err());
    }

    #[test]
    fn generic_archive_indexes_by_n_and_detects_file_corruption() {
        let path = std::env::temp_dir().join(format!(
            "sudoku-minimal-archive-{}-{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let (puzzle, solution) = crate::generator::verified_17_clue_base(7);
        let analysis = analyze_minimality(puzzle, solution, 100_000).unwrap();
        let mut archive = MinimalPuzzleArchive::new();
        let provenance = Provenance {
            source: "unit-test".into(),
            algorithm: "fixture".into(),
            seed: Some(7),
            ..Provenance::default()
        };
        assert!(archive
            .insert_analysis(&analysis, provenance, 100_000)
            .unwrap());
        assert!(!archive
            .insert_analysis(&analysis, Provenance::default(), 100_000)
            .unwrap());
        assert_eq!(archive.count(17), 1);
        assert_eq!(archive.count(18), 0);
        assert_eq!(archive.stats().distinct_solution_classes, 1);
        archive.save_atomic(&path).unwrap();

        let loaded = MinimalPuzzleArchive::load(&path, 100_000).unwrap();
        assert_eq!(loaded, archive);
        let mut bytes = fs::read(&path).unwrap();
        let byte = bytes.iter_mut().find(|byte| **byte == b'7').unwrap();
        *byte = b'8';
        fs::write(&path, bytes).unwrap();
        assert!(MinimalPuzzleArchive::load(&path, 100_000).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn verified_archive_merge_is_idempotent() {
        let (puzzle, solution) = crate::generator::verified_17_clue_base(21);
        let analysis = analyze_minimality(puzzle, solution, 100_000).unwrap();
        let mut source = MinimalPuzzleArchive::new();
        source
            .insert_analysis(&analysis, Provenance::default(), 100_000)
            .unwrap();
        let mut destination = source.clone();
        assert_eq!(destination.merge_verified(&source), 0);
        assert_eq!(destination, source);
    }
}
