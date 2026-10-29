//! Solution-space pivot search for verified 17-clue Sudoku puzzles.
//!
//! A pivot removes one parent clue, studies the resulting 16-clue solution
//! space, and adds a clue that selects one completion. Every candidate is
//! checked directly for uniqueness; independent checks can run in parallel.

use crate::symmetry::canonical_key;
use crate::{Matrix, SearchError, SearchEvent, SolutionSearch, UniquenessCheck, UniquenessStatus};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PARENT_CLUE_COUNT: usize = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PivotMode {
    Random,
    Exhaustive,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PivotCandidateMode {
    /// Enumerate the 16-clue solution space and use singleton frequencies.
    #[default]
    Frequency,
    /// Check each legal added clue with a bounded uniqueness query.
    Direct,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    SolutionLimit,
    NodeLimit,
    Cancelled,
    TimeLimit,
    CandidateLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionSpaceKind {
    NotEnumerated,
    ExactZero,
    ExactOne,
    ExactTwo,
    ExactMany,
    Partial(StopReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PivotPhase {
    Enumerating,
    CheckingPivots,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PivotProgress {
    pub removed_position: usize,
    pub phase: PivotPhase,
    pub solutions_found: u64,
    pub explored_nodes: u64,
    pub candidate_checks: u64,
}

/// Frequency table over all solutions found for one 16-clue board.
/// `witnesses[cell][digit]` is present only while the sampled frequency is 1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionFrequencies {
    /// Saturated occurrence count: 0, 1, or 2 (meaning two or more).
    pub counts: [[u8; 9]; 81],
    pub total: u64,
    witnesses: [[Option<Matrix>; 9]; 81],
}

impl Default for SolutionFrequencies {
    fn default() -> Self {
        Self {
            counts: [[0; 9]; 81],
            total: 0,
            witnesses: std::array::from_fn(|_| std::array::from_fn(|_| None)),
        }
    }
}

impl SolutionFrequencies {
    fn observe(&mut self, solution: Matrix) {
        for (position, &digit) in solution.iter().flatten().enumerate() {
            let count = &mut self.counts[position][digit as usize - 1];
            if *count == 0 {
                self.witnesses[position][digit as usize - 1] = Some(solution);
            } else if *count == 1 {
                self.witnesses[position][digit as usize - 1] = None;
            }
            *count = count.saturating_add(1).min(2);
        }
        self.total = self.total.saturating_add(1);
    }

    fn singleton_candidates(&self, board: &Matrix) -> Vec<PivotSeed> {
        let mut candidates = Vec::new();
        for position in 0..81 {
            if (1..=9).contains(&board[position / 9][position % 9]) {
                continue;
            }
            for digit in 1..=9 {
                if self.counts[position][digit - 1] == 1 {
                    if let Some(solution) = self.witnesses[position][digit - 1] {
                        candidates.push(PivotSeed {
                            position,
                            digit: digit as u8,
                            solution,
                        });
                    }
                }
            }
        }
        candidates
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionSpaceSummary {
    pub kind: SolutionSpaceKind,
    pub frequencies: SolutionFrequencies,
    pub explored_nodes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PivotError {
    InvalidOptions,
    InvalidParent,
    ParentUniquenessUnknown { explored_nodes: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PivotOptions {
    pub mode: PivotMode,
    pub candidate_mode: PivotCandidateMode,
    /// Maximum number of native workers used for independent clue deletions.
    /// WebAssembly builds default to one worker.
    pub threads: usize,
    /// At most this many solutions are retained; one additional solution may
    /// be observed to prove that the configured limit was exceeded.
    pub max_solutions: u64,
    pub max_nodes_per_search: u64,
    pub max_nodes_per_candidate_check: u64,
    pub max_candidate_checks: u64,
    pub seed: u64,
    pub prioritize_different_solution: bool,
    pub max_duration: Option<Duration>,
}

impl Default for PivotOptions {
    fn default() -> Self {
        Self {
            mode: PivotMode::Random,
            candidate_mode: PivotCandidateMode::Direct,
            threads: default_pivot_threads(),
            max_solutions: 256,
            max_nodes_per_search: 100_000,
            max_nodes_per_candidate_check: 100_000,
            max_candidate_checks: 1_088,
            seed: 1,
            prioritize_different_solution: true,
            max_duration: None,
        }
    }
}

fn default_pivot_threads() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        (effective_parallelism() / 2).max(1)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn effective_parallelism() -> usize {
    let host_count = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);

    #[cfg(target_os = "linux")]
    if let Ok(cpu_max) = std::fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        let mut fields = cpu_max.split_whitespace();
        if let (Some(quota), Some(period)) = (fields.next(), fields.next()) {
            if quota != "max" {
                if let (Ok(quota), Ok(period)) = (quota.parse::<u64>(), period.parse::<u64>()) {
                    if period > 0 {
                        let quota_count = quota.div_ceil(period).max(1) as usize;
                        return host_count.min(quota_count);
                    }
                }
            }
        }
    }

    host_count
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PivotCandidate {
    pub puzzle: Matrix,
    pub solution: Matrix,
    pub removed_position: usize,
    pub pivot_position: usize,
    pub exact_frequency_proof: bool,
    pub different_solution: bool,
    pub different_solution_class: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PivotDeletionStats {
    pub removed_position: usize,
    pub solution_space_kind: SolutionSpaceKind,
    pub solutions_found: u64,
    pub enumeration_nodes: u64,
    pub candidate_proof_nodes: u64,
    pub elapsed_micros: u64,
    pub pivot_candidates: u64,
    pub candidate_budget: u64,
    pub candidate_budget_exhausted: bool,
    pub multiple_candidates: u64,
    pub unknown_candidates: u64,
    pub no_solution_candidates: u64,
    pub witness_pruned_candidates: u64,
    pub unique_candidates: u64,
    pub new_canonical_classes: u64,
    pub cache_hit: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PivotSearchReport {
    pub mode: PivotMode,
    pub candidate_mode: PivotCandidateMode,
    pub candidates: Vec<PivotCandidate>,
    pub deletions: Vec<PivotDeletionStats>,
    pub parent_uniqueness_checked: bool,
    pub parent_verification_nodes: u64,
    pub explored_nodes: u64,
    pub candidate_checks: u64,
    pub cache_hits: u64,
    pub solution_spaces_complete: bool,
    pub complete: bool,
    pub stop_reason: Option<StopReason>,
}

#[derive(Clone, Copy)]
struct PivotSeed {
    position: usize,
    digit: u8,
    solution: Matrix,
}

#[derive(Clone, Copy)]
struct UniqueSolutionCheck {
    status: UniquenessStatus,
    solution: Option<Matrix>,
    alternative: Option<Matrix>,
    explored_nodes: u64,
}

/// Bounded FIFO cache of exact-board solution-space summaries.
pub struct PivotCache {
    capacity: usize,
    entries: HashMap<[u8; 81], SolutionSpaceSummary>,
    order: VecDeque<[u8; 81]>,
}

impl PivotCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn get(&self, board: &Matrix) -> Option<SolutionSpaceSummary> {
        self.entries.get(&board_key(board)).cloned()
    }

    fn insert(&mut self, board: &Matrix, summary: SolutionSpaceSummary) {
        if self.capacity == 0 {
            return;
        }
        let key = board_key(board);
        if self.entries.contains_key(&key) {
            self.entries.insert(key, summary);
            return;
        }
        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
        self.order.push_back(key);
        self.entries.insert(key, summary);
    }
}

impl Default for PivotCache {
    fn default() -> Self {
        Self::new(32)
    }
}

/// Enumerate the solution space up to a solution and node budget.
/// A partial sample is never reported as an exact solution count.
pub fn enumerate_solution_space(
    board: &Matrix,
    max_solutions: u64,
    max_nodes: u64,
    cancelled: &AtomicBool,
) -> Result<SolutionSpaceSummary, PivotError> {
    let mut on_progress = |_| {};
    enumerate_solution_space_until(
        board,
        max_solutions,
        max_nodes,
        cancelled,
        None,
        0,
        0,
        0,
        &mut on_progress,
    )
}

fn enumerate_solution_space_until(
    board: &Matrix,
    max_solutions: u64,
    max_nodes: u64,
    cancelled: &AtomicBool,
    started: Option<(Instant, Duration)>,
    removed_position: usize,
    node_offset: u64,
    candidate_checks: u64,
    on_progress: &mut dyn FnMut(PivotProgress),
) -> Result<SolutionSpaceSummary, PivotError> {
    if max_solutions == 0 || max_nodes == 0 {
        return Err(PivotError::InvalidOptions);
    }
    let mut search = SolutionSearch::new(board).map_err(|_| PivotError::InvalidParent)?;
    let mut frequencies = SolutionFrequencies::default();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(SolutionSpaceSummary {
                kind: SolutionSpaceKind::Partial(StopReason::Cancelled),
                frequencies,
                explored_nodes: search.explored_nodes(),
            });
        }
        if let Some((start, limit)) = started {
            if start.elapsed() >= limit {
                return Ok(SolutionSpaceSummary {
                    kind: SolutionSpaceKind::Partial(StopReason::TimeLimit),
                    frequencies,
                    explored_nodes: search.explored_nodes(),
                });
            }
        }
        if search.explored_nodes() >= max_nodes {
            return Ok(SolutionSpaceSummary {
                kind: SolutionSpaceKind::Partial(StopReason::NodeLimit),
                frequencies,
                explored_nodes: search.explored_nodes(),
            });
        }
        match search.next() {
            Some(SearchEvent::SolutionFound { board, .. }) => {
                frequencies.observe(board);
                if frequencies.total > max_solutions {
                    return Ok(SolutionSpaceSummary {
                        kind: SolutionSpaceKind::Partial(StopReason::SolutionLimit),
                        frequencies,
                        explored_nodes: search.explored_nodes(),
                    });
                }
            }
            Some(SearchEvent::Progress { .. }) => on_progress(PivotProgress {
                removed_position,
                phase: PivotPhase::Enumerating,
                solutions_found: frequencies.total,
                explored_nodes: node_offset.saturating_add(search.explored_nodes()),
                candidate_checks,
            }),
            None => {
                let kind = match frequencies.total {
                    0 => SolutionSpaceKind::ExactZero,
                    1 => SolutionSpaceKind::ExactOne,
                    2 => SolutionSpaceKind::ExactTwo,
                    _ => SolutionSpaceKind::ExactMany,
                };
                return Ok(SolutionSpaceSummary {
                    kind,
                    frequencies,
                    explored_nodes: search.explored_nodes(),
                });
            }
        }
    }
}

enum DeletionWorkerMessage {
    Progress {
        task_index: usize,
        progress: PivotProgress,
    },
    Done {
        task_index: usize,
        result_index: usize,
        result: Result<SolutionSpaceSummary, PivotError>,
        elapsed_micros: u64,
    },
}

/// Enumerate independent deletion neighborhoods with deterministic result
/// slots. Progress is forwarded on the calling thread; result processing is
/// later performed in the original seeded deletion order.
fn enumerate_deletion_batch(
    tasks: &[(usize, usize, Matrix)],
    result_count: usize,
    options: PivotOptions,
    started: Instant,
    node_offset: u64,
    cancelled: &AtomicBool,
    on_progress: &mut impl FnMut(PivotProgress),
) -> Result<Vec<Option<(SolutionSpaceSummary, u64)>>, PivotError> {
    let mut results: Vec<Option<(SolutionSpaceSummary, u64)>> =
        std::iter::repeat_with(|| None).take(result_count).collect();
    if tasks.is_empty() {
        return Ok(results);
    }

    #[cfg(target_arch = "wasm32")]
    let worker_count = 1;
    #[cfg(not(target_arch = "wasm32"))]
    let worker_count = options.threads.max(1).min(tasks.len());
    if worker_count == 1 {
        let mut completed_nodes = 0u64;
        for &(result_index, removed_position, board) in tasks {
            let task_started = Instant::now();
            let mut forward_progress = |progress: PivotProgress| {
                on_progress(PivotProgress {
                    explored_nodes: node_offset
                        .saturating_add(completed_nodes)
                        .saturating_add(progress.explored_nodes),
                    ..progress
                });
            };
            let summary = enumerate_solution_space_until(
                &board,
                options.max_solutions,
                options.max_nodes_per_search,
                cancelled,
                options.max_duration.map(|limit| (started, limit)),
                removed_position,
                0,
                0,
                &mut forward_progress,
            )?;
            let elapsed_micros = task_started.elapsed().as_micros() as u64;
            completed_nodes = completed_nodes.saturating_add(summary.explored_nodes);
            on_progress(PivotProgress {
                removed_position,
                phase: PivotPhase::Enumerating,
                solutions_found: summary.frequencies.total,
                explored_nodes: node_offset.saturating_add(completed_nodes),
                candidate_checks: 0,
            });
            results[result_index] = Some((summary, elapsed_micros));
        }
        return Ok(results);
    }

    let next_task = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    let mut task_nodes = vec![0u64; tasks.len()];
    let mut worker_error = None;

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let sender = sender.clone();
            let tasks = tasks;
            let next_task = &next_task;
            scope.spawn(move || loop {
                let task_index = next_task.fetch_add(1, Ordering::Relaxed);
                let Some(&(result_index, removed_position, board)) = tasks.get(task_index) else {
                    break;
                };
                let task_started = Instant::now();
                let mut send_progress = |progress| {
                    let _ = sender.send(DeletionWorkerMessage::Progress {
                        task_index,
                        progress,
                    });
                };
                let result = enumerate_solution_space_until(
                    &board,
                    options.max_solutions,
                    options.max_nodes_per_search,
                    cancelled,
                    options.max_duration.map(|limit| (started, limit)),
                    removed_position,
                    0,
                    0,
                    &mut send_progress,
                );
                let elapsed_micros = task_started.elapsed().as_micros() as u64;
                let _ = sender.send(DeletionWorkerMessage::Done {
                    task_index,
                    result_index,
                    result,
                    elapsed_micros,
                });
            });
        }
        drop(sender);

        for message in receiver {
            match message {
                DeletionWorkerMessage::Progress {
                    task_index,
                    progress,
                } => {
                    task_nodes[task_index] = task_nodes[task_index].max(progress.explored_nodes);
                    on_progress(PivotProgress {
                        explored_nodes: node_offset
                            .saturating_add(task_nodes.iter().copied().sum()),
                        ..progress
                    });
                }
                DeletionWorkerMessage::Done {
                    task_index,
                    result_index,
                    result,
                    elapsed_micros,
                } => match result {
                    Ok(summary) => {
                        task_nodes[task_index] = summary.explored_nodes;
                        on_progress(PivotProgress {
                            removed_position: tasks[task_index].1,
                            phase: PivotPhase::Enumerating,
                            solutions_found: summary.frequencies.total,
                            explored_nodes: node_offset
                                .saturating_add(task_nodes.iter().copied().sum()),
                            candidate_checks: 0,
                        });
                        results[result_index] = Some((summary, elapsed_micros));
                    }
                    Err(error) => worker_error = Some(error),
                },
            }
        }
    });

    if let Some(error) = worker_error {
        return Err(error);
    }
    Ok(results)
}

fn check_uniqueness_cancellable(
    board: &Matrix,
    max_nodes: u64,
    cancelled: &AtomicBool,
    deadline: Option<(Instant, Duration)>,
) -> Result<UniquenessCheck, SearchError> {
    let mut search = SolutionSearch::new(board)?;
    loop {
        if cancelled.load(Ordering::Relaxed)
            || deadline.is_some_and(|(start, limit)| start.elapsed() >= limit)
            || search.explored_nodes() >= max_nodes
        {
            return Ok(UniquenessCheck {
                status: UniquenessStatus::Unknown,
                solutions_found: search.solutions_found(),
                explored_nodes: search.explored_nodes(),
            });
        }
        match search.next() {
            Some(SearchEvent::SolutionFound { .. }) if search.solutions_found() >= 2 => {
                return Ok(UniquenessCheck {
                    status: UniquenessStatus::Multiple,
                    solutions_found: search.solutions_found(),
                    explored_nodes: search.explored_nodes(),
                });
            }
            Some(SearchEvent::SolutionFound { .. } | SearchEvent::Progress { .. }) => {}
            None => {
                return Ok(UniquenessCheck {
                    status: if search.solutions_found() == 1 {
                        UniquenessStatus::Unique
                    } else {
                        UniquenessStatus::NoSolution
                    },
                    solutions_found: search.solutions_found(),
                    explored_nodes: search.explored_nodes(),
                });
            }
        }
    }
}

fn parallel_unique_checks(
    candidates: &[(PivotSeed, Matrix)],
    requested_threads: usize,
    max_nodes: u64,
    cancelled: &AtomicBool,
    deadline: Option<(Instant, Duration)>,
) -> HashMap<(usize, u8), UniqueSolutionCheck> {
    if candidates.is_empty() {
        return HashMap::new();
    }
    #[cfg(target_arch = "wasm32")]
    let worker_count = {
        let _ = requested_threads;
        1
    };
    #[cfg(not(target_arch = "wasm32"))]
    let worker_count = requested_threads.max(1).min(candidates.len());

    let mut ordered_results = vec![None; candidates.len()];
    if worker_count == 1 {
        for (index, (_, puzzle)) in candidates.iter().enumerate() {
            ordered_results[index] =
                check_unique_solution_cancellable(puzzle, max_nodes, cancelled, deadline).ok();
        }
    } else {
        let next_candidate = AtomicUsize::new(0);
        let (sender, receiver) = mpsc::channel();
        std::thread::scope(|scope| {
            for _ in 0..worker_count {
                let sender = sender.clone();
                let next_candidate = &next_candidate;
                scope.spawn(move || loop {
                    let index = next_candidate.fetch_add(1, Ordering::Relaxed);
                    let Some((_, puzzle)) = candidates.get(index) else {
                        break;
                    };
                    let check =
                        check_unique_solution_cancellable(puzzle, max_nodes, cancelled, deadline)
                            .ok();
                    if sender.send((index, check)).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            for (index, check) in receiver {
                ordered_results[index] = check;
            }
        });
    }

    candidates
        .iter()
        .zip(ordered_results)
        .filter_map(|((pivot, _), result)| {
            result.map(|check| ((pivot.position, pivot.digit), check))
        })
        .collect()
}

fn check_unique_solution_cancellable(
    board: &Matrix,
    max_nodes: u64,
    cancelled: &AtomicBool,
    deadline: Option<(Instant, Duration)>,
) -> Result<UniqueSolutionCheck, SearchError> {
    let mut search = SolutionSearch::new(board)?;
    let mut first_solution = None;
    loop {
        if cancelled.load(Ordering::Relaxed)
            || deadline.is_some_and(|(start, limit)| start.elapsed() >= limit)
            || search.explored_nodes() >= max_nodes
        {
            return Ok(UniqueSolutionCheck {
                status: UniquenessStatus::Unknown,
                solution: first_solution,
                alternative: None,
                explored_nodes: search.explored_nodes(),
            });
        }
        match search.next() {
            Some(SearchEvent::SolutionFound { board, .. }) => {
                if first_solution.is_some() {
                    return Ok(UniqueSolutionCheck {
                        status: UniquenessStatus::Multiple,
                        solution: first_solution,
                        alternative: Some(board),
                        explored_nodes: search.explored_nodes(),
                    });
                }
                first_solution = Some(board);
            }
            Some(SearchEvent::Progress { .. }) => {}
            None => {
                return Ok(UniqueSolutionCheck {
                    status: if first_solution.is_some() {
                        UniquenessStatus::Unique
                    } else {
                        UniquenessStatus::NoSolution
                    },
                    solution: first_solution,
                    alternative: None,
                    explored_nodes: search.explored_nodes(),
                });
            }
        }
    }
}

/// Explore one or all clue deletions from a verified 17-clue parent.
/// `cancelled` is checked during solution enumeration and between pivot checks.
pub fn search_solution_space_pivots(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    options: PivotOptions,
    cancelled: &AtomicBool,
    cache: &mut PivotCache,
) -> Result<PivotSearchReport, PivotError> {
    search_solution_space_pivots_with_progress(
        seed,
        parent,
        parent_solution,
        options,
        cancelled,
        cache,
        |_| {},
    )
}

pub fn search_solution_space_pivots_with_progress(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    options: PivotOptions,
    cancelled: &AtomicBool,
    cache: &mut PivotCache,
    on_progress: impl FnMut(PivotProgress),
) -> Result<PivotSearchReport, PivotError> {
    search_solution_space_pivots_impl(
        seed,
        parent,
        parent_solution,
        options,
        cancelled,
        cache,
        on_progress,
        true,
    )
}

/// Search from a parent whose uniqueness has already been independently
/// verified by the caller. The completed solution and clue consistency are
/// still checked here; this avoids repeating an expensive uniqueness proof in
/// pool collection where every parent record was validated on load.
pub fn search_solution_space_pivots_with_progress_verified_parent(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    options: PivotOptions,
    cancelled: &AtomicBool,
    cache: &mut PivotCache,
    on_progress: impl FnMut(PivotProgress),
) -> Result<PivotSearchReport, PivotError> {
    search_solution_space_pivots_impl(
        seed,
        parent,
        parent_solution,
        options,
        cancelled,
        cache,
        on_progress,
        false,
    )
}

fn search_solution_space_pivots_impl(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    options: PivotOptions,
    cancelled: &AtomicBool,
    cache: &mut PivotCache,
    mut on_progress: impl FnMut(PivotProgress),
    verify_parent_uniqueness: bool,
) -> Result<PivotSearchReport, PivotError> {
    if options.candidate_mode != PivotCandidateMode::Direct
        || options.max_solutions == 0
        || options.max_nodes_per_search == 0
        || options.max_nodes_per_candidate_check == 0
        || options.max_candidate_checks == 0
        || options.threads == 0
        || options
            .max_duration
            .is_some_and(|duration| duration.is_zero())
    {
        return Err(PivotError::InvalidOptions);
    }
    let parent_values_valid = parent
        .iter()
        .flatten()
        .all(|&value| value == 0 || (1..=9).contains(&value));
    let parent_clues = parent
        .iter()
        .flatten()
        .filter(|&&v| (1..=9).contains(&v))
        .count();
    if !parent_values_valid
        || parent_clues != PARENT_CLUE_COUNT
        || !is_complete_solution(&parent_solution)
        || !solution_matches_clues(&parent_solution, &parent)
    {
        return Err(PivotError::InvalidParent);
    }
    let started = Instant::now();
    let parent_check = if verify_parent_uniqueness {
        let check = check_uniqueness_cancellable(
            &parent,
            options.max_nodes_per_candidate_check,
            cancelled,
            options.max_duration.map(|limit| (started, limit)),
        )
        .map_err(|_| PivotError::InvalidParent)?;
        if check.status == UniquenessStatus::Unknown {
            return Err(PivotError::ParentUniquenessUnknown {
                explored_nodes: check.explored_nodes,
            });
        }
        if check.status != UniquenessStatus::Unique {
            return Err(PivotError::InvalidParent);
        }
        Some(check)
    } else {
        None
    };

    let parent_puzzle_key = canonical_key(&parent);
    let parent_solution_key = canonical_key(&parent_solution);
    let mut rng = PivotRng(seed);
    let mut removals: Vec<_> = (0..81)
        .filter(|&position| (1..=9).contains(&parent[position / 9][position % 9]))
        .collect();
    rng.shuffle(&mut removals);
    if options.mode == PivotMode::Random {
        removals.truncate(1);
    }

    let mut report = PivotSearchReport {
        mode: options.mode,
        candidate_mode: options.candidate_mode,
        candidates: Vec::new(),
        deletions: Vec::with_capacity(removals.len()),
        parent_uniqueness_checked: parent_check.is_some(),
        parent_verification_nodes: parent_check.map_or(0, |check| check.explored_nodes),
        explored_nodes: parent_check.map_or(0, |check| check.explored_nodes),
        candidate_checks: 0,
        cache_hits: 0,
        solution_spaces_complete: options.candidate_mode == PivotCandidateMode::Frequency,
        complete: true,
        stop_reason: None,
    };
    let mut seen_classes = HashSet::from([parent_puzzle_key]);
    let mut rejected_classes = HashSet::new();

    let mut prepared: Vec<Option<(SolutionSpaceSummary, bool, u64)>> =
        std::iter::repeat_with(|| None)
            .take(removals.len())
            .collect();
    let mut pending = Vec::new();
    for (index, &removed) in removals.iter().enumerate() {
        let mut reduced = parent;
        reduced[removed / 9][removed % 9] = 0;
        if options.candidate_mode == PivotCandidateMode::Direct {
            prepared[index] = Some((
                SolutionSpaceSummary {
                    kind: SolutionSpaceKind::NotEnumerated,
                    frequencies: SolutionFrequencies::default(),
                    explored_nodes: 0,
                },
                false,
                0,
            ));
        } else if let Some(cached) = cache.get(&reduced) {
            prepared[index] = Some((cached, true, 0));
        } else {
            pending.push((index, removed, reduced));
        }
    }
    let batch = if options.candidate_mode == PivotCandidateMode::Frequency {
        enumerate_deletion_batch(
            &pending,
            removals.len(),
            options,
            started,
            report.explored_nodes,
            cancelled,
            &mut on_progress,
        )?
    } else {
        Vec::new()
    };
    for (index, result) in batch.into_iter().enumerate() {
        if let Some((summary, elapsed_micros)) = result {
            prepared[index] = Some((summary, false, elapsed_micros));
        }
    }

    let total_deletions = removals.len();
    for (index, removed) in removals.into_iter().enumerate() {
        let deletion_started = Instant::now();
        let candidate_budget_start = report.candidate_checks;
        let remaining_candidate_budget = options
            .max_candidate_checks
            .saturating_sub(candidate_budget_start);
        let deletions_remaining = (total_deletions - index) as u64;
        let deletion_candidate_budget = per_deletion_candidate_budget(
            options.mode,
            remaining_candidate_budget,
            deletions_remaining,
        );
        let deletion_candidate_limit =
            candidate_budget_start.saturating_add(deletion_candidate_budget);
        let mut reduced = parent;
        reduced[removed / 9][removed % 9] = 0;
        let (summary, cache_hit, enumeration_elapsed_micros) = prepared[index]
            .take()
            .expect("every requested deletion must have an enumeration result");
        if options.candidate_mode == PivotCandidateMode::Frequency
            && !cache_hit
            && !matches!(summary.kind, SolutionSpaceKind::Partial(_))
        {
            cache.insert(&reduced, summary.clone());
        }
        if cache_hit {
            report.cache_hits += 1;
        }
        let enumeration_nodes = if cache_hit { 0 } else { summary.explored_nodes };
        report.explored_nodes = report.explored_nodes.saturating_add(enumeration_nodes);
        let complete_enumeration = matches!(
            summary.kind,
            SolutionSpaceKind::ExactZero
                | SolutionSpaceKind::ExactOne
                | SolutionSpaceKind::ExactTwo
                | SolutionSpaceKind::ExactMany
        );
        if options.candidate_mode == PivotCandidateMode::Frequency {
            on_progress(PivotProgress {
                removed_position: removed,
                phase: PivotPhase::Enumerating,
                solutions_found: summary.frequencies.total,
                explored_nodes: report.explored_nodes,
                candidate_checks: report.candidate_checks,
            });
        }
        if matches!(summary.kind, SolutionSpaceKind::Partial(_)) {
            report.solution_spaces_complete = false;
            report.complete = false;
            if let SolutionSpaceKind::Partial(
                reason @ (StopReason::Cancelled | StopReason::TimeLimit),
            ) = summary.kind
            {
                report.stop_reason = Some(reason);
            }
        }
        let mut deletion = PivotDeletionStats {
            removed_position: removed,
            solution_space_kind: summary.kind,
            solutions_found: summary.frequencies.total,
            enumeration_nodes,
            candidate_proof_nodes: 0,
            elapsed_micros: 0,
            pivot_candidates: 0,
            candidate_budget: deletion_candidate_budget,
            candidate_budget_exhausted: false,
            multiple_candidates: 0,
            unknown_candidates: 0,
            no_solution_candidates: 0,
            witness_pruned_candidates: 0,
            unique_candidates: 0,
            new_canonical_classes: 0,
            cache_hit,
        };
        let mut pivots = if options.candidate_mode == PivotCandidateMode::Frequency {
            summary.frequencies.singleton_candidates(&reduced)
        } else {
            (0..81)
                .filter(|&position| !(1..=9).contains(&reduced[position / 9][position % 9]))
                .flat_map(|position| {
                    crate::candidates_for(&reduced, position % 9, position / 9)
                        .into_iter()
                        .map(move |digit| PivotSeed {
                            position,
                            digit,
                            // Direct probes replace this placeholder with the proved solution.
                            solution: parent_solution,
                        })
                })
                .collect()
        };
        match options.candidate_mode {
            PivotCandidateMode::Frequency if options.prioritize_different_solution => {
                pivots.sort_by_key(|pivot| {
                    let different_solution = pivot.solution != parent_solution;
                    let different_class = canonical_key(&pivot.solution) != parent_solution_key;
                    (!different_class, !different_solution)
                });
            }
            PivotCandidateMode::Direct => rng.shuffle(&mut pivots),
            _ => {}
        }

        // Checks run in bounded batches so a counterexample found in one
        // batch can prune candidates before the next batch is dispatched.
        let batch_size = options.threads.max(1);
        let mut rejected_by_witness = HashSet::new();
        'pivots: for pivot_batch in pivots.chunks(batch_size) {
            // Candidate checks are independent. Merge results by the seeded
            // candidate order so thread scheduling cannot change the report.
            let direct_checks =
                if options.candidate_mode == PivotCandidateMode::Direct && options.threads > 1 {
                    let mut checks = Vec::new();
                    let mut planned_classes = seen_classes.clone();
                    planned_classes.extend(rejected_classes.iter().cloned());
                    let remaining_batch_budget =
                        deletion_candidate_limit.saturating_sub(report.candidate_checks);
                    for pivot in pivot_batch {
                        let mut candidate = reduced;
                        candidate[pivot.position / 9][pivot.position % 9] = pivot.digit;
                        if candidate == parent {
                            continue;
                        }
                        if rejected_by_witness.contains(&(pivot.position, pivot.digit)) {
                            continue;
                        }
                        let key = canonical_key(&candidate);
                        if !planned_classes.insert(key) {
                            continue;
                        }
                        if checks.len() as u64 >= remaining_batch_budget {
                            break;
                        }
                        checks.push((*pivot, candidate));
                    }
                    parallel_unique_checks(
                        &checks,
                        options.threads,
                        options.max_nodes_per_candidate_check,
                        cancelled,
                        options.max_duration.map(|limit| (started, limit)),
                    )
                } else {
                    HashMap::new()
                };

            // A counterexample solution satisfies the 16-clue core. Any pivot
            // that adds the same value at one of its empty cells is therefore
            // known to remain multiple, regardless of candidate-generation mode.
            let mut batch_counterexamples = Vec::new();
            for pivot in pivot_batch.iter().copied() {
                if rejected_by_witness.remove(&(pivot.position, pivot.digit)) {
                    deletion.witness_pruned_candidates += 1;
                    continue;
                }
                if report.stop_reason.is_some() {
                    break 'pivots;
                }
                if cancelled.load(Ordering::Relaxed) {
                    report.complete = false;
                    report.stop_reason = Some(StopReason::Cancelled);
                    break 'pivots;
                }
                if options
                    .max_duration
                    .is_some_and(|limit| started.elapsed() >= limit)
                {
                    report.complete = false;
                    report.stop_reason = Some(StopReason::TimeLimit);
                    break 'pivots;
                }
                let mut candidate = reduced;
                candidate[pivot.position / 9][pivot.position % 9] = pivot.digit;
                if candidate == parent {
                    continue;
                }
                let puzzle_key = canonical_key(&candidate);
                if seen_classes.contains(&puzzle_key) || rejected_classes.contains(&puzzle_key) {
                    continue;
                }
                if report.candidate_checks >= deletion_candidate_limit {
                    report.complete = false;
                    deletion.candidate_budget_exhausted = true;
                    if options.mode == PivotMode::Random || index + 1 == total_deletions {
                        report.stop_reason = Some(StopReason::CandidateLimit);
                    }
                    break 'pivots;
                }
                report.candidate_checks += 1;
                deletion.pivot_candidates += 1;

                let exact_frequency_proof = complete_enumeration;
                let mut completion = pivot.solution;
                if options.candidate_mode == PivotCandidateMode::Direct {
                    let check = if let Some(check) =
                        direct_checks.get(&(pivot.position, pivot.digit)).copied()
                    {
                        check
                    } else {
                        match check_unique_solution_cancellable(
                            &candidate,
                            options.max_nodes_per_candidate_check,
                            cancelled,
                            options.max_duration.map(|limit| (started, limit)),
                        ) {
                            Ok(check) => check,
                            Err(_) => continue,
                        }
                    };
                    deletion.candidate_proof_nodes = deletion
                        .candidate_proof_nodes
                        .saturating_add(check.explored_nodes);
                    report.explored_nodes =
                        report.explored_nodes.saturating_add(check.explored_nodes);
                    on_progress(PivotProgress {
                        removed_position: removed,
                        phase: PivotPhase::CheckingPivots,
                        solutions_found: 0,
                        explored_nodes: report.explored_nodes,
                        candidate_checks: report.candidate_checks,
                    });
                    if check.status != UniquenessStatus::Unique {
                        match check.status {
                            UniquenessStatus::Multiple => {
                                deletion.multiple_candidates += 1;
                                rejected_classes.insert(puzzle_key.clone());
                                if let Some(alternative) = check.alternative {
                                    if direct_checks.is_empty() {
                                        prune_candidates_with_witness(
                                            &reduced,
                                            &alternative,
                                            &mut rejected_by_witness,
                                        );
                                    } else {
                                        batch_counterexamples.push(alternative);
                                    }
                                }
                            }
                            UniquenessStatus::Unknown => {
                                deletion.unknown_candidates += 1;
                                report.complete = false;
                            }
                            UniquenessStatus::NoSolution => {
                                deletion.no_solution_candidates += 1;
                                rejected_classes.insert(puzzle_key.clone());
                            }
                            UniquenessStatus::Unique => unreachable!(),
                        }
                        if check.status == UniquenessStatus::Unknown {
                            if cancelled.load(Ordering::Relaxed) {
                                report.complete = false;
                                report.stop_reason = Some(StopReason::Cancelled);
                                break 'pivots;
                            }
                            if options
                                .max_duration
                                .is_some_and(|limit| started.elapsed() >= limit)
                            {
                                report.complete = false;
                                report.stop_reason = Some(StopReason::TimeLimit);
                                break 'pivots;
                            }
                        }
                        continue;
                    }
                    completion = check.solution.expect("unique result has a solution");
                } else if !exact_frequency_proof {
                    match check_unique_solution_cancellable(
                        &candidate,
                        options.max_nodes_per_candidate_check,
                        cancelled,
                        options.max_duration.map(|limit| (started, limit)),
                    ) {
                        Ok(check) => {
                            deletion.candidate_proof_nodes = deletion
                                .candidate_proof_nodes
                                .saturating_add(check.explored_nodes);
                            report.explored_nodes =
                                report.explored_nodes.saturating_add(check.explored_nodes);
                            on_progress(PivotProgress {
                                removed_position: removed,
                                phase: PivotPhase::CheckingPivots,
                                solutions_found: summary.frequencies.total,
                                explored_nodes: report.explored_nodes,
                                candidate_checks: report.candidate_checks,
                            });
                            if check.status != UniquenessStatus::Unique {
                                match check.status {
                                    UniquenessStatus::Multiple => {
                                        deletion.multiple_candidates += 1;
                                        if let Some(alternative) = check.alternative {
                                            prune_candidates_with_witness(
                                                &reduced,
                                                &alternative,
                                                &mut rejected_by_witness,
                                            );
                                        }
                                    }
                                    UniquenessStatus::Unknown => deletion.unknown_candidates += 1,
                                    UniquenessStatus::NoSolution => {
                                        deletion.no_solution_candidates += 1
                                    }
                                    UniquenessStatus::Unique => unreachable!(),
                                }
                                if matches!(
                                    check.status,
                                    UniquenessStatus::Multiple | UniquenessStatus::NoSolution
                                ) {
                                    rejected_classes.insert(puzzle_key.clone());
                                }
                                if check.status == UniquenessStatus::Unknown {
                                    if cancelled.load(Ordering::Relaxed) {
                                        report.complete = false;
                                        report.stop_reason = Some(StopReason::Cancelled);
                                        break 'pivots;
                                    }
                                    if options
                                        .max_duration
                                        .is_some_and(|limit| started.elapsed() >= limit)
                                    {
                                        report.complete = false;
                                        report.stop_reason = Some(StopReason::TimeLimit);
                                        break 'pivots;
                                    }
                                }
                                continue;
                            }
                            completion = check.solution.expect("unique result has a solution");
                        }
                        Err(_) => continue,
                    }
                }

                if !is_complete_solution(&completion)
                    || !solution_matches_clues(&completion, &candidate)
                {
                    continue;
                }
                seen_classes.insert(puzzle_key);
                deletion.unique_candidates += 1;
                deletion.new_canonical_classes += 1;
                let different_solution = completion != parent_solution;
                let different_solution_class = canonical_key(&completion) != parent_solution_key;
                report.candidates.push(PivotCandidate {
                    puzzle: candidate,
                    solution: completion,
                    removed_position: removed,
                    pivot_position: pivot.position,
                    exact_frequency_proof,
                    different_solution,
                    different_solution_class,
                });
                on_progress(PivotProgress {
                    removed_position: removed,
                    phase: PivotPhase::CheckingPivots,
                    solutions_found: summary.frequencies.total,
                    explored_nodes: report.explored_nodes,
                    candidate_checks: report.candidate_checks,
                });
            }
            for alternative in batch_counterexamples {
                prune_candidates_with_witness(&reduced, &alternative, &mut rejected_by_witness);
            }
        }

        deletion.elapsed_micros = enumeration_elapsed_micros
            .saturating_add(deletion_started.elapsed().as_micros() as u64);
        report.deletions.push(deletion);
    }

    if options.prioritize_different_solution {
        report.candidates.sort_by_key(|candidate| {
            (
                !candidate.different_solution_class,
                !candidate.different_solution,
            )
        });
    }
    Ok(report)
}

fn board_key(board: &Matrix) -> [u8; 81] {
    std::array::from_fn(|index| board[index / 9][index % 9])
}

fn is_complete_solution(solution: &Matrix) -> bool {
    let mut rows = [0u16; 9];
    let mut columns = [0u16; 9];
    let mut blocks = [0u16; 9];
    for (position, &digit) in solution.iter().flatten().enumerate() {
        if !(1..=9).contains(&digit) {
            return false;
        }
        let row = position / 9;
        let column = position % 9;
        let block = (row / 3) * 3 + column / 3;
        let bit = 1u16 << (digit - 1);
        if rows[row] & bit != 0 || columns[column] & bit != 0 || blocks[block] & bit != 0 {
            return false;
        }
        rows[row] |= bit;
        columns[column] |= bit;
        blocks[block] |= bit;
    }
    true
}

fn solution_matches_clues(solution: &Matrix, puzzle: &Matrix) -> bool {
    solution
        .iter()
        .flatten()
        .zip(puzzle.iter().flatten())
        .all(|(&digit, &clue)| !(1..=9).contains(&clue) || digit == clue)
}

fn prune_candidates_with_witness(
    core: &Matrix,
    alternative: &Matrix,
    rejected: &mut HashSet<(usize, u8)>,
) -> u64 {
    let mut pruned = 0;
    for position in 0..81 {
        if !(1..=9).contains(&core[position / 9][position % 9])
            && rejected.insert((position, alternative[position / 9][position % 9]))
        {
            pruned += 1;
        }
    }
    pruned
}

fn per_deletion_candidate_budget(
    mode: PivotMode,
    remaining_budget: u64,
    deletions_remaining: u64,
) -> u64 {
    if deletions_remaining == 0 {
        return 0;
    }
    match mode {
        PivotMode::Random => remaining_budget,
        PivotMode::Exhaustive => {
            remaining_budget.saturating_add(deletions_remaining - 1) / deletions_remaining
        }
    }
}

struct PivotRng(u64);

impl PivotRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let other = (self.next() % (index as u64 + 1)) as usize;
            values.swap(index, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        enumerate_solution_space, PivotCache, SolutionFrequencies, SolutionSpaceKind, StopReason,
    };
    use crate::{check_uniqueness, Matrix, UniquenessStatus};
    use std::collections::HashSet;
    use std::sync::atomic::AtomicBool;

    const SOLUTION: Matrix = [
        [5, 3, 4, 6, 7, 8, 9, 1, 2],
        [6, 7, 2, 1, 9, 5, 3, 4, 8],
        [1, 9, 8, 3, 4, 2, 5, 6, 7],
        [8, 5, 9, 7, 6, 1, 4, 2, 3],
        [4, 2, 6, 8, 5, 3, 7, 9, 1],
        [7, 1, 3, 9, 2, 4, 8, 5, 6],
        [9, 6, 1, 5, 3, 7, 2, 8, 4],
        [2, 8, 7, 4, 1, 9, 6, 3, 5],
        [3, 4, 5, 2, 8, 6, 1, 7, 9],
    ];

    fn two_solution_rectangle() -> (Matrix, Matrix) {
        let mut first = SOLUTION;
        for (y, x) in [(3, 5), (3, 8), (4, 5), (4, 8)] {
            first[y][x] = 0;
        }
        let mut second = SOLUTION;
        for (y, x, value) in [(3, 5, 3), (3, 8, 1), (4, 5, 1), (4, 8, 3)] {
            second[y][x] = value;
        }
        (first, second)
    }

    fn four_solution_rectangles() -> Matrix {
        let mut board = SOLUTION;
        for (y, x) in [
            (0, 3),
            (0, 4),
            (3, 3),
            (3, 4),
            (1, 7),
            (1, 8),
            (6, 7),
            (6, 8),
        ] {
            board[y][x] = 0;
        }
        board
    }

    #[test]
    fn frequency_counts_saturate_at_two_and_singleton_witnesses_are_removed() {
        let mut frequencies = SolutionFrequencies::default();
        frequencies.observe(SOLUTION);
        let position = 3 * 9 + 5;
        assert_eq!(frequencies.counts[position][0], 1);
        assert!(frequencies.witnesses[position][0].is_some());

        let mut second = SOLUTION;
        for (y, x, digit) in [(3, 5, 3), (3, 8, 1), (4, 5, 1), (4, 8, 3)] {
            second[y][x] = digit;
        }
        frequencies.observe(second);
        frequencies.observe(SOLUTION);
        assert_eq!(frequencies.counts[position][0], 2);
        assert_eq!(frequencies.total, 3);
        assert!(frequencies.witnesses[position][0].is_none());
    }

    #[test]
    fn exact_two_is_reported_only_after_complete_enumeration_and_pivots_are_unique() {
        let (board, second_solution) = two_solution_rectangle();
        let summary = enumerate_solution_space(&board, 2, 100_000, &AtomicBool::new(false))
            .expect("the rectangle is a valid Sudoku");
        assert_eq!(summary.kind, SolutionSpaceKind::ExactTwo);
        assert_eq!(summary.frequencies.total, 2);

        for position in [3 * 9 + 5, 3 * 9 + 8, 4 * 9 + 5, 4 * 9 + 8] {
            let digits: Vec<_> = summary.frequencies.counts[position]
                .iter()
                .enumerate()
                .filter_map(|(digit, &count)| (count == 1).then_some((digit + 1) as u8))
                .collect();
            assert_eq!(digits.len(), 2);
            for digit in digits {
                let mut candidate = board;
                candidate[position / 9][position % 9] = digit;
                let check = check_uniqueness(&candidate, 100_000).unwrap();
                assert_eq!(check.status, UniquenessStatus::Unique);
                let mut completion = candidate;
                assert!(crate::solve(&mut completion));
                assert!(completion == SOLUTION || completion == second_solution);
            }
        }

        let mut common_cell = board;
        common_cell[0][0] = SOLUTION[0][0];
        assert_eq!(
            check_uniqueness(&common_cell, 100_000).unwrap().status,
            UniquenessStatus::Multiple
        );
    }

    #[test]
    fn solution_cap_and_node_cap_are_partial_not_exact_counts() {
        let (board, _) = two_solution_rectangle();
        let solution_limited =
            enumerate_solution_space(&board, 1, 100_000, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            solution_limited.kind,
            SolutionSpaceKind::Partial(StopReason::SolutionLimit)
        );
        assert_eq!(solution_limited.frequencies.total, 2);

        let node_limited =
            enumerate_solution_space(&board, 10, 1, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            node_limited.kind,
            SolutionSpaceKind::Partial(StopReason::NodeLimit)
        );
    }

    #[test]
    fn exact_many_uses_only_frequency_one_pivots() {
        let board = four_solution_rectangles();
        let summary = enumerate_solution_space(&board, 100, 100_000, &AtomicBool::new(false))
            .expect("the trade board is valid");
        assert_eq!(summary.kind, SolutionSpaceKind::ExactMany);
        assert!(summary.frequencies.total >= 3);

        let candidates = summary.frequencies.singleton_candidates(&board);
        assert!(candidates.is_empty());
        assert!(summary
            .frequencies
            .counts
            .iter()
            .flat_map(|counts| counts.iter())
            .filter(|&&count| count != 0)
            .all(|&count| count >= 2));

        let partial =
            enumerate_solution_space(&board, 2, 100_000, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            partial.kind,
            SolutionSpaceKind::Partial(StopReason::SolutionLimit)
        );
        assert!(!partial.frequencies.singleton_candidates(&board).is_empty());
        for pivot in partial.frequencies.singleton_candidates(&board) {
            let mut candidate = board;
            candidate[pivot.position / 9][pivot.position % 9] = pivot.digit;
            assert_eq!(
                check_uniqueness(&candidate, 100_000).unwrap().status,
                UniquenessStatus::Multiple,
                "sample singletons must not be accepted without exact verification"
            );
        }
    }

    #[test]
    fn direct_pivot_mode_skips_solution_space_enumeration_and_bounds_queries() {
        let (parent, solution) = crate::generator::verified_17_clue_base(19);
        let options = super::PivotOptions {
            mode: super::PivotMode::Random,
            candidate_mode: super::PivotCandidateMode::Direct,
            threads: 1,
            max_solutions: 10,
            max_nodes_per_search: 100,
            max_nodes_per_candidate_check: 1,
            max_candidate_checks: 2,
            seed: 19,
            prioritize_different_solution: true,
            max_duration: None,
        };
        let report = super::search_solution_space_pivots_with_progress_verified_parent(
            19,
            parent,
            solution,
            options,
            &AtomicBool::new(false),
            &mut super::PivotCache::new(0),
            |_| {},
        )
        .unwrap();

        assert_eq!(report.candidate_mode, super::PivotCandidateMode::Direct);
        assert!(!report.solution_spaces_complete);
        assert_eq!(report.candidate_checks, 2);
        assert_eq!(report.stop_reason, Some(StopReason::CandidateLimit));
        assert!(report.deletions.iter().all(|deletion| {
            deletion.solution_space_kind == SolutionSpaceKind::NotEnumerated
                && deletion.solutions_found == 0
                && deletion.enumeration_nodes == 0
        }));
    }

    #[test]
    fn parallel_direct_pivots_carry_counterexamples_into_the_next_batch() {
        let (parent, solution) = crate::generator::verified_17_clue_base(19);
        let options = super::PivotOptions {
            mode: super::PivotMode::Random,
            candidate_mode: super::PivotCandidateMode::Direct,
            threads: 4,
            max_solutions: 10,
            max_nodes_per_search: 100,
            max_nodes_per_candidate_check: 100_000,
            max_candidate_checks: 8,
            seed: 19,
            prioritize_different_solution: false,
            max_duration: None,
        };
        let report = super::search_solution_space_pivots_with_progress_verified_parent(
            19,
            parent,
            solution,
            options,
            &AtomicBool::new(false),
            &mut super::PivotCache::new(0),
            |_| {},
        )
        .unwrap();

        assert!(report
            .deletions
            .iter()
            .any(|deletion| deletion.witness_pruned_candidates > 0));
    }

    #[test]
    fn direct_uniqueness_query_returns_proof_witness_or_unknown() {
        let unique =
            super::check_unique_solution_cancellable(&SOLUTION, 10, &AtomicBool::new(false), None)
                .unwrap();
        assert_eq!(unique.status, UniquenessStatus::Unique);
        assert_eq!(unique.solution, Some(SOLUTION));

        let ambiguous = two_solution_rectangle().0;
        let multiple = super::check_unique_solution_cancellable(
            &ambiguous,
            100_000,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
        assert_eq!(multiple.status, UniquenessStatus::Multiple);
        assert!(multiple.solution.is_some());

        let unknown = super::check_unique_solution_cancellable(
            &[[0; 9]; 9],
            1,
            &AtomicBool::new(false),
            None,
        )
        .unwrap();
        assert_eq!(unknown.status, UniquenessStatus::Unknown);
        assert!(unknown.explored_nodes > 1);
    }

    #[test]
    fn counterexample_prunes_only_additions_it_satisfies() {
        let mut core = [[0; 9]; 9];
        core[0][0] = SOLUTION[0][0];
        let mut rejected = HashSet::new();
        let pruned = super::prune_candidates_with_witness(&core, &SOLUTION, &mut rejected);
        assert_eq!(pruned, 80);
        assert!(!rejected.contains(&(0, SOLUTION[0][0])));
        assert!(rejected.contains(&(1, SOLUTION[0][1])));
        assert!(!rejected.contains(&(1, if SOLUTION[0][1] == 9 { 1 } else { 9 })));
        assert_eq!(
            super::prune_candidates_with_witness(&core, &SOLUTION, &mut rejected),
            0,
            "a repeated witness must not double-count exclusions"
        );
    }

    #[test]
    fn exhaustive_pivot_budget_is_shared_fairly_between_deletions() {
        assert_eq!(
            super::per_deletion_candidate_budget(super::PivotMode::Random, 128, 17),
            128
        );
        assert_eq!(
            super::per_deletion_candidate_budget(super::PivotMode::Exhaustive, 128, 17),
            8
        );
        assert_eq!(
            super::per_deletion_candidate_budget(super::PivotMode::Exhaustive, 120, 16),
            8
        );
        assert_eq!(
            super::per_deletion_candidate_budget(super::PivotMode::Exhaustive, 5, 4),
            2
        );
        assert_eq!(
            super::per_deletion_candidate_budget(super::PivotMode::Exhaustive, 0, 3),
            0
        );
    }

    #[test]
    fn cancelled_enumeration_is_reported_as_partial() {
        let cancelled = AtomicBool::new(true);
        let summary = enumerate_solution_space(&SOLUTION, 10, 100_000, &cancelled).unwrap();
        assert_eq!(
            summary.kind,
            SolutionSpaceKind::Partial(StopReason::Cancelled)
        );
        assert_eq!(summary.frequencies.total, 0);
    }

    #[test]
    fn pivot_cache_is_bounded() {
        let mut cache = PivotCache::new(1);
        let summary =
            enumerate_solution_space(&SOLUTION, 1, 100_000, &AtomicBool::new(false)).unwrap();
        cache.insert(&SOLUTION, summary.clone());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&SOLUTION), Some(summary));
        let mut another = SOLUTION;
        another[0][0] = 0;
        cache.insert(
            &another,
            enumerate_solution_space(&another, 1, 100_000, &AtomicBool::new(false)).unwrap(),
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn parallel_deletion_enumeration_merges_in_input_order() {
        let board = four_solution_rectangles();
        let tasks = [(0, 10, board), (1, 20, board), (2, 30, board)];
        let mut sequential_options = super::PivotOptions {
            mode: super::PivotMode::Exhaustive,
            candidate_mode: super::PivotCandidateMode::Frequency,
            threads: 1,
            max_solutions: 100,
            max_nodes_per_search: 100_000,
            max_nodes_per_candidate_check: 100_000,
            max_candidate_checks: 100,
            seed: 7,
            prioritize_different_solution: true,
            max_duration: None,
        };
        let sequential = super::enumerate_deletion_batch(
            &tasks,
            tasks.len(),
            sequential_options,
            std::time::Instant::now(),
            0,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();

        sequential_options.threads = tasks.len();
        let parallel = super::enumerate_deletion_batch(
            &tasks,
            tasks.len(),
            sequential_options,
            std::time::Instant::now(),
            0,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();

        let summaries = |results: Vec<Option<(super::SolutionSpaceSummary, u64)>>| {
            results
                .into_iter()
                .map(|result| result.map(|(summary, _)| summary))
                .collect::<Vec<_>>()
        };
        assert_eq!(summaries(sequential), summaries(parallel));
        assert!(summaries(
            super::enumerate_deletion_batch(
                &tasks,
                tasks.len(),
                sequential_options,
                std::time::Instant::now(),
                0,
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .unwrap()
        )
        .iter()
        .all(|summary| summary
            .as_ref()
            .is_some_and(|summary| summary.kind == SolutionSpaceKind::ExactMany)));
    }
}
