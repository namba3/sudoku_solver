//! Exact canonicalization and independent equivalence checking for classic Sudoku.
//!
//! The spatial group contains 2 * 1,296 * 1,296 transforms. Digit relabeling is
//! normalized by first occurrence, so the 9! digit permutations are never listed.

use crate::{check_uniqueness, Matrix, UniquenessStatus};
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

const CANONICAL_FORMAT_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SpatialTransform {
    /// Source row selected for each output row (or source column when transposed).
    pub rows: [u8; 9],
    /// Source column selected for each output column (or source row when transposed).
    pub cols: [u8; 9],
    pub transpose: bool,
}

impl SpatialTransform {
    pub fn apply(&self, board: &Matrix) -> Matrix {
        let mut out = [[0; 9]; 9];
        for (y, row) in out.iter_mut().enumerate() {
            for (x, value) in row.iter_mut().enumerate() {
                *value = if self.transpose {
                    board[self.cols[x] as usize][self.rows[y] as usize]
                } else {
                    board[self.rows[y] as usize][self.cols[x] as usize]
                };
            }
        }
        out
    }

    pub fn apply_clues(&self, clues: u128) -> u128 {
        let mut out = 0u128;
        for y in 0..9 {
            for x in 0..9 {
                let (sy, sx) = if self.transpose {
                    (self.cols[x] as usize, self.rows[y] as usize)
                } else {
                    (self.rows[y] as usize, self.cols[x] as usize)
                };
                if clues & clue_bit(sy, sx) != 0 {
                    out |= clue_bit(y, x);
                }
            }
        }
        out
    }

    /// Compose spatial actions so that `self.compose(inner).apply(board)` is
    /// equal to `self.apply(&inner.apply(board))`.
    pub fn compose(&self, inner: &Self) -> Self {
        let transpose = self.transpose ^ inner.transpose;
        let mut result = Self {
            rows: [0; 9],
            cols: [0; 9],
            transpose,
        };
        for y in 0..9 {
            for x in 0..9 {
                let (mid_y, mid_x) = self.source_coord(y, x);
                let (src_y, src_x) = inner.source_coord(mid_y, mid_x);
                if transpose {
                    result.rows[y] = src_x as u8;
                    result.cols[x] = src_y as u8;
                } else {
                    result.rows[y] = src_y as u8;
                    result.cols[x] = src_x as u8;
                }
            }
        }
        result
    }

    /// Return the inverse spatial action.
    pub fn inverse(&self) -> Self {
        let mut result = Self {
            rows: [0; 9],
            cols: [0; 9],
            transpose: self.transpose,
        };
        for y in 0..9 {
            for x in 0..9 {
                let (src_y, src_x) = self.source_coord(y, x);
                if self.transpose {
                    result.rows[src_y] = x as u8;
                    result.cols[src_x] = y as u8;
                } else {
                    result.rows[src_y] = y as u8;
                    result.cols[src_x] = x as u8;
                }
            }
        }
        result
    }

    fn source_coord(&self, y: usize, x: usize) -> (usize, usize) {
        if self.transpose {
            (self.cols[x] as usize, self.rows[y] as usize)
        } else {
            (self.rows[y] as usize, self.cols[x] as usize)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitMapping(pub [u8; 10]);

impl DigitMapping {
    pub fn apply(&self, board: &Matrix) -> Matrix {
        let mut out = *board;
        for value in out.iter_mut().flatten() {
            if (1..=9).contains(value) {
                *value = self.0[*value as usize];
            } else {
                *value = 0;
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransformWitness {
    pub spatial: SpatialTransform,
    pub digits: DigitMapping,
}

impl TransformWitness {
    pub fn apply(&self, board: &Matrix) -> Matrix {
        self.digits.apply(&self.spatial.apply(board))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalizationMethod {
    PuzzleMinLex,
    SolutionMinLex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalKey {
    Puzzle([u8; 81]),
    Solution {
        canonical_solution: [u8; 81],
        canonical_clues: u128,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CanonicalizationStats {
    pub transforms_considered: u64,
    pub pattern_prefix_prunes: u64,
    pub solution_prefix_prunes: u64,
    pub minimum_pattern_transforms: u64,
    pub uniqueness_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalizationResult {
    pub key: CanonicalKey,
    pub witness: TransformWitness,
    pub stats: CanonicalizationStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalizationError {
    InvalidPuzzle,
    InvalidSolution,
    SolutionDoesNotMatchClues,
    NoSolution,
    MultipleSolutions,
    UniquenessUnknown { explored_nodes: u64 },
}

/// Canonicalize a clue board without requiring or consulting a completion.
/// The leading zero/one pattern is minimized first. All transforms attaining
/// that exact minimum remain eligible for the digit-normalized comparison.
pub fn canonicalize_puzzle(board: &Matrix) -> CanonicalizationResult {
    let perms = grouped_permutations();
    let mut best_pattern = [u8::MAX; 81];
    let mut best_key = [u8::MAX; 81];
    let mut best_transform = identity_transform();
    let mut stats = CanonicalizationStats::default();

    for transpose in [false, true] {
        for &rows in perms {
            for &cols in perms {
                stats.transforms_considered += 1;
                let transform = SpatialTransform {
                    rows,
                    cols,
                    transpose,
                };
                let mut pattern = [0u8; 81];
                let mut key = [0u8; 81];
                let mut map = [0u8; 10];
                let mut next = 1u8;
                let mut pruned = false;
                let mut prefix_order = std::cmp::Ordering::Equal;
                for y in 0..9 {
                    for x in 0..9 {
                        let v = source_value(board, &transform, y, x);
                        let i = y * 9 + x;
                        pattern[i] = u8::from((1..=9).contains(&v));
                        if best_pattern[0] != u8::MAX && prefix_order == std::cmp::Ordering::Equal {
                            match pattern[i].cmp(&best_pattern[i]) {
                                std::cmp::Ordering::Greater => {
                                    stats.pattern_prefix_prunes += 1;
                                    pruned = true;
                                    break;
                                }
                                std::cmp::Ordering::Less => prefix_order = std::cmp::Ordering::Less,
                                std::cmp::Ordering::Equal => {}
                            }
                        }
                        if (1..=9).contains(&v) {
                            let d = &mut map[v as usize];
                            if *d == 0 {
                                *d = next;
                                next += 1;
                            }
                            key[i] = *d;
                        }
                    }
                    if pruned {
                        break;
                    }
                }
                if pruned {
                    continue;
                }
                match pattern.cmp(&best_pattern) {
                    std::cmp::Ordering::Less => {
                        best_pattern = pattern;
                        best_key = key;
                        best_transform = transform;
                        stats.minimum_pattern_transforms = 1;
                    }
                    std::cmp::Ordering::Equal => {
                        stats.minimum_pattern_transforms += 1;
                        if key < best_key {
                            best_key = key;
                            best_transform = transform;
                        }
                    }
                    std::cmp::Ordering::Greater => unreachable!("prefix pruning must catch this"),
                }
            }
        }
    }
    CanonicalizationResult {
        key: CanonicalKey::Puzzle(best_key),
        witness: TransformWitness {
            spatial: best_transform,
            digits: mapping_for_transform(board, &best_transform),
        },
        stats,
    }
}

/// Compatibility wrapper for the pool CLI and callers of the earlier API.
/// The format remains an 81-character ASCII representation (`0` is blank).
pub fn canonical_key(board: &Matrix) -> String {
    match canonicalize_puzzle(board).key {
        CanonicalKey::Puzzle(key) => key.into_iter().map(|v| char::from(b'0' + v)).collect(),
        CanonicalKey::Solution { .. } => unreachable!(),
    }
}

/// Canonicalize a verified unique puzzle and its completion together.
/// `max_uniqueness_nodes` bounds the required uniqueness proof; exhaustion is
/// returned as an error and can never be mistaken for a valid key.
pub fn canonicalize_solution(
    puzzle: &Matrix,
    solution: &Matrix,
    max_uniqueness_nodes: u64,
) -> Result<CanonicalizationResult, CanonicalizationError> {
    if !valid_partial_board(puzzle) {
        return Err(CanonicalizationError::InvalidPuzzle);
    }
    if !valid_complete_solution(solution) {
        return Err(CanonicalizationError::InvalidSolution);
    }
    if puzzle
        .iter()
        .flatten()
        .zip(solution.iter().flatten())
        .any(|(&p, &s)| (1..=9).contains(&p) && p != s)
    {
        return Err(CanonicalizationError::SolutionDoesNotMatchClues);
    }
    let unique = check_uniqueness(puzzle, max_uniqueness_nodes)
        .map_err(|_| CanonicalizationError::InvalidPuzzle)?;
    let nodes = unique.explored_nodes;
    match unique.status {
        UniquenessStatus::Unique => {}
        UniquenessStatus::NoSolution => return Err(CanonicalizationError::NoSolution),
        UniquenessStatus::Multiple => return Err(CanonicalizationError::MultipleSolutions),
        UniquenessStatus::Unknown => {
            return Err(CanonicalizationError::UniquenessUnknown {
                explored_nodes: nodes,
            })
        }
    }

    let perms = grouped_permutations();
    let mut best_solution = [u8::MAX; 81];
    let mut best_clues = u128::MAX;
    let mut best_transform = identity_transform();
    let mut stats = CanonicalizationStats {
        uniqueness_nodes: nodes,
        ..Default::default()
    };
    for transpose in [false, true] {
        for &rows in perms {
            for &cols in perms {
                stats.transforms_considered += 1;
                let transform = SpatialTransform {
                    rows,
                    cols,
                    transpose,
                };
                let mut map = [0u8; 10];
                let mut next = 1u8;
                let mut normalized = [0u8; 81];
                let mut mask = 0u128;
                let mut prefix_order = std::cmp::Ordering::Equal;
                let mut pruned = false;
                for y in 0..9 {
                    for x in 0..9 {
                        let i = y * 9 + x;
                        let v = source_value(solution, &transform, y, x);
                        let d = &mut map[v as usize];
                        if *d == 0 {
                            *d = next;
                            next += 1;
                        }
                        normalized[i] = *d;
                        if best_solution[0] != u8::MAX && prefix_order == std::cmp::Ordering::Equal
                        {
                            match normalized[i].cmp(&best_solution[i]) {
                                std::cmp::Ordering::Greater => {
                                    stats.solution_prefix_prunes += 1;
                                    pruned = true;
                                    break;
                                }
                                std::cmp::Ordering::Less => prefix_order = std::cmp::Ordering::Less,
                                std::cmp::Ordering::Equal => {}
                            }
                        }
                        if (1..=9).contains(&source_value(puzzle, &transform, y, x)) {
                            mask |= clue_bit(y, x);
                        }
                    }
                    if pruned {
                        break;
                    }
                }
                if pruned {
                    continue;
                }
                if normalized < best_solution || (normalized == best_solution && mask < best_clues)
                {
                    best_solution = normalized;
                    best_clues = mask;
                    best_transform = transform;
                }
            }
        }
    }
    Ok(CanonicalizationResult {
        key: CanonicalKey::Solution {
            canonical_solution: best_solution,
            canonical_clues: best_clues,
        },
        witness: TransformWitness {
            spatial: best_transform,
            digits: mapping_for_transform(solution, &best_transform),
        },
        stats,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PairwiseStats {
    pub transforms_considered: u64,
    pub invariant_rejections: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EquivalenceResult {
    Equivalent {
        witness: TransformWitness,
        stats: PairwiseStats,
    },
    NotEquivalent {
        stats: PairwiseStats,
    },
    Unknown {
        stats: PairwiseStats,
    },
}

/// Independent direct checker. It applies each spatial transform and checks
/// clue positions and a bijective digit mapping without consulting canonical keys.
/// The budget counts fully attempted spatial transforms.
pub fn check_equivalent_pairwise(a: &Matrix, b: &Matrix, max_transforms: u64) -> EquivalenceResult {
    let mut stats = PairwiseStats::default();
    if !valid_partial_board(a) || !valid_partial_board(b) || !same_clue_count(a, b) {
        stats.invariant_rejections += 1;
        return EquivalenceResult::NotEquivalent { stats };
    }
    if invariant_signature(a) != invariant_signature(b) {
        stats.invariant_rejections += 1;
        return EquivalenceResult::NotEquivalent { stats };
    }
    for transpose in [false, true] {
        for rows in grouped_permutations() {
            for cols in grouped_permutations() {
                if stats.transforms_considered >= max_transforms {
                    return EquivalenceResult::Unknown { stats };
                }
                stats.transforms_considered += 1;
                let spatial = SpatialTransform {
                    rows: *rows,
                    cols: *cols,
                    transpose,
                };
                let mut forward = [0u8; 10];
                let mut reverse = [0u8; 10];
                let mut matches = true;
                for y in 0..9 {
                    for x in 0..9 {
                        // Deliberately spell out the mapping here instead of
                        // calling the canonicalizer's cell-access helper.
                        let av = if transpose {
                            a[cols[x] as usize][rows[y] as usize]
                        } else {
                            a[rows[y] as usize][cols[x] as usize]
                        };
                        let bv = b[y][x];
                        let ah = (1..=9).contains(&av);
                        let bh = (1..=9).contains(&bv);
                        if ah != bh {
                            matches = false;
                            break;
                        }
                        if ah {
                            let (xv, yv) = (&mut forward[av as usize], &mut reverse[bv as usize]);
                            if (*xv != 0 && *xv != bv) || (*yv != 0 && *yv != av) {
                                matches = false;
                                break;
                            }
                            *xv = bv;
                            *yv = av;
                        }
                    }
                    if !matches {
                        break;
                    }
                }
                if matches {
                    complete_mapping(&mut forward, &mut reverse);
                    let witness = TransformWitness {
                        spatial,
                        digits: DigitMapping(forward),
                    };
                    debug_assert_eq!(witness.apply(a), normalized_board(b));
                    return EquivalenceResult::Equivalent { witness, stats };
                }
            }
        }
    }
    EquivalenceResult::NotEquivalent { stats }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossCheckError {
    pub puzzle_a: Matrix,
    pub solution_a: Matrix,
    pub puzzle_b: Matrix,
    pub solution_b: Matrix,
    pub puzzle_minlex_equal: bool,
    pub solution_minlex_equal: bool,
    pub pairwise: EquivalenceResult,
    pub key_a: CanonicalKey,
    pub key_b: CanonicalKey,
    pub solution_key_a: CanonicalKey,
    pub solution_key_b: CanonicalKey,
    pub puzzle_stats_a: CanonicalizationStats,
    pub puzzle_stats_b: CanonicalizationStats,
    pub solution_stats_a: CanonicalizationStats,
    pub solution_stats_b: CanonicalizationStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CrossValidationError {
    InvalidInput(CanonicalizationError),
    PairwiseIncomplete(PairwiseStats),
    Mismatch(CrossCheckError),
}

pub fn canonicalize(
    method: CanonicalizationMethod,
    puzzle: &Matrix,
    solution: Option<&Matrix>,
    max_uniqueness_nodes: u64,
) -> Result<CanonicalizationResult, CanonicalizationError> {
    match method {
        CanonicalizationMethod::PuzzleMinLex => Ok(canonicalize_puzzle(puzzle)),
        CanonicalizationMethod::SolutionMinLex => canonicalize_solution(
            puzzle,
            solution.ok_or(CanonicalizationError::InvalidSolution)?,
            max_uniqueness_nodes,
        ),
    }
}

pub fn equivalent_by_canonical_key(
    method: CanonicalizationMethod,
    puzzle_a: &Matrix,
    solution_a: Option<&Matrix>,
    puzzle_b: &Matrix,
    solution_b: Option<&Matrix>,
    max_uniqueness_nodes: u64,
) -> Result<bool, CanonicalizationError> {
    let key_a = canonicalize(method, puzzle_a, solution_a, max_uniqueness_nodes)?.key;
    let key_b = canonicalize(method, puzzle_b, solution_b, max_uniqueness_nodes)?.key;
    Ok(key_a == key_b)
}

/// Compare independent equivalence decisions. Solution keys require unique
/// puzzles; any uniqueness/input errors are returned to the caller.
pub fn cross_validate(
    a: &Matrix,
    solution_a: &Matrix,
    b: &Matrix,
    solution_b: &Matrix,
    max_nodes: u64,
    max_pairwise_transforms: u64,
) -> Result<(), CrossValidationError> {
    let ka = canonicalize_puzzle(a);
    let kb = canonicalize_puzzle(b);
    let sa = canonicalize_solution(a, solution_a, max_nodes)
        .map_err(CrossValidationError::InvalidInput)?;
    let sb = canonicalize_solution(b, solution_b, max_nodes)
        .map_err(CrossValidationError::InvalidInput)?;
    let puzzle_equal = ka.key == kb.key;
    let solution_equal = sa.key == sb.key;
    let pairwise = check_equivalent_pairwise(a, b, max_pairwise_transforms);
    let pairwise_equal = match pairwise {
        EquivalenceResult::Equivalent { .. } => Some(true),
        EquivalenceResult::NotEquivalent { .. } => Some(false),
        EquivalenceResult::Unknown { .. } => None,
    };
    if pairwise_equal.is_none() {
        let stats = match pairwise {
            EquivalenceResult::Unknown { stats } => stats,
            _ => unreachable!(),
        };
        return Err(CrossValidationError::PairwiseIncomplete(stats));
    }
    if puzzle_equal == solution_equal && pairwise_equal == Some(puzzle_equal) {
        return Ok(());
    }
    Err(CrossValidationError::Mismatch(CrossCheckError {
        puzzle_a: *a,
        solution_a: *solution_a,
        puzzle_b: *b,
        solution_b: *solution_b,
        puzzle_minlex_equal: puzzle_equal,
        solution_minlex_equal: solution_equal,
        pairwise,
        key_a: ka.key,
        key_b: kb.key,
        solution_key_a: sa.key,
        solution_key_b: sb.key,
        puzzle_stats_a: ka.stats,
        puzzle_stats_b: kb.stats,
        solution_stats_a: sa.stats,
        solution_stats_b: sb.stats,
    }))
}

/// Bounded FIFO cache. Equality uses the full normalized 81-cell input and a
/// format version; hashes are only the HashMap's lookup accelerator.
#[derive(Debug)]
pub struct CanonicalizationCache {
    capacity: usize,
    entries: HashMap<(u8, [u8; 81]), CanonicalizationResult>,
    order: VecDeque<(u8, [u8; 81])>,
    pub hits: u64,
    pub misses: u64,
}

impl CanonicalizationCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
            hits: 0,
            misses: 0,
        }
    }
    pub fn canonicalize_puzzle(&mut self, board: &Matrix) -> CanonicalizationResult {
        let key = (CANONICAL_FORMAT_VERSION, flatten(&normalized_board(board)));
        if let Some(result) = self.entries.get(&key) {
            self.hits += 1;
            return result.clone();
        }
        self.misses += 1;
        let result = canonicalize_puzzle(board);
        if self.capacity > 0 {
            while self.entries.len() >= self.capacity {
                if let Some(old) = self.order.pop_front() {
                    self.entries.remove(&old);
                } else {
                    break;
                }
            }
            self.order.push_back(key);
            self.entries.insert(key, result.clone());
        }
        result
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn canonicalize_puzzle_batch<'a>(
    boards: impl IntoIterator<Item = &'a Matrix>,
    cache: &mut CanonicalizationCache,
) -> Vec<CanonicalizationResult> {
    boards
        .into_iter()
        .map(|board| cache.canonicalize_puzzle(board))
        .collect()
}

fn source_value(board: &Matrix, t: &SpatialTransform, y: usize, x: usize) -> u8 {
    if t.transpose {
        board[t.cols[x] as usize][t.rows[y] as usize]
    } else {
        board[t.rows[y] as usize][t.cols[x] as usize]
    }
}
fn clue_bit(y: usize, x: usize) -> u128 {
    1u128 << (80 - (y * 9 + x))
}
fn flatten(board: &Matrix) -> [u8; 81] {
    let mut out = [0; 81];
    for (i, v) in board.iter().flatten().enumerate() {
        out[i] = *v;
    }
    out
}
fn normalized_board(board: &Matrix) -> Matrix {
    let mut b = *board;
    for v in b.iter_mut().flatten() {
        if !(1..=9).contains(v) {
            *v = 0;
        }
    }
    b
}
fn same_clue_count(a: &Matrix, b: &Matrix) -> bool {
    a.iter()
        .flatten()
        .filter(|&&v| (1..=9).contains(&v))
        .count()
        == b.iter()
            .flatten()
            .filter(|&&v| (1..=9).contains(&v))
            .count()
}
fn invariant_signature(board: &Matrix) -> ([u8; 9], [u8; 9], [u8; 9], [u8; 9], [u8; 9], [u8; 9]) {
    let mut row_counts = [0u8; 9];
    let mut col_counts = [0u8; 9];
    let mut block_counts = [0u8; 9];
    let mut digit_counts = [0u8; 9];
    for y in 0..9 {
        for x in 0..9 {
            let v = board[y][x];
            if (1..=9).contains(&v) {
                row_counts[y] += 1;
                col_counts[x] += 1;
                block_counts[(y / 3) * 3 + x / 3] += 1;
                digit_counts[(v - 1) as usize] += 1;
            }
        }
    }
    let mut bands = [0u8; 9];
    let mut stacks = [0u8; 9];
    for i in 0..9 {
        bands[i / 3] += row_counts[i];
        stacks[i / 3] += col_counts[i];
    }
    row_counts.sort_unstable();
    col_counts.sort_unstable();
    bands.sort_unstable();
    stacks.sort_unstable();
    block_counts.sort_unstable();
    digit_counts.sort_unstable();
    // Transposition can swap these distributions, so sort each pair as a unit.
    let (axis_min, axis_max) = if row_counts <= col_counts {
        (row_counts, col_counts)
    } else {
        (col_counts, row_counts)
    };
    let (group_min, group_max) = if bands <= stacks {
        (bands, stacks)
    } else {
        (stacks, bands)
    };
    (
        axis_min,
        axis_max,
        group_min,
        group_max,
        block_counts,
        digit_counts,
    )
}
fn valid_partial_board(board: &Matrix) -> bool {
    for y in 0..9 {
        for x in 0..9 {
            let v = board[y][x];
            if !(1..=9).contains(&v) {
                continue;
            }
            for i in 0..9 {
                if (i != x && board[y][i] == v) || (i != y && board[i][x] == v) {
                    return false;
                }
            }
            for by in (y / 3) * 3..(y / 3) * 3 + 3 {
                for bx in (x / 3) * 3..(x / 3) * 3 + 3 {
                    if (by != y || bx != x) && board[by][bx] == v {
                        return false;
                    }
                }
            }
        }
    }
    true
}
fn valid_complete_solution(board: &Matrix) -> bool {
    if board.iter().flatten().any(|v| !(1..=9).contains(v)) || !valid_partial_board(board) {
        return false;
    }
    true
}
fn mapping_for_transform(board: &Matrix, t: &SpatialTransform) -> DigitMapping {
    let mut map = [0u8; 10];
    let mut next = 1u8;
    for y in 0..9 {
        for x in 0..9 {
            let v = source_value(board, t, y, x);
            if (1..=9).contains(&v) && map[v as usize] == 0 {
                map[v as usize] = next;
                next += 1;
            }
        }
    }
    let available: Vec<u8> = (1..=9)
        .filter(|&d| !map.contains(&(d as u8)))
        .map(|d| d as u8)
        .collect();
    let mut next_available = 0;
    for v in 1..=9 {
        if map[v] == 0 {
            map[v] = available[next_available];
            next_available += 1;
        }
    }
    DigitMapping(map)
}
fn complete_mapping(forward: &mut [u8; 10], reverse: &mut [u8; 10]) {
    let targets: Vec<u8> = (1..=9)
        .filter(|&v| reverse[v] == 0)
        .map(|v| v as u8)
        .collect();
    let mut next_target = 0;
    for source in 1..=9 {
        if forward[source] == 0 {
            let target = targets[next_target];
            next_target += 1;
            forward[source] = target;
            reverse[target as usize] = source as u8;
        }
    }
}
fn identity_transform() -> SpatialTransform {
    let id = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    SpatialTransform {
        rows: id,
        cols: id,
        transpose: false,
    }
}

fn grouped_permutations() -> &'static [[u8; 9]] {
    static PERMS: OnceLock<Vec<[u8; 9]>> = OnceLock::new();
    PERMS.get_or_init(|| {
        const T: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut out = Vec::with_capacity(1296);
        for groups in T {
            for a in T {
                for b in T {
                    for c in T {
                        let within = [a, b, c];
                        let mut p = [0u8; 9];
                        for g in 0..3 {
                            for i in 0..3 {
                                p[g * 3 + i] = (groups[g] * 3 + within[g][i]) as u8;
                            }
                        }
                        out.push(p);
                    }
                }
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUZZLE: Matrix = [
        [0, 0, 3, 0, 8, 0, 0, 1, 0],
        [4, 0, 0, 0, 0, 0, 0, 0, 6],
        [0, 2, 0, 9, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 5, 0, 4, 0, 7],
        [0, 6, 8, 0, 0, 0, 3, 0, 0],
        [7, 0, 1, 0, 9, 0, 0, 0, 0],
        [3, 0, 0, 4, 0, 0, 2, 0, 0],
        [0, 5, 0, 1, 0, 0, 0, 0, 0],
        [0, 0, 0, 8, 0, 6, 0, 0, 0],
    ];
    #[test]
    fn transforms_are_exact_and_keys_ignore_spatial_and_digit_symmetry() {
        let a = canonicalize_puzzle(&PUZZLE);
        let t = SpatialTransform {
            rows: [3, 4, 5, 0, 1, 2, 6, 7, 8],
            cols: [1, 2, 0, 4, 5, 3, 7, 8, 6],
            transpose: true,
        };
        let mut changed = t.apply(&PUZZLE);
        for v in changed.iter_mut().flatten() {
            if (1..=9).contains(v) {
                *v = 10 - *v;
            }
        }
        assert_eq!(a.key, canonicalize_puzzle(&changed).key);
        assert_eq!(a.witness.apply(&PUZZLE), as_board(&a.key));
        assert_eq!(grouped_permutations().len(), 1296);
        let labels: Matrix = std::array::from_fn(|y| std::array::from_fn(|x| (y * 9 + x) as u8));
        assert_eq!(t.inverse().apply(&t.apply(&labels)), labels);
        let id = t.compose(&t.inverse());
        assert_eq!(id.apply(&labels), labels);
        let t2 = SpatialTransform {
            rows: [2, 1, 0, 3, 5, 4, 6, 8, 7],
            cols: [0, 2, 1, 4, 3, 5, 8, 6, 7],
            transpose: false,
        };
        assert_eq!(t.compose(&t2).apply(&labels), t.apply(&t2.apply(&labels)));
    }

    #[test]
    fn pairwise_returns_a_replayable_witness_and_budget_is_unknown() {
        let transform = SpatialTransform {
            rows: [3, 4, 5, 0, 1, 2, 6, 7, 8],
            cols: [1, 2, 0, 4, 5, 3, 7, 8, 6],
            transpose: true,
        };
        let mut changed = transform.apply(&PUZZLE);
        for v in changed.iter_mut().flatten() {
            if (1..=9).contains(v) {
                *v = 10 - *v;
            }
        }
        let result = check_equivalent_pairwise(&PUZZLE, &changed, u64::MAX);
        match result {
            EquivalenceResult::Equivalent { witness, .. } => {
                assert_eq!(witness.apply(&PUZZLE), changed)
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            check_equivalent_pairwise(&PUZZLE, &changed, 0),
            EquivalenceResult::Unknown { .. }
        ));
    }

    #[test]
    fn pairwise_invariants_reject_a_different_clue_pattern() {
        let mut same_row = [[0u8; 9]; 9];
        same_row[0][0] = 1;
        same_row[0][8] = 2;
        let mut same_box = [[0u8; 9]; 9];
        same_box[0][0] = 1;
        same_box[1][1] = 2;
        match check_equivalent_pairwise(&same_row, &same_box, 1) {
            EquivalenceResult::NotEquivalent { stats } => {
                assert_eq!(stats.invariant_rejections, 1);
                assert_eq!(stats.transforms_considered, 0);
            }
            other => panic!("{other:?}"),
        }
        assert_ne!(
            canonicalize_puzzle(&same_row).key,
            canonicalize_puzzle(&same_box).key
        );
    }

    #[test]
    fn cache_uses_full_inputs_and_reports_hits() {
        let mut cache = CanonicalizationCache::new(2);
        let first = cache.canonicalize_puzzle(&PUZZLE);
        assert_eq!(cache.canonicalize_puzzle(&PUZZLE), first);
        assert_eq!((cache.hits, cache.misses), (1, 1));
    }

    #[test]
    fn solution_minlex_validates_unique_completion_and_is_invariant() {
        let solution =
            std::array::from_fn(|y| std::array::from_fn(|x| ((y * 3 + y / 3 + x) % 9 + 1) as u8));
        let mut puzzle = solution;
        puzzle[0][0] = 0;
        let key = canonicalize_solution(&puzzle, &solution, 10_000)
            .unwrap()
            .key;
        let transform = SpatialTransform {
            rows: [3, 4, 5, 0, 1, 2, 6, 7, 8],
            cols: [1, 2, 0, 4, 5, 3, 7, 8, 6],
            transpose: true,
        };
        let mut p2 = transform.apply(&puzzle);
        let mut s2 = transform.apply(&solution);
        for v in p2.iter_mut().flatten().chain(s2.iter_mut().flatten()) {
            if (1..=9).contains(v) {
                *v = 10 - *v;
            }
        }
        assert_eq!(key, canonicalize_solution(&p2, &s2, 10_000).unwrap().key);
        assert_eq!(
            cross_validate(&puzzle, &solution, &p2, &s2, 10_000, u64::MAX),
            Ok(())
        );
    }

    fn as_board(key: &CanonicalKey) -> Matrix {
        match key {
            CanonicalKey::Puzzle(k) => {
                let mut b = [[0; 9]; 9];
                for i in 0..81 {
                    b[i / 9][i % 9] = k[i];
                }
                b
            }
            _ => unreachable!(),
        }
    }
}
