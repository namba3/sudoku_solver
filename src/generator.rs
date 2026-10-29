pub use crate::minimal_puzzle_seeds::{minimal_puzzle_seeds, MINIMAL_PUZZLES_BY_CLUE_COUNT};
use crate::solver::{
    find_alternative_solution as solver_find_alternative_solution, AlternativeSolutionStatus,
};
use crate::Matrix;
use std::collections::{HashSet, VecDeque};

pub const MIN_CLUE_COUNT: usize = 17;
pub const MAX_CLUE_COUNT: usize = 81;
pub const MAX_INTERMEDIATE_CLUE_COUNT: usize = 80;
pub const DEFAULT_CLUE_COUNT: usize = 30;

const RANDOM_TRANSFORM_STEPS: usize = 100;
const PAIR_CANDIDATE_CAPACITY: usize = 2_145;
const PAIR_CANDIDATE_WORDS: usize = PAIR_CANDIDATE_CAPACITY.div_ceil(64);
const UNAVOIDABLE_WITNESS_CACHE_CAPACITY: usize = 64;

/// Fixed-capacity bitset over the C(66, 2) additions possible from a 15-clue core.
struct PairCandidateSet {
    pairs: Vec<(usize, usize)>,
    active: [u64; PAIR_CANDIDATE_WORDS],
    hit_by_position: [[u64; PAIR_CANDIDATE_WORDS]; 81],
}

impl PairCandidateSet {
    fn new(pairs: Vec<(usize, usize)>) -> Self {
        assert!(pairs.len() <= PAIR_CANDIDATE_CAPACITY);
        let mut active = [0; PAIR_CANDIDATE_WORDS];
        let mut hit_by_position = [[0; PAIR_CANDIDATE_WORDS]; 81];
        for index in 0..pairs.len() {
            active[index / 64] |= 1 << (index % 64);
            let (first, second) = pairs[index];
            let bit = 1 << (index % 64);
            hit_by_position[first][index / 64] |= bit;
            hit_by_position[second][index / 64] |= bit;
        }
        Self {
            pairs,
            active,
            hit_by_position,
        }
    }

    fn retain_pairs_hit_by(&mut self, difference: u128) {
        let mut hit = [0; PAIR_CANDIDATE_WORDS];
        let mut positions = difference;
        while positions != 0 {
            let position = positions.trailing_zeros() as usize;
            positions &= positions - 1;
            for (word, position_hits) in hit.iter_mut().zip(&self.hit_by_position[position]) {
                *word |= *position_hits;
            }
        }
        for (active, hit) in self.active.iter_mut().zip(hit) {
            *active &= hit;
        }
    }

    fn pop(&mut self) -> Option<(usize, usize)> {
        for (word_index, word) in self.active.iter_mut().enumerate() {
            if *word != 0 {
                let bit = word.trailing_zeros() as usize;
                *word &= *word - 1;
                return self.pairs.get(word_index * 64 + bit).copied();
            }
        }
        None
    }
}

/// Minimal antichain of unavoidable position sets for a fixed solution.
#[derive(Default)]
struct DifferenceAntichain(Vec<u128>);

impl DifferenceAntichain {
    fn insert(&mut self, difference: u128) {
        if difference == 0
            || self
                .0
                .iter()
                .any(|&existing| existing & difference == existing)
        {
            return;
        }
        self.0
            .retain(|&existing| difference & existing != difference);
        self.0.push(difference);
    }

    fn prune_pairs(&self, candidates: &mut PairCandidateSet) {
        for &difference in &self.0 {
            candidates.retain_pairs_hit_by(difference);
        }
    }
}

/// Bounded local cache of alternative completions reusable across compatible cores.
#[derive(Default)]
struct UnavoidableWitnessCache {
    alternatives: VecDeque<Matrix>,
}

impl UnavoidableWitnessCache {
    fn insert(&mut self, alternative: Matrix) {
        if self.alternatives.contains(&alternative) {
            return;
        }
        if self.alternatives.len() == UNAVOIDABLE_WITNESS_CACHE_CAPACITY {
            self.alternatives.pop_front();
        }
        self.alternatives.push_back(alternative);
    }

    fn compatible_with<'a>(&'a self, core: &'a Matrix) -> impl Iterator<Item = &'a Matrix> + 'a {
        self.alternatives.iter().filter(move |alternative| {
            (0..81).all(|position| {
                let clue = core[position / 9][position % 9];
                !(1..=9).contains(&clue) || clue == alternative[position / 9][position % 9]
            })
        })
    }
}

fn solution_difference(solution: &Matrix, alternative: &Matrix) -> u128 {
    let mut difference = 0;
    for position in 0..81 {
        if solution[position / 9][position % 9] != alternative[position / 9][position % 9] {
            difference |= 1u128 << position;
        }
    }
    difference
}

/// Return the verified 17-clue puzzle and its known solution selected by `seed`.
/// The returned pair is suitable as a parent for bounded Diverse strategies.
pub fn verified_17_clue_base(seed: u64) -> (Matrix, Matrix) {
    let bases = minimal_puzzle_seeds(17);
    let index = (SplitMix64(seed).next() % bases.len() as u64) as usize;
    bases[index]
}

/// Return every built-in verified 17-clue seed, including the original anchor.
pub fn verified_17_clue_bases() -> impl Iterator<Item = (Matrix, Matrix)> {
    minimal_puzzle_seeds(17).iter().copied()
}

fn verified_minimal_base_up_to(rng: &mut SplitMix64, max_clue_count: usize) -> (Matrix, Matrix) {
    let pool_size: usize = (MIN_CLUE_COUNT..=max_clue_count)
        .map(|count| minimal_puzzle_seeds(count as u8).len())
        .sum();
    debug_assert!(pool_size > 0);

    let mut index = (rng.next() % pool_size as u64) as usize;
    for clue_count in MIN_CLUE_COUNT..=max_clue_count {
        let pool = minimal_puzzle_seeds(clue_count as u8);
        if index < pool.len() {
            return pool[index];
        }
        index -= pool.len();
    }

    unreachable!("the selected minimal puzzle index must be in the pool")
}

/// Generate a randomized uniquely solvable puzzle with the default 30 clues.
pub fn generate_puzzle(seed: u64) -> Matrix {
    generate_puzzle_with_clues(seed, DEFAULT_CLUE_COUNT).expect("the default clue count is valid")
}

/// Generate a randomized uniquely solvable puzzle with exactly `clue_count` clues.
///
/// The supported range is 17 through 81. The randomized puzzle starts from a
/// uniformly selected built-in minimal puzzle with at most `clue_count` clues,
/// applies 100 successive random Sudoku-preserving transforms, then adds digits
/// from its solution as needed. Adding clues preserves uniqueness while
/// guaranteeing the requested clue count.
pub fn generate_puzzle_with_clues(seed: u64, clue_count: usize) -> Result<Matrix, &'static str> {
    if !(MIN_CLUE_COUNT..=MAX_CLUE_COUNT).contains(&clue_count) {
        return Err("clue count must be between 17 and 81");
    }

    let mut rng = SplitMix64(seed);
    let (base_puzzle, base_solution) = verified_minimal_base_up_to(&mut rng, clue_count);
    let base_clue_count = base_puzzle
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count();
    let mut puzzle = base_puzzle;
    let mut solution = base_solution;
    for _ in 0..RANDOM_TRANSFORM_STEPS {
        let transform = SudokuTransform::random(&mut rng);
        puzzle = transform.apply(&puzzle);
        solution = transform.apply(&solution);
    }

    let mut empty_positions = [0usize; 81];
    let mut empty_count = 0;
    for position in 0..81 {
        if puzzle[position / 9][position % 9] == 0 {
            empty_positions[empty_count] = position;
            empty_count += 1;
        }
    }
    rng.shuffle(&mut empty_positions[..empty_count]);
    for &position in empty_positions[..empty_count]
        .iter()
        .take(clue_count - base_clue_count)
    {
        let y = position / 9;
        let x = position % 9;
        puzzle[y][x] = solution[y][x];
    }

    Ok(puzzle)
}

/// Select a puzzle generation strategy. `Fast` preserves the established
/// deterministic generator; `Diverse` searches for an independent completion
/// and removes clues while proving uniqueness within a finite budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationMethod {
    Fast,
    Diverse,
}

/// Search schedule used by 17-clue Diverse generation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiverseStrategy {
    /// Try intermediate states, then `-2+2`, `-1+1`, and `-3+3`.
    #[default]
    Adaptive,
    /// Search only the one-clue exchange neighborhood.
    SwapOne,
    /// Try every value for a one-clue addition before removing one parent clue.
    PlusOneMinusOne,
    /// Search only the two-clue exchange neighborhood.
    SwapTwo,
    /// Search only the three-clue exchange neighborhood.
    SwapThree,
    /// Search only paths through verified 18-clue states.
    ViaEighteen,
    /// Search through bridge states from 18 clues up to the configured limit.
    ViaIntermediate,
    /// Walk one clue at a time, drifting toward 17 clues while allowing multi-solution states.
    DriftWalk,
    /// Use the solution set of a 16-clue deletion to pivot to new 17-clue puzzles.
    SolutionSpacePivot,
    /// Start from random 17 cells of a random completed grid and repair ambiguity
    /// with witness-guided one-clue-for-one-clue swaps.
    Random17Witness,
}

impl DiverseStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::SwapOne => "swap-1+1",
            Self::PlusOneMinusOne => "swap+1-1",
            Self::SwapTwo => "swap-2+2",
            Self::SwapThree => "swap-3+3",
            Self::ViaEighteen => "via-18",
            Self::ViaIntermediate => "via-intermediate",
            Self::DriftWalk => "drift-walk",
            Self::SolutionSpacePivot => "solution-space-pivot",
            Self::Random17Witness => "random-17-witness",
        }
    }
}

fn adaptive_schedule(max_intermediate_clues: usize) -> Vec<DiverseStrategy> {
    let mut schedule: Vec<_> = (18..=max_intermediate_clues)
        .map(|_| DiverseStrategy::ViaIntermediate)
        .collect();
    schedule.extend([
        DiverseStrategy::SwapTwo,
        DiverseStrategy::SwapOne,
        DiverseStrategy::SwapThree,
    ]);
    schedule
}

/// Shared request shape for the available generation methods.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationConfig {
    pub seed: u64,
    pub clue_count: usize,
    pub method: GenerationMethod,
    pub diverse_budget: DiverseBudget,
}

/// Error returned by the shared generation interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationError {
    InvalidClueCount,
    Diverse(DiverseGenerationError),
}

/// Finite limits used by Diverse generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiverseBudget {
    pub max_attempts: u32,
    pub max_nodes_per_uniqueness_check: u64,
    pub max_hitting_set_nodes: u64,
    pub max_neighborhood_checks: u64,
    /// Highest clue count to try in the intermediate-state route (18..=80).
    pub max_intermediate_clues: usize,
    pub strategy: DiverseStrategy,
}

/// Snapshot emitted while a Diverse generation attempt is running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiverseProgress {
    pub attempt: u32,
    pub clues_remaining: usize,
    pub uniqueness_nodes: u64,
    pub hitting_set_nodes: u64,
    pub neighborhood_checks: u64,
    pub strategy: DiverseStrategy,
}

impl Default for DiverseBudget {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            max_nodes_per_uniqueness_check: 100_000,
            max_hitting_set_nodes: 100_000,
            max_neighborhood_checks: 1_088,
            max_intermediate_clues: 18,
            strategy: DiverseStrategy::Adaptive,
        }
    }
}

/// Generate one puzzle with the selected method.
pub fn generate(config: GenerationConfig) -> Result<Matrix, GenerationError> {
    generate_with_progress(config, |_| {})
}

/// Generate a puzzle and report Diverse progress snapshots.
pub fn generate_with_progress(
    config: GenerationConfig,
    on_progress: impl FnMut(DiverseProgress),
) -> Result<Matrix, GenerationError> {
    match config.method {
        GenerationMethod::Fast => generate_puzzle_with_clues(config.seed, config.clue_count)
            .map_err(|_| GenerationError::InvalidClueCount),
        GenerationMethod::Diverse => generate_diverse_with_progress(
            config.seed,
            config.clue_count,
            config.diverse_budget,
            on_progress,
        )
        .map_err(GenerationError::Diverse),
    }
}

/// Failure to produce the requested clue count within the supplied budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiverseGenerationError {
    InvalidClueCount,
    InvalidIntermediateClueCount,
    InvalidParent,
    StrategyDoesNotUseParent,
    EmptyBudget,
    BudgetExhausted {
        attempts: u32,
        uniqueness_nodes: u64,
        hitting_set_nodes: u64,
        neighborhood_checks: u64,
    },
}

/// Generate a puzzle using the selected Diverse strategy.
///
/// For 17 clues, this evolves a verified base puzzle through bounded
/// intermediate-state, exchange neighborhoods, and single-clue drift walks. For larger
/// clue counts, it randomizes a completion, removes clues, and may rebuild a
/// target-sized set from observed unavoidable constraints. Every returned
/// puzzle has passed a complete uniqueness check.
pub fn generate_diverse(
    seed: u64,
    clue_count: usize,
    budget: DiverseBudget,
) -> Result<Matrix, DiverseGenerationError> {
    generate_diverse_with_progress(seed, clue_count, budget, |_| {})
}

/// Search the `-1+1` neighborhood of a previously verified 17-clue puzzle.
/// The supplied solution must be the puzzle's unique completion.
pub fn generate_diverse_from_parent(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    budget: DiverseBudget,
) -> Result<Matrix, DiverseGenerationError> {
    generate_diverse_from_parent_with_progress(seed, parent, parent_solution, budget, |_| {})
}

/// Progress-reporting form of [`generate_diverse_from_parent`].
/// `Random17Witness` is rejected because that strategy starts from its own
/// random completion and does not evolve the supplied parent.
pub fn generate_diverse_from_parent_with_progress(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    budget: DiverseBudget,
    mut on_progress: impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    if budget.max_attempts == 0 || budget.max_nodes_per_uniqueness_check == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }
    if !(18..=MAX_INTERMEDIATE_CLUE_COUNT).contains(&budget.max_intermediate_clues) {
        return Err(DiverseGenerationError::InvalidIntermediateClueCount);
    }
    if budget.strategy == DiverseStrategy::Random17Witness {
        return Err(DiverseGenerationError::StrategyDoesNotUseParent);
    }
    let clues = parent
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count();
    if clues != MIN_CLUE_COUNT
        || !is_complete_valid_solution(&parent_solution)
        || parent
            .iter()
            .zip(parent_solution.iter())
            .any(|(puzzle_row, solution_row)| {
                puzzle_row
                    .iter()
                    .zip(solution_row.iter())
                    .any(|(&given, &value)| (1..=9).contains(&given) && given != value)
            })
    {
        return Err(DiverseGenerationError::InvalidParent);
    }
    let check = crate::check_uniqueness(&parent, budget.max_nodes_per_uniqueness_check)
        .map_err(|_| DiverseGenerationError::InvalidParent)?;
    let mut solved = parent;
    if check.status == crate::UniquenessStatus::Unknown {
        return Err(DiverseGenerationError::BudgetExhausted {
            attempts: 0,
            uniqueness_nodes: check.explored_nodes,
            hitting_set_nodes: 0,
            neighborhood_checks: 0,
        });
    }
    if check.status != crate::UniquenessStatus::Unique
        || !crate::solve(&mut solved)
        || solved != parent_solution
    {
        return Err(DiverseGenerationError::InvalidParent);
    }
    generate_diverse_17(
        seed,
        parent,
        parent_solution,
        check.explored_nodes,
        budget,
        &mut on_progress,
    )
}

fn is_complete_valid_solution(solution: &Matrix) -> bool {
    if solution
        .iter()
        .flatten()
        .any(|value| !(1..=9).contains(value))
    {
        return false;
    }
    let full = (1u16 << 9) - 1;
    for index in 0..9 {
        let mut row = 0u16;
        let mut column = 0u16;
        let mut block = 0u16;
        for offset in 0..9 {
            row |= 1 << (solution[index][offset] - 1);
            column |= 1 << (solution[offset][index] - 1);
            let y = (index / 3) * 3 + offset / 3;
            let x = (index % 3) * 3 + offset % 3;
            block |= 1 << (solution[y][x] - 1);
        }
        if row != full || column != full || block != full {
            return false;
        }
    }
    true
}

/// Progress-reporting form of [`generate_diverse`]. The callback runs between
/// uniqueness checks and can be used to publish progress from a Worker.
pub fn generate_diverse_with_progress(
    seed: u64,
    clue_count: usize,
    budget: DiverseBudget,
    mut on_progress: impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    if !(MIN_CLUE_COUNT..=MAX_CLUE_COUNT).contains(&clue_count) {
        return Err(DiverseGenerationError::InvalidClueCount);
    }
    if budget.max_attempts == 0 || budget.max_nodes_per_uniqueness_check == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }
    if !(18..=MAX_INTERMEDIATE_CLUE_COUNT).contains(&budget.max_intermediate_clues) {
        return Err(DiverseGenerationError::InvalidIntermediateClueCount);
    }

    if clue_count == MIN_CLUE_COUNT {
        if budget.strategy == DiverseStrategy::Random17Witness {
            return generate_diverse_random_17_witness(seed, budget, &mut on_progress);
        }
        let (parent, parent_solution) = verified_17_clue_base(seed);
        return generate_diverse_17(seed, parent, parent_solution, 0, budget, &mut on_progress);
    }

    let mut rng = SplitMix64(seed);
    let mut total_nodes = 0u64;
    let mut total_hitting_nodes = 0u64;
    for attempt in 1..=budget.max_attempts {
        let mut solution = [[0; 9]; 9];
        let mut rows = [0u16; 9];
        let mut columns = [0u16; 9];
        let mut blocks = [0u16; 9];
        if !fill_random_solution(
            &mut solution,
            &mut rows,
            &mut columns,
            &mut blocks,
            &mut rng,
        ) {
            continue;
        }

        let mut puzzle = solution;
        let mut unavoidable_sets = Vec::new();
        let mut positions = std::array::from_fn::<_, 81, _>(|index| index);
        rng.shuffle(&mut positions);
        let mut clues = MAX_CLUE_COUNT;
        for position in positions {
            if clues == clue_count {
                break;
            }
            let (y, x) = (position / 9, position % 9);
            puzzle[y][x] = 0;
            match find_alternative_solution(
                &puzzle,
                &solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    if clues > clue_count {
                        clues -= 1;
                    }
                }
                ProbeResult::Multiple { alternative, nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    puzzle[y][x] = solution[y][x];
                    add_unavoidable_set(&solution, &alternative, &mut unavoidable_sets);
                }
                ProbeResult::Unknown { nodes } | ProbeResult::Invalid { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    puzzle[y][x] = solution[y][x];
                }
            }
            on_progress(DiverseProgress {
                attempt,
                clues_remaining: clues,
                uniqueness_nodes: total_nodes,
                hitting_set_nodes: total_hitting_nodes,
                neighborhood_checks: 0,
                strategy: DiverseStrategy::Adaptive,
            });
        }
        if clues == clue_count {
            return Ok(puzzle);
        }

        // A greedy pass often reaches a locally minimal puzzle above the
        // target. Rebuild the clue set from all observed unavoidable sets so
        // the rejected clues can be exchanged for different cells.
        let mut tried_sets = HashSet::new();
        while clues > clue_count
            && total_hitting_nodes < budget.max_hitting_set_nodes
            && !unavoidable_sets.is_empty()
        {
            let remaining_budget = budget.max_hitting_set_nodes - total_hitting_nodes;
            let (candidate_mask, used_nodes) =
                find_hitting_set(&unavoidable_sets, clue_count, remaining_budget, &mut rng);
            total_hitting_nodes = total_hitting_nodes.saturating_add(used_nodes);
            let Some(candidate_mask) = candidate_mask else {
                break;
            };
            if !tried_sets.insert(candidate_mask) {
                break;
            }
            let candidate = board_for_clue_mask(&solution, candidate_mask);
            match find_alternative_solution(
                &candidate,
                &solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { .. } => return Ok(candidate),
                ProbeResult::Multiple { alternative, nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    add_unavoidable_set(&solution, &alternative, &mut unavoidable_sets);
                }
                ProbeResult::Unknown { nodes } | ProbeResult::Invalid { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    break;
                }
            }
            on_progress(DiverseProgress {
                attempt,
                clues_remaining: clue_count,
                uniqueness_nodes: total_nodes,
                hitting_set_nodes: total_hitting_nodes,
                neighborhood_checks: 0,
                strategy: DiverseStrategy::Adaptive,
            });
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: budget.max_attempts,
        uniqueness_nodes: total_nodes,
        hitting_set_nodes: total_hitting_nodes,
        neighborhood_checks: 0,
    })
}

/// Evolve a verified 17-clue puzzle through its solution-preserving `-1+1`
/// neighborhood. A counterexample to one candidate eliminates every added
/// position on which that counterexample agrees with the known solution.
fn generate_diverse_17(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    let schedule: Vec<DiverseStrategy> = match budget.strategy {
        DiverseStrategy::Adaptive => adaptive_schedule(budget.max_intermediate_clues),
        DiverseStrategy::ViaIntermediate => (18..=budget.max_intermediate_clues)
            .map(|_| DiverseStrategy::ViaIntermediate)
            .collect(),
        strategy => vec![strategy],
    };
    let mut total_nodes = 0u64;
    let mut total_hitting_nodes = 0u64;
    let mut total_checks = 0u64;
    let mut last_attempts = 0u32;
    for (stage, &strategy) in schedule.iter().enumerate() {
        // Reuse the same seed across stages for reproducible, comparable
        // neighborhoods from the same randomized parent transform.
        let stage_seed = seed;
        let stage_initial_nodes = if stage == 0 { initial_nodes } else { 0 };
        let node_offset = total_nodes;
        let check_offset = total_checks;
        let mut stage_budget = budget;
        stage_budget.strategy = strategy;
        let mut report = |progress: DiverseProgress| {
            let mut progress = progress;
            progress.strategy = strategy;
            progress.attempt = progress.attempt.saturating_add(stage as u32);
            progress.uniqueness_nodes = node_offset.saturating_add(progress.uniqueness_nodes);
            progress.neighborhood_checks =
                check_offset.saturating_add(progress.neighborhood_checks);
            on_progress(progress);
        };
        let result = match strategy {
            DiverseStrategy::Adaptive => unreachable!("adaptive is a scheduler, not a searcher"),
            DiverseStrategy::SwapOne => generate_diverse_swap_one(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::PlusOneMinusOne => generate_diverse_plus_one_minus_one(
                stage_seed,
                parent,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::SwapTwo => generate_diverse_swap_two(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::SwapThree => generate_diverse_swap_three(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::ViaEighteen => generate_diverse_via_intermediate(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                18,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::ViaIntermediate => generate_diverse_via_intermediate(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                18 + stage,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::DriftWalk => generate_diverse_drift_walk(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::SolutionSpacePivot => generate_diverse_solution_space_pivot(
                stage_seed,
                parent,
                parent_solution,
                stage_initial_nodes,
                stage_budget,
                &mut report,
            ),
            DiverseStrategy::Random17Witness => {
                generate_diverse_random_17_witness(stage_seed, stage_budget, &mut report)
            }
        };
        match result {
            Ok(board) => return Ok(board),
            Err(DiverseGenerationError::BudgetExhausted {
                attempts,
                uniqueness_nodes,
                hitting_set_nodes,
                neighborhood_checks,
            }) => {
                last_attempts = last_attempts.saturating_add(attempts);
                total_nodes = total_nodes.saturating_add(uniqueness_nodes);
                total_hitting_nodes = total_hitting_nodes.saturating_add(hitting_set_nodes);
                total_checks = total_checks.saturating_add(neighborhood_checks);
            }
            Err(error) => return Err(error),
        }
    }
    Err(DiverseGenerationError::BudgetExhausted {
        attempts: last_attempts,
        uniqueness_nodes: total_nodes,
        hitting_set_nodes: total_hitting_nodes,
        neighborhood_checks: total_checks,
    })
}

fn generate_diverse_solution_space_pivot(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    let options = crate::pivot::PivotOptions {
        seed,
        max_nodes_per_search: budget.max_nodes_per_uniqueness_check,
        max_nodes_per_candidate_check: budget.max_nodes_per_uniqueness_check,
        max_candidate_checks: budget.max_neighborhood_checks,
        ..crate::pivot::PivotOptions::default()
    };
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let mut cache = crate::pivot::PivotCache::default();
    let report = crate::pivot::search_solution_space_pivots_with_progress_verified_parent(
        seed,
        parent,
        parent_solution,
        options,
        &cancelled,
        &mut cache,
        |progress| {
            emit_generation_progress(
                on_progress,
                1,
                MIN_CLUE_COUNT,
                initial_nodes.saturating_add(progress.explored_nodes),
                progress.candidate_checks,
                DiverseStrategy::SolutionSpacePivot,
            )
        },
    )
    .map_err(|error| match error {
        crate::pivot::PivotError::InvalidOptions => DiverseGenerationError::EmptyBudget,
        crate::pivot::PivotError::InvalidParent => DiverseGenerationError::InvalidParent,
        crate::pivot::PivotError::ParentUniquenessUnknown { explored_nodes } => {
            DiverseGenerationError::BudgetExhausted {
                attempts: 0,
                uniqueness_nodes: initial_nodes.saturating_add(explored_nodes),
                hitting_set_nodes: 0,
                neighborhood_checks: 0,
            }
        }
    })?;
    if let Some(candidate) = report.candidates.first() {
        return Ok(candidate.puzzle);
    }
    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: initial_nodes.saturating_add(report.explored_nodes),
        hitting_set_nodes: 0,
        neighborhood_checks: report.candidate_checks,
    })
}

/// Start from a random 17-clue subset of a random complete grid. Since every
/// clue is copied from that grid, each state is guaranteed to retain at least
/// one solution. When a second solution is found, swap one current clue for a
/// clue on a cell where the two completions differ, and continue from there.
fn generate_diverse_random_17_witness(
    seed: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }

    let mut rng = SplitMix64(seed);
    let mut total_nodes = 0u64;
    let mut checks = 0u64;

    for attempt in 1..=budget.max_attempts {
        let mut solution = [[0; 9]; 9];
        let mut rows = [0u16; 9];
        let mut columns = [0u16; 9];
        let mut blocks = [0u16; 9];
        if !fill_random_solution(
            &mut solution,
            &mut rows,
            &mut columns,
            &mut blocks,
            &mut rng,
        ) {
            continue;
        }

        let mut positions = std::array::from_fn::<_, 81, _>(|position| position);
        rng.shuffle(&mut positions);
        let mut clue_mask = positions
            .iter()
            .take(MIN_CLUE_COUNT)
            .fold(0u128, |mask, &position| mask | (1u128 << position));
        let mut visited_masks = HashSet::from([clue_mask]);
        let mut witness_sets = Vec::new();

        while checks < budget.max_neighborhood_checks {
            let puzzle = board_for_clue_mask(&solution, clue_mask);
            checks = checks.saturating_add(1);
            match find_alternative_solution(
                &puzzle,
                &solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    emit_generation_progress(
                        on_progress,
                        attempt,
                        MIN_CLUE_COUNT,
                        total_nodes,
                        checks,
                        DiverseStrategy::Random17Witness,
                    );
                    return Ok(puzzle);
                }
                ProbeResult::Multiple { alternative, nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    let newest_witness = (0..81).fold(0u128, |mask, position| {
                        if solution[position / 9][position % 9]
                            != alternative[position / 9][position % 9]
                        {
                            mask | (1u128 << position)
                        } else {
                            mask
                        }
                    });
                    if newest_witness != 0 && !witness_sets.contains(&newest_witness) {
                        witness_sets.push(newest_witness);
                    }
                    let Some(next_mask) = witness_guided_swap(
                        clue_mask,
                        newest_witness,
                        &witness_sets,
                        &visited_masks,
                        &mut rng,
                    ) else {
                        emit_generation_progress(
                            on_progress,
                            attempt,
                            MIN_CLUE_COUNT,
                            total_nodes,
                            checks,
                            DiverseStrategy::Random17Witness,
                        );
                        break;
                    };
                    clue_mask = next_mask;
                    visited_masks.insert(clue_mask);
                }
                ProbeResult::Unknown { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    let Some(next_mask) = random_clue_swap(clue_mask, &visited_masks, &mut rng)
                    else {
                        break;
                    };
                    clue_mask = next_mask;
                    visited_masks.insert(clue_mask);
                }
                ProbeResult::Invalid { nodes } => {
                    total_nodes = total_nodes.saturating_add(nodes);
                    break;
                }
            }
            emit_generation_progress(
                on_progress,
                attempt,
                MIN_CLUE_COUNT,
                total_nodes,
                checks,
                DiverseStrategy::Random17Witness,
            );
        }

        if checks >= budget.max_neighborhood_checks {
            break;
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: budget.max_attempts,
        uniqueness_nodes: total_nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

fn witness_guided_swap(
    current_mask: u128,
    newest_witness: u128,
    witness_sets: &[u128],
    visited_masks: &HashSet<u128>,
    rng: &mut SplitMix64,
) -> Option<u128> {
    let mut additions: Vec<_> = (0..81)
        .filter(|&position| newest_witness & (1u128 << position) != 0)
        .collect();
    let mut removals: Vec<_> = (0..81)
        .filter(|&position| current_mask & (1u128 << position) != 0)
        .collect();
    rng.shuffle(&mut additions);
    rng.shuffle(&mut removals);

    let mut best: Option<(usize, u128)> = None;
    for &addition in &additions {
        let addition_bit = 1u128 << addition;
        for &removal in &removals {
            let candidate = (current_mask & !(1u128 << removal)) | addition_bit;
            if visited_masks.contains(&candidate) {
                continue;
            }
            let covered = witness_sets
                .iter()
                .filter(|&&witness| witness & candidate != 0)
                .count();
            if best.is_none_or(|(best_covered, _)| covered > best_covered) {
                best = Some((covered, candidate));
            }
        }
    }
    best.map(|(_, mask)| mask)
}

fn random_clue_swap(
    current_mask: u128,
    visited_masks: &HashSet<u128>,
    rng: &mut SplitMix64,
) -> Option<u128> {
    let mut additions: Vec<_> = (0..81)
        .filter(|&position| current_mask & (1u128 << position) == 0)
        .collect();
    let mut removals: Vec<_> = (0..81)
        .filter(|&position| current_mask & (1u128 << position) != 0)
        .collect();
    rng.shuffle(&mut additions);
    rng.shuffle(&mut removals);
    for addition in additions {
        for &removal in &removals {
            let candidate = (current_mask & !(1u128 << removal)) | (1u128 << addition);
            if !visited_masks.contains(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn drift_walk_add_probability(clues: usize, max_clues: usize) -> f64 {
    if clues == 0 {
        return 1.0;
    }
    if clues >= max_clues {
        return 0.0;
    }
    (0.5 - (clues as f64 - MIN_CLUE_COUNT as f64) * 0.05).clamp(0.1, 0.9)
}

/// Search a seeded random walk whose individual transitions add or remove one clue.
/// The walk may pass through multi-solution states; it keeps a valid completion as
/// its active witness and only accepts a state after proving it uniquely solvable.
fn generate_diverse_drift_walk(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }

    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);
    let parent = transform.apply(&parent);
    let parent_solution = transform.apply(&parent_solution);
    let parent_key = canonical_key(&parent);
    let mut puzzle = parent;
    let mut active_solution = parent_solution;
    let mut nodes = initial_nodes;
    let mut checks = 0u64;
    let mut checked_target_states = HashSet::new();

    while checks < budget.max_neighborhood_checks {
        let clues = puzzle
            .iter()
            .flatten()
            .filter(|&&value| (1..=9).contains(&value))
            .count();
        let add_probability = drift_walk_add_probability(clues, budget.max_intermediate_clues);
        let add = (rng.next() % 1_000) as f64 / 1_000.0 < add_probability;

        if add {
            let empty: Vec<_> = (0..81)
                .filter(|&position| puzzle[position / 9][position % 9] == 0)
                .collect();
            if empty.is_empty() {
                break;
            }
            let position = empty[(rng.next() as usize) % empty.len()];
            puzzle[position / 9][position % 9] = active_solution[position / 9][position % 9];
        } else {
            let clues: Vec<_> = (0..81)
                .filter(|&position| puzzle[position / 9][position % 9] != 0)
                .collect();
            if clues.is_empty() {
                break;
            }
            let position = clues[(rng.next() as usize) % clues.len()];
            puzzle[position / 9][position % 9] = 0;
        }
        checks += 1;

        let clue_count = if add { clues + 1 } else { clues - 1 };
        let should_probe = !add || clue_count == MIN_CLUE_COUNT;
        if should_probe {
            match find_alternative_solution(
                &puzzle,
                &active_solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                    if clue_count == MIN_CLUE_COUNT {
                        let key = canonical_key(&puzzle);
                        if key != parent_key && checked_target_states.insert(key) {
                            emit_generation_progress(
                                on_progress,
                                checks as u32,
                                clue_count,
                                nodes,
                                checks,
                                DiverseStrategy::DriftWalk,
                            );
                            return Ok(puzzle);
                        }
                    }
                }
                ProbeResult::Multiple {
                    alternative,
                    nodes: used,
                } => {
                    nodes = nodes.saturating_add(used);
                    if rng.next() & 1 == 1 {
                        active_solution = alternative;
                    }
                }
                ProbeResult::Unknown { nodes: used } | ProbeResult::Invalid { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                }
            }
        }

        emit_generation_progress(
            on_progress,
            checks as u32,
            clue_count,
            nodes,
            checks,
            DiverseStrategy::DriftWalk,
        );
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

/// Add one clue first using every digit value, then remove one original clue.
/// The added value may conflict with the parent puzzle; only the final 17-clue
/// board is required to be valid and uniquely solvable.
fn generate_diverse_plus_one_minus_one(
    seed: u64,
    parent: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }

    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);
    let parent = transform.apply(&parent);
    let parent_key = canonical_key(&parent);
    let mut clue_positions = Vec::with_capacity(MIN_CLUE_COUNT);
    let mut empty_positions = Vec::with_capacity(81 - MIN_CLUE_COUNT);
    for position in 0..81 {
        if parent[position / 9][position % 9] == 0 {
            empty_positions.push(position);
        } else {
            clue_positions.push(position);
        }
    }
    rng.shuffle(&mut clue_positions);
    rng.shuffle(&mut empty_positions);

    let mut digits: Vec<_> = (1..=9).collect();
    rng.shuffle(&mut digits);
    let mut nodes = initial_nodes;
    let mut checks = 0u64;
    'candidates: for &added in &empty_positions {
        for &digit in &digits {
            for &removed in &clue_positions {
                if checks >= budget.max_neighborhood_checks {
                    break 'candidates;
                }
                let mut candidate = parent;
                candidate[added / 9][added % 9] = digit;
                candidate[removed / 9][removed % 9] = 0;
                checks += 1;

                if let Ok(check) =
                    crate::check_uniqueness(&candidate, budget.max_nodes_per_uniqueness_check)
                {
                    nodes = nodes.saturating_add(check.explored_nodes);
                    if check.status == crate::UniquenessStatus::Unique {
                        let mut completion = candidate;
                        if crate::solve(&mut completion) && canonical_key(&candidate) != parent_key
                        {
                            emit_generation_progress(
                                on_progress,
                                1,
                                MIN_CLUE_COUNT,
                                nodes,
                                checks,
                                DiverseStrategy::PlusOneMinusOne,
                            );
                            return Ok(candidate);
                        }
                    }
                }

                emit_generation_progress(
                    on_progress,
                    1,
                    MIN_CLUE_COUNT,
                    nodes,
                    checks,
                    DiverseStrategy::PlusOneMinusOne,
                );
            }
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

fn generate_diverse_swap_one(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }

    let mut rng = SplitMix64(seed);
    let mut total_nodes = initial_nodes;
    let mut checks = 0u64;
    let mut attempt = 0u32;
    let mut base_key = None;
    let preserving_limit = (budget.max_neighborhood_checks * 2 / 3).max(1);
    while attempt == 0 && checks < preserving_limit {
        attempt += 1;
        let transform = SudokuTransform::random(&mut rng);
        let transformed_parent = transform.apply(&parent);
        let solution = transform.apply(&parent_solution);
        let parent_key = base_key.get_or_insert_with(|| canonical_key(&transformed_parent));

        let mut removals = [0usize; MIN_CLUE_COUNT];
        let mut removal_count = 0;
        let mut additions = [0usize; 81 - MIN_CLUE_COUNT];
        let mut addition_count = 0;
        for position in 0..81 {
            if transformed_parent[position / 9][position % 9] == 0 {
                additions[addition_count] = position;
                addition_count += 1;
            } else {
                removals[removal_count] = position;
                removal_count += 1;
            }
        }
        rng.shuffle(&mut removals);
        rng.shuffle(&mut additions);

        'removals: for removed in removals {
            let mut reduced = transformed_parent;
            reduced[removed / 9][removed % 9] = 0;
            let mut eligible = additions.to_vec();
            rng.shuffle(&mut eligible);
            while let Some(added) = eligible.pop() {
                let remaining = preserving_limit.saturating_sub(checks);
                if remaining == 0 {
                    break 'removals;
                }
                let (y, x) = (added / 9, added % 9);
                let mut candidate = reduced;
                candidate[y][x] = solution[y][x];
                checks += 1;
                match find_alternative_solution(
                    &candidate,
                    &solution,
                    budget.max_nodes_per_uniqueness_check,
                ) {
                    ProbeResult::Unique { nodes } => {
                        total_nodes = total_nodes.saturating_add(nodes);
                        let key = canonical_key(&candidate);
                        if key != *parent_key {
                            emit_generation_progress(
                                on_progress,
                                attempt,
                                MIN_CLUE_COUNT,
                                total_nodes,
                                checks,
                                DiverseStrategy::SwapOne,
                            );
                            return Ok(candidate);
                        }
                    }
                    ProbeResult::Multiple { alternative, nodes } => {
                        total_nodes = total_nodes.saturating_add(nodes);
                        // The witness is a valid completion of `reduced`.
                        // Any added clue where it agrees with `solution` keeps
                        // this witness alive, so that position cannot work.
                        prune_witness_survivors(&solution, &alternative, &mut eligible);
                    }
                    ProbeResult::Unknown { nodes } | ProbeResult::Invalid { nodes } => {
                        total_nodes = total_nodes.saturating_add(nodes);
                    }
                }
                on_progress(DiverseProgress {
                    attempt,
                    clues_remaining: MIN_CLUE_COUNT,
                    uniqueness_nodes: total_nodes,
                    hitting_set_nodes: 0,
                    neighborhood_checks: checks,
                    strategy: DiverseStrategy::SwapOne,
                });
            }
        }
    }

    let remaining = budget.max_neighborhood_checks.saturating_sub(checks);
    if remaining != 0 {
        let transform = SudokuTransform::random(&mut rng);
        let transformed_parent = transform.apply(&parent);
        let solution = transform.apply(&parent_solution);
        let parent_key = canonical_key(&transformed_parent);
        let mut removals = [0usize; MIN_CLUE_COUNT];
        let mut additions = [0usize; 81 - MIN_CLUE_COUNT];
        let (mut removal_count, mut addition_count) = (0, 0);
        for position in 0..81 {
            if transformed_parent[position / 9][position % 9] == 0 {
                additions[addition_count] = position;
                addition_count += 1;
            } else {
                removals[removal_count] = position;
                removal_count += 1;
            }
        }
        rng.shuffle(&mut removals);
        rng.shuffle(&mut additions);
        'changing: for removed in removals {
            let mut reduced = transformed_parent;
            reduced[removed / 9][removed % 9] = 0;
            for &added in &additions {
                let (y, x) = (added / 9, added % 9);
                let allowed = crate::candidates_for(&reduced, x, y);
                let mut digits: Vec<u8> = (1..=9)
                    .filter(|&digit| digit != solution[y][x] && allowed.contains(&digit))
                    .collect();
                rng.shuffle(&mut digits);
                for digit in digits {
                    if checks >= budget.max_neighborhood_checks {
                        break 'changing;
                    }
                    let mut candidate = reduced;
                    candidate[y][x] = digit;
                    checks += 1;
                    let uniqueness =
                        crate::check_uniqueness(&candidate, budget.max_nodes_per_uniqueness_check);
                    if let Ok(check) = uniqueness {
                        total_nodes = total_nodes.saturating_add(check.explored_nodes);
                        if check.status == crate::UniquenessStatus::Unique {
                            let mut completion = candidate;
                            if crate::solve(&mut completion)
                                && completion != solution
                                && canonical_key(&candidate) != parent_key
                            {
                                emit_generation_progress(
                                    on_progress,
                                    attempt.max(1),
                                    MIN_CLUE_COUNT,
                                    total_nodes,
                                    checks,
                                    DiverseStrategy::SwapOne,
                                );
                                return Ok(candidate);
                            }
                        }
                    }
                    on_progress(DiverseProgress {
                        attempt: attempt.max(1),
                        clues_remaining: MIN_CLUE_COUNT,
                        uniqueness_nodes: total_nodes,
                        hitting_set_nodes: 0,
                        neighborhood_checks: checks,
                        strategy: DiverseStrategy::SwapOne,
                    });
                }
            }
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: attempt,
        uniqueness_nodes: total_nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

fn generate_diverse_swap_two(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }
    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);
    let parent = transform.apply(&parent);
    let solution = transform.apply(&parent_solution);
    let parent_key = canonical_key(&parent);
    let mut clue_positions = Vec::with_capacity(MIN_CLUE_COUNT);
    let mut empty_positions = Vec::with_capacity(81 - MIN_CLUE_COUNT);
    for position in 0..81 {
        if parent[position / 9][position % 9] == 0 {
            empty_positions.push(position);
        } else {
            clue_positions.push(position);
        }
    }
    let mut removals = position_pairs(&clue_positions);
    let mut additions = position_pairs(&empty_positions);
    rng.shuffle(&mut removals);
    rng.shuffle(&mut additions);

    let preserving_limit = (budget.max_neighborhood_checks * 2 / 3).max(1);
    let mut nodes = initial_nodes;
    let mut checks = 0u64;
    let mut witness_cache = UnavoidableWitnessCache::default();
    'preserving: for &(removed_a, removed_b) in &removals {
        if checks >= preserving_limit {
            break;
        }
        let mut reduced = parent;
        reduced[removed_a / 9][removed_a % 9] = 0;
        reduced[removed_b / 9][removed_b % 9] = 0;
        let core_empties: Vec<_> = (0..81)
            .filter(|&position| reduced[position / 9][position % 9] == 0)
            .collect();
        let mut core_pairs = position_pairs(&core_empties);
        rng.shuffle(&mut core_pairs);
        let mut candidates = PairCandidateSet::new(core_pairs);
        let mut witnesses = DifferenceAntichain::default();
        for alternative in witness_cache.compatible_with(&reduced) {
            witnesses.insert(solution_difference(&solution, alternative));
        }
        witnesses.prune_pairs(&mut candidates);
        match find_alternative_solution(&reduced, &solution, budget.max_nodes_per_uniqueness_check)
        {
            ProbeResult::Unique { nodes: used } => {
                nodes = nodes.saturating_add(used);
                // A unique 15-clue state stays unique after either clue is added.
                while let Some((added_a, added_b)) = candidates.pop() {
                    if checks >= preserving_limit {
                        break 'preserving;
                    }
                    let candidate = with_two_solution_clues(&reduced, &solution, added_a, added_b);
                    checks += 1;
                    if canonical_key(&candidate) != parent_key {
                        emit_generation_progress(
                            on_progress,
                            1,
                            MIN_CLUE_COUNT,
                            nodes,
                            checks,
                            DiverseStrategy::SwapTwo,
                        );
                        return Ok(candidate);
                    }
                }
                continue;
            }
            ProbeResult::Multiple {
                alternative,
                nodes: used,
            } => {
                nodes = nodes.saturating_add(used);
                witness_cache.insert(alternative);
                witnesses.insert(solution_difference(&solution, &alternative));
                witnesses.prune_pairs(&mut candidates);
            }
            ProbeResult::Unknown { nodes: used } | ProbeResult::Invalid { nodes: used } => {
                nodes = nodes.saturating_add(used);
            }
        }

        while let Some((added_a, added_b)) = candidates.pop() {
            if checks >= preserving_limit {
                break 'preserving;
            }
            let candidate = with_two_solution_clues(&reduced, &solution, added_a, added_b);
            checks += 1;
            match find_alternative_solution(
                &candidate,
                &solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                    if canonical_key(&candidate) != parent_key {
                        emit_generation_progress(
                            on_progress,
                            1,
                            MIN_CLUE_COUNT,
                            nodes,
                            checks,
                            DiverseStrategy::SwapTwo,
                        );
                        return Ok(candidate);
                    }
                }
                ProbeResult::Multiple {
                    alternative,
                    nodes: used,
                } => {
                    nodes = nodes.saturating_add(used);
                    witness_cache.insert(alternative);
                    witnesses.insert(solution_difference(&solution, &alternative));
                    witnesses.prune_pairs(&mut candidates);
                }
                ProbeResult::Unknown { nodes: used } | ProbeResult::Invalid { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                }
            }
            on_progress(DiverseProgress {
                attempt: 1,
                clues_remaining: MIN_CLUE_COUNT,
                uniqueness_nodes: nodes,
                hitting_set_nodes: 0,
                neighborhood_checks: checks,
                strategy: DiverseStrategy::SwapTwo,
            });
        }
    }

    // The changing mode uses the remaining finite budget. Candidate digits are
    // filtered by row, column, and block masks before exact uniqueness checks.
    'changing: for &(removed_a, removed_b) in &removals {
        let mut reduced = parent;
        reduced[removed_a / 9][removed_a % 9] = 0;
        reduced[removed_b / 9][removed_b % 9] = 0;
        for &(added_a, added_b) in &additions {
            let (ay, ax) = (added_a / 9, added_a % 9);
            let (by, bx) = (added_b / 9, added_b % 9);
            let digits_a = crate::candidates_for(&reduced, ax, ay);
            let digits_b = crate::candidates_for(&reduced, bx, by);
            for &digit_a in &digits_a {
                for &digit_b in &digits_b {
                    if checks >= budget.max_neighborhood_checks {
                        break 'changing;
                    }
                    if digit_a == solution[ay][ax] && digit_b == solution[by][bx] {
                        continue;
                    }
                    let mut candidate = reduced;
                    candidate[ay][ax] = digit_a;
                    candidate[by][bx] = digit_b;
                    checks += 1;
                    if let Ok(check) =
                        crate::check_uniqueness(&candidate, budget.max_nodes_per_uniqueness_check)
                    {
                        nodes = nodes.saturating_add(check.explored_nodes);
                        if check.status == crate::UniquenessStatus::Unique {
                            let mut completion = candidate;
                            if crate::solve(&mut completion)
                                && completion != solution
                                && canonical_key(&candidate) != parent_key
                            {
                                emit_generation_progress(
                                    on_progress,
                                    1,
                                    MIN_CLUE_COUNT,
                                    nodes,
                                    checks,
                                    DiverseStrategy::SwapTwo,
                                );
                                return Ok(candidate);
                            }
                        }
                    }
                    on_progress(DiverseProgress {
                        attempt: 1,
                        clues_remaining: MIN_CLUE_COUNT,
                        uniqueness_nodes: nodes,
                        hitting_set_nodes: 0,
                        neighborhood_checks: checks,
                        strategy: DiverseStrategy::SwapTwo,
                    });
                }
            }
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

fn generate_diverse_swap_three(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }
    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);
    let parent = transform.apply(&parent);
    let solution = transform.apply(&parent_solution);
    let parent_key = canonical_key(&parent);
    let mut clue_positions = Vec::with_capacity(MIN_CLUE_COUNT);
    let mut empty_positions = Vec::with_capacity(81 - MIN_CLUE_COUNT);
    for position in 0..81 {
        if parent[position / 9][position % 9] == 0 {
            empty_positions.push(position);
        } else {
            clue_positions.push(position);
        }
    }
    let mut removals: Vec<_> = PositionCombinations::new(&clue_positions, 3)
        .map(|triple| (triple[0], triple[1], triple[2]))
        .collect();
    let mut additions: Vec<_> = PositionCombinations::new(&empty_positions, 3)
        .map(|triple| (triple[0], triple[1], triple[2]))
        .collect();
    rng.shuffle(&mut removals);
    rng.shuffle(&mut additions);

    let preserving_limit = (budget.max_neighborhood_checks * 2 / 3).max(1);
    let mut nodes = initial_nodes;
    let mut checks = 0u64;
    'preserving: for &(removed_a, removed_b, removed_c) in &removals {
        if checks >= preserving_limit {
            break;
        }
        let mut reduced = parent;
        reduced[removed_a / 9][removed_a % 9] = 0;
        reduced[removed_b / 9][removed_b % 9] = 0;
        reduced[removed_c / 9][removed_c % 9] = 0;
        let mut candidates = additions.clone();
        match find_alternative_solution(&reduced, &solution, budget.max_nodes_per_uniqueness_check)
        {
            ProbeResult::Unique { nodes: used } => {
                nodes = nodes.saturating_add(used);
                for &(added_a, added_b, added_c) in &candidates {
                    if checks >= preserving_limit {
                        break 'preserving;
                    }
                    let candidate =
                        with_three_solution_clues(&reduced, &solution, added_a, added_b, added_c);
                    checks += 1;
                    if canonical_key(&candidate) != parent_key {
                        emit_generation_progress(
                            on_progress,
                            1,
                            MIN_CLUE_COUNT,
                            nodes,
                            checks,
                            DiverseStrategy::SwapThree,
                        );
                        return Ok(candidate);
                    }
                }
                continue;
            }
            ProbeResult::Multiple {
                alternative,
                nodes: used,
            } => {
                nodes = nodes.saturating_add(used);
                prune_triple_witnesses(&solution, &alternative, &mut candidates);
            }
            ProbeResult::Unknown { nodes: used } | ProbeResult::Invalid { nodes: used } => {
                nodes = nodes.saturating_add(used);
            }
        }

        while let Some((added_a, added_b, added_c)) = candidates.pop() {
            if checks >= preserving_limit {
                break 'preserving;
            }
            let candidate =
                with_three_solution_clues(&reduced, &solution, added_a, added_b, added_c);
            checks += 1;
            match find_alternative_solution(
                &candidate,
                &solution,
                budget.max_nodes_per_uniqueness_check,
            ) {
                ProbeResult::Unique { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                    if canonical_key(&candidate) != parent_key {
                        emit_generation_progress(
                            on_progress,
                            1,
                            MIN_CLUE_COUNT,
                            nodes,
                            checks,
                            DiverseStrategy::SwapThree,
                        );
                        return Ok(candidate);
                    }
                }
                ProbeResult::Multiple {
                    alternative,
                    nodes: used,
                } => {
                    nodes = nodes.saturating_add(used);
                    prune_triple_witnesses(&solution, &alternative, &mut candidates);
                }
                ProbeResult::Unknown { nodes: used } | ProbeResult::Invalid { nodes: used } => {
                    nodes = nodes.saturating_add(used);
                }
            }
            on_progress(DiverseProgress {
                attempt: 1,
                clues_remaining: MIN_CLUE_COUNT,
                uniqueness_nodes: nodes,
                hitting_set_nodes: 0,
                neighborhood_checks: checks,
                strategy: DiverseStrategy::SwapThree,
            });
        }
    }

    'changing: for &(removed_a, removed_b, removed_c) in &removals {
        let mut reduced = parent;
        reduced[removed_a / 9][removed_a % 9] = 0;
        reduced[removed_b / 9][removed_b % 9] = 0;
        reduced[removed_c / 9][removed_c % 9] = 0;
        for &(added_a, added_b, added_c) in &additions {
            let (ay, ax) = (added_a / 9, added_a % 9);
            let (by, bx) = (added_b / 9, added_b % 9);
            let (cy, cx) = (added_c / 9, added_c % 9);
            let digits_a = crate::candidates_for(&reduced, ax, ay);
            for &digit_a in &digits_a {
                let mut with_a = reduced;
                with_a[ay][ax] = digit_a;
                let digits_b = crate::candidates_for(&with_a, bx, by);
                for &digit_b in &digits_b {
                    let mut with_ab = with_a;
                    with_ab[by][bx] = digit_b;
                    let digits_c = crate::candidates_for(&with_ab, cx, cy);
                    for &digit_c in &digits_c {
                        if checks >= budget.max_neighborhood_checks {
                            break 'changing;
                        }
                        if digit_a == solution[ay][ax]
                            && digit_b == solution[by][bx]
                            && digit_c == solution[cy][cx]
                        {
                            continue;
                        }
                        let mut candidate = with_ab;
                        candidate[cy][cx] = digit_c;
                        checks += 1;
                        if let Ok(check) = crate::check_uniqueness(
                            &candidate,
                            budget.max_nodes_per_uniqueness_check,
                        ) {
                            nodes = nodes.saturating_add(check.explored_nodes);
                            if check.status == crate::UniquenessStatus::Unique {
                                let mut completion = candidate;
                                if crate::solve(&mut completion)
                                    && completion != solution
                                    && canonical_key(&candidate) != parent_key
                                {
                                    emit_generation_progress(
                                        on_progress,
                                        1,
                                        MIN_CLUE_COUNT,
                                        nodes,
                                        checks,
                                        DiverseStrategy::SwapThree,
                                    );
                                    return Ok(candidate);
                                }
                            }
                        }
                        on_progress(DiverseProgress {
                            attempt: 1,
                            clues_remaining: MIN_CLUE_COUNT,
                            uniqueness_nodes: nodes,
                            hitting_set_nodes: 0,
                            neighborhood_checks: checks,
                            strategy: DiverseStrategy::SwapThree,
                        });
                    }
                }
            }
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

fn position_pairs(positions: &[usize]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::with_capacity(positions.len() * positions.len() / 2);
    for first in 0..positions.len() {
        for second in first + 1..positions.len() {
            pairs.push((positions[first], positions[second]));
        }
    }
    pairs
}

fn with_two_solution_clues(
    reduced: &Matrix,
    solution: &Matrix,
    first: usize,
    second: usize,
) -> Matrix {
    let mut candidate = *reduced;
    candidate[first / 9][first % 9] = solution[first / 9][first % 9];
    candidate[second / 9][second % 9] = solution[second / 9][second % 9];
    candidate
}

fn with_three_solution_clues(
    reduced: &Matrix,
    solution: &Matrix,
    first: usize,
    second: usize,
    third: usize,
) -> Matrix {
    let mut candidate = *reduced;
    for position in [first, second, third] {
        candidate[position / 9][position % 9] = solution[position / 9][position % 9];
    }
    candidate
}

fn prune_triple_witnesses(
    solution: &Matrix,
    alternative: &Matrix,
    candidates: &mut Vec<(usize, usize, usize)>,
) {
    candidates.retain(|&(first, second, third)| {
        solution[first / 9][first % 9] != alternative[first / 9][first % 9]
            || solution[second / 9][second % 9] != alternative[second / 9][second % 9]
            || solution[third / 9][third % 9] != alternative[third / 9][third % 9]
    });
}

struct PositionCombinations<'a> {
    positions: &'a [usize],
    indices: Vec<usize>,
    first: bool,
    done: bool,
}

impl<'a> PositionCombinations<'a> {
    fn new(positions: &'a [usize], choose: usize) -> Self {
        Self {
            positions,
            indices: (0..choose).collect(),
            first: true,
            done: choose > positions.len(),
        }
    }
}

impl Iterator for PositionCombinations<'_> {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.first {
            self.first = false;
            return Some(
                self.indices
                    .iter()
                    .map(|&index| self.positions[index])
                    .collect(),
            );
        }
        if self.indices.is_empty() {
            self.done = true;
            return None;
        }

        let choose = self.indices.len();
        let Some(index) = (0..choose)
            .rev()
            .find(|&index| self.indices[index] < self.positions.len() - choose + index)
        else {
            self.done = true;
            return None;
        };
        self.indices[index] += 1;
        for next in index + 1..choose {
            self.indices[next] = self.indices[next - 1] + 1;
        }
        Some(
            self.indices
                .iter()
                .map(|&index| self.positions[index])
                .collect(),
        )
    }
}

fn generate_diverse_via_intermediate(
    seed: u64,
    parent: Matrix,
    parent_solution: Matrix,
    initial_nodes: u64,
    bridge_clues: usize,
    budget: DiverseBudget,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Result<Matrix, DiverseGenerationError> {
    use crate::symmetry::canonical_key;

    if !(18..=MAX_INTERMEDIATE_CLUE_COUNT).contains(&bridge_clues) {
        return Err(DiverseGenerationError::InvalidIntermediateClueCount);
    }
    if budget.max_neighborhood_checks == 0 {
        return Err(DiverseGenerationError::EmptyBudget);
    }

    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);
    let parent = transform.apply(&parent);
    let solution = transform.apply(&parent_solution);
    let parent_key = canonical_key(&parent);
    let mut parent_clues = Vec::with_capacity(MIN_CLUE_COUNT);
    let mut empty_positions = Vec::with_capacity(81 - MIN_CLUE_COUNT);
    for position in 0..81 {
        if parent[position / 9][position % 9] == 0 {
            empty_positions.push(position);
        } else {
            parent_clues.push(position);
        }
    }
    rng.shuffle(&mut parent_clues);
    rng.shuffle(&mut empty_positions);

    let extra_clues = bridge_clues - MIN_CLUE_COUNT;
    let mut nodes = initial_nodes;
    let mut checks = 0u64;
    'paths: for initial_additions in PositionCombinations::new(&empty_positions, extra_clues) {
        let mut bridge = parent;
        for &position in &initial_additions {
            bridge[position / 9][position % 9] = solution[position / 9][position % 9];
        }

        for &removed in &parent_clues {
            let mut after_remove = bridge;
            after_remove[removed / 9][removed % 9] = 0;
            for &second_added in &empty_positions {
                if initial_additions.contains(&second_added) {
                    continue;
                }
                if checks >= budget.max_neighborhood_checks {
                    break 'paths;
                }
                let (y, x) = (second_added / 9, second_added % 9);
                let preserving_digit = solution[y][x];

                // Try a changed value first so Adaptive spends its early
                // bridge budget on paths that can reach a different solution.
                let allowed = crate::candidates_for(&after_remove, x, y);
                let mut changed_digits: Vec<_> = allowed
                    .into_iter()
                    .filter(|&digit| digit != preserving_digit)
                    .collect();
                rng.shuffle(&mut changed_digits);
                for digit in changed_digits {
                    if checks >= budget.max_neighborhood_checks {
                        break 'paths;
                    }
                    let mut changed_bridge = after_remove;
                    changed_bridge[y][x] = digit;
                    checks += 1;
                    let Ok(check) = crate::check_uniqueness(
                        &changed_bridge,
                        budget.max_nodes_per_uniqueness_check,
                    ) else {
                        continue;
                    };
                    nodes = nodes.saturating_add(check.explored_nodes);
                    if check.status == crate::UniquenessStatus::Unique {
                        let mut changed_solution = changed_bridge;
                        if crate::solve(&mut changed_solution) && changed_solution != solution {
                            if let Some(candidate) = search_bridge_reductions(
                                &changed_bridge,
                                &changed_solution,
                                &solution,
                                &parent_key,
                                extra_clues,
                                bridge_clues,
                                true,
                                budget,
                                &mut nodes,
                                &mut checks,
                                on_progress,
                            ) {
                                return Ok(candidate);
                            }
                        }
                    }
                    emit_generation_progress(
                        on_progress,
                        1,
                        bridge_clues,
                        nodes,
                        checks,
                        DiverseStrategy::ViaIntermediate,
                    );
                }

                if checks >= budget.max_neighborhood_checks {
                    break 'paths;
                }
                let mut preserving_bridge = after_remove;
                preserving_bridge[y][x] = preserving_digit;
                checks += 1;
                match find_alternative_solution(
                    &preserving_bridge,
                    &solution,
                    budget.max_nodes_per_uniqueness_check,
                ) {
                    ProbeResult::Unique { nodes: used } => {
                        nodes = nodes.saturating_add(used);
                        if let Some(candidate) = search_bridge_reductions(
                            &preserving_bridge,
                            &solution,
                            &solution,
                            &parent_key,
                            extra_clues,
                            bridge_clues,
                            false,
                            budget,
                            &mut nodes,
                            &mut checks,
                            on_progress,
                        ) {
                            return Ok(candidate);
                        }
                    }
                    ProbeResult::Multiple { nodes: used, .. }
                    | ProbeResult::Unknown { nodes: used }
                    | ProbeResult::Invalid { nodes: used } => {
                        nodes = nodes.saturating_add(used);
                    }
                }
                emit_generation_progress(
                    on_progress,
                    1,
                    bridge_clues,
                    nodes,
                    checks,
                    DiverseStrategy::ViaIntermediate,
                );
            }
        }
    }

    Err(DiverseGenerationError::BudgetExhausted {
        attempts: 1,
        uniqueness_nodes: nodes,
        hitting_set_nodes: 0,
        neighborhood_checks: checks,
    })
}

#[allow(clippy::too_many_arguments)]
fn search_bridge_reductions(
    bridge: &Matrix,
    bridge_solution: &Matrix,
    parent_solution: &Matrix,
    parent_key: &str,
    clues_to_remove: usize,
    bridge_clues: usize,
    require_solution_change: bool,
    budget: DiverseBudget,
    nodes: &mut u64,
    checks: &mut u64,
    on_progress: &mut impl FnMut(DiverseProgress),
) -> Option<Matrix> {
    use crate::symmetry::canonical_key;

    let positions: Vec<_> = (0..81)
        .filter(|&position| bridge[position / 9][position % 9] != 0)
        .collect();
    for removed in PositionCombinations::new(&positions, clues_to_remove) {
        if *checks >= budget.max_neighborhood_checks {
            break;
        }
        let mut candidate = *bridge;
        for position in removed {
            candidate[position / 9][position % 9] = 0;
        }
        *checks += 1;
        match find_alternative_solution(
            &candidate,
            bridge_solution,
            budget.max_nodes_per_uniqueness_check,
        ) {
            ProbeResult::Unique { nodes: used } => {
                *nodes = nodes.saturating_add(used);
                if require_solution_change && bridge_solution == parent_solution {
                    continue;
                }
                if canonical_key(&candidate).as_str() != parent_key {
                    let mut completion = candidate;
                    if crate::solve(&mut completion)
                        && completion == *bridge_solution
                        && (!require_solution_change || completion != *parent_solution)
                    {
                        return Some(candidate);
                    }
                }
            }
            ProbeResult::Multiple { nodes: used, .. }
            | ProbeResult::Unknown { nodes: used }
            | ProbeResult::Invalid { nodes: used } => {
                *nodes = nodes.saturating_add(used);
            }
        }
        emit_generation_progress(
            on_progress,
            1,
            bridge_clues,
            *nodes,
            *checks,
            DiverseStrategy::ViaIntermediate,
        );
    }
    None
}

#[cfg(test)]
fn prune_pair_witnesses(
    solution: &Matrix,
    alternative: &Matrix,
    candidates: &mut Vec<(usize, usize)>,
) {
    candidates.retain(|&(first, second)| {
        solution[first / 9][first % 9] != alternative[first / 9][first % 9]
            || solution[second / 9][second % 9] != alternative[second / 9][second % 9]
    });
}

fn prune_witness_survivors(solution: &Matrix, alternative: &Matrix, candidates: &mut Vec<usize>) {
    candidates.retain(|&position| {
        solution[position / 9][position % 9] != alternative[position / 9][position % 9]
    });
}

fn emit_generation_progress(
    on_progress: &mut impl FnMut(DiverseProgress),
    attempt: u32,
    clues_remaining: usize,
    uniqueness_nodes: u64,
    neighborhood_checks: u64,
    strategy: DiverseStrategy,
) {
    on_progress(DiverseProgress {
        attempt,
        clues_remaining,
        uniqueness_nodes,
        hitting_set_nodes: 0,
        neighborhood_checks,
        strategy,
    });
}

enum ProbeResult {
    Unique { nodes: u64 },
    Multiple { alternative: Matrix, nodes: u64 },
    Unknown { nodes: u64 },
    Invalid { nodes: u64 },
}

fn find_alternative_solution(
    board: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
) -> ProbeResult {
    let result = solver_find_alternative_solution(board, known_solution, max_nodes);
    match result.status {
        AlternativeSolutionStatus::AlternativeFound => ProbeResult::Multiple {
            alternative: result
                .alternative
                .expect("alternative status carries a solution"),
            nodes: result.explored_nodes,
        },
        AlternativeSolutionStatus::Unique => ProbeResult::Unique {
            nodes: result.explored_nodes,
        },
        AlternativeSolutionStatus::Unknown => ProbeResult::Unknown {
            nodes: result.explored_nodes,
        },
        AlternativeSolutionStatus::InvalidInput => ProbeResult::Invalid {
            nodes: result.explored_nodes,
        },
    }
}

fn add_unavoidable_set(solution: &Matrix, alternative: &Matrix, sets: &mut Vec<u128>) {
    let mut mask = 0u128;
    for position in 0..81 {
        if solution[position / 9][position % 9] != alternative[position / 9][position % 9] {
            mask |= 1u128 << position;
        }
    }
    if mask != 0 && !sets.contains(&mask) {
        sets.push(mask);
    }
}

fn board_for_clue_mask(solution: &Matrix, mask: u128) -> Matrix {
    std::array::from_fn(|y| {
        std::array::from_fn(|x| {
            if mask & (1u128 << (y * 9 + x)) != 0 {
                solution[y][x]
            } else {
                0
            }
        })
    })
}

fn find_hitting_set(
    sets: &[u128],
    target: usize,
    budget: u64,
    rng: &mut SplitMix64,
) -> (Option<u128>, u64) {
    if sets.is_empty() || budget == 0 {
        return (None, 0);
    }
    let mut order = std::array::from_fn::<_, 81, _>(|index| index);
    rng.shuffle(&mut order);
    let mut tie_rank = [0u8; 81];
    for (rank, position) in order.into_iter().enumerate() {
        tie_rank[position] = rank as u8;
    }
    let mut nodes = 0;
    let mut visited = HashSet::new();
    let result = hit_search(sets, target, budget, 0, &tie_rank, &mut nodes, &mut visited);
    let Some(mut mask) = result else {
        return (None, nodes);
    };
    let mut count = mask.count_ones() as usize;
    for position in order {
        if count == target {
            break;
        }
        let bit = 1u128 << position;
        if mask & bit == 0 {
            mask |= bit;
            count += 1;
        }
    }
    (Some(mask), nodes)
}

fn hit_search(
    sets: &[u128],
    target: usize,
    budget: u64,
    selected: u128,
    tie_rank: &[u8; 81],
    nodes: &mut u64,
    visited: &mut HashSet<u128>,
) -> Option<u128> {
    if *nodes >= budget || !visited.insert(selected) {
        return None;
    }
    *nodes += 1;
    let mut uncovered = Vec::new();
    for &set in sets {
        if set & selected == 0 {
            uncovered.push(set);
        }
    }
    if uncovered.is_empty() {
        return Some(selected);
    }
    let selected_count = selected.count_ones() as usize;
    if selected_count >= target {
        return None;
    }
    uncovered.sort_unstable_by_key(|set| set.count_ones());
    if selected_count + disjoint_lower_bound(&uncovered) > target {
        return None;
    }
    let constraint = uncovered[0];
    let mut choices: Vec<(u32, u8, u128)> = (0..81)
        .filter_map(|position| {
            let bit = 1u128 << position;
            (constraint & bit != 0).then(|| {
                let coverage = uncovered.iter().filter(|set| **set & bit != 0).count() as u32;
                (coverage, tie_rank[position], bit)
            })
        })
        .collect();
    choices.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, _, bit) in choices {
        if let Some(found) = hit_search(
            sets,
            target,
            budget,
            selected | bit,
            tie_rank,
            nodes,
            visited,
        ) {
            return Some(found);
        }
        if *nodes >= budget {
            break;
        }
    }
    None
}

fn disjoint_lower_bound(uncovered: &[u128]) -> usize {
    let mut used = 0u128;
    let mut count = 0;
    for &set in uncovered {
        if set & used == 0 {
            used |= set;
            count += 1;
        }
    }
    count
}

fn fill_random_solution(
    board: &mut Matrix,
    rows: &mut [u16; 9],
    columns: &mut [u16; 9],
    blocks: &mut [u16; 9],
    rng: &mut SplitMix64,
) -> bool {
    let mut best: Option<(usize, usize, u16)> = None;
    let mut best_count = 10;
    for y in 0..9 {
        for x in 0..9 {
            if board[y][x] != 0 {
                continue;
            }
            let block = (y / 3) * 3 + x / 3;
            let candidates = 0x01ff & !(rows[y] | columns[x] | blocks[block]);
            let count = candidates.count_ones();
            if count == 0 {
                return false;
            }
            if count < best_count {
                best = Some((y, x, candidates));
                best_count = count;
                if count == 1 {
                    break;
                }
            }
        }
        if best_count == 1 {
            break;
        }
    }
    let Some((y, x, mask)) = best else {
        return true;
    };
    let block = (y / 3) * 3 + x / 3;
    let mut candidates = [0u8; 9];
    let mut len = 0;
    for digit in 1..=9 {
        if mask & (1 << (digit - 1)) != 0 {
            candidates[len] = digit;
            len += 1;
        }
    }
    rng.shuffle(&mut candidates[..len]);
    for &digit in &candidates[..len] {
        let bit = 1 << (digit - 1);
        board[y][x] = digit;
        rows[y] |= bit;
        columns[x] |= bit;
        blocks[block] |= bit;
        if fill_random_solution(board, rows, columns, blocks, rng) {
            return true;
        }
        board[y][x] = 0;
        rows[y] &= !bit;
        columns[x] &= !bit;
        blocks[block] &= !bit;
    }
    false
}

struct SudokuTransform {
    rows: [usize; 9],
    columns: [usize; 9],
    digits: [u8; 10],
    transpose: bool,
}

impl SudokuTransform {
    fn random(rng: &mut SplitMix64) -> Self {
        let mut digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        rng.shuffle(&mut digits);
        let mut digit_map = [0; 10];
        for (digit, mapped_digit) in (1..=9).zip(digits) {
            digit_map[digit] = mapped_digit;
        }

        Self {
            rows: grouped_permutation(rng),
            columns: grouped_permutation(rng),
            digits: digit_map,
            transpose: rng.next() & 1 == 1,
        }
    }

    fn apply(&self, board: &Matrix) -> Matrix {
        let mut transformed = [[0; 9]; 9];
        if self.transpose {
            for y in 0..9 {
                for x in 0..9 {
                    let value = board[self.rows[y]][self.columns[x]];
                    transformed[x][y] = if value == 0 {
                        0
                    } else {
                        self.digits[value as usize]
                    };
                }
            }
        } else {
            for y in 0..9 {
                for x in 0..9 {
                    let value = board[self.rows[y]][self.columns[x]];
                    transformed[y][x] = if value == 0 {
                        0
                    } else {
                        self.digits[value as usize]
                    };
                }
            }
        }
        transformed
    }
}

fn grouped_permutation(rng: &mut SplitMix64) -> [usize; 9] {
    let mut groups = [0, 1, 2];
    rng.shuffle(&mut groups);
    let mut result = [0; 9];
    for (group_index, group) in groups.into_iter().enumerate() {
        let mut within_group = [0, 1, 2];
        rng.shuffle(&mut within_group);
        for (offset, item) in within_group.into_iter().enumerate() {
            result[group_index * 3 + offset] = group * 3 + item;
        }
    }
    result
}

struct SplitMix64(u64);

impl SplitMix64 {
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
        adaptive_schedule, add_unavoidable_set, drift_walk_add_probability,
        find_alternative_solution, find_hitting_set, generate, generate_diverse,
        generate_diverse_from_parent, generate_diverse_from_parent_with_progress, generate_puzzle,
        generate_puzzle_with_clues, minimal_puzzle_seeds, position_pairs, prune_pair_witnesses,
        prune_triple_witnesses, prune_witness_survivors, verified_17_clue_base,
        verified_minimal_base_up_to, with_three_solution_clues, witness_guided_swap,
        DifferenceAntichain, DiverseBudget, DiverseStrategy, GenerationConfig, GenerationMethod,
        PairCandidateSet, PositionCombinations, ProbeResult, SplitMix64, SudokuTransform,
        UnavoidableWitnessCache, DEFAULT_CLUE_COUNT, MAX_CLUE_COUNT, MINIMAL_PUZZLES_BY_CLUE_COUNT,
        MIN_CLUE_COUNT, UNAVOIDABLE_WITNESS_CACHE_CAPACITY,
    };
    use crate::{solve, SearchEvent, SolutionSearch};
    use std::collections::HashSet;

    fn clue_count(board: &crate::Matrix) -> usize {
        board
            .iter()
            .flatten()
            .filter(|&&value| (1..=9).contains(&value))
            .count()
    }

    fn solution_count(board: &crate::Matrix) -> usize {
        SolutionSearch::new(board)
            .unwrap()
            .filter(|event| matches!(event, SearchEvent::SolutionFound { .. }))
            .count()
    }

    #[test]
    fn generation_is_reproducible_for_a_seed_and_clue_count() {
        assert_eq!(
            generate_puzzle_with_clues(42, 30),
            generate_puzzle_with_clues(42, 30)
        );
    }

    #[test]
    fn verified_base_pool_has_consistent_17_clue_puzzles_and_covers_each_entry() {
        assert_eq!(MINIMAL_PUZZLES_BY_CLUE_COUNT.len(), 82);
        assert_eq!(MINIMAL_PUZZLES_BY_CLUE_COUNT[17].len(), 7);
        assert_eq!(MINIMAL_PUZZLES_BY_CLUE_COUNT[18].len(), 135);
        assert!(minimal_puzzle_seeds(u8::MAX).is_empty());
        let bases17 = minimal_puzzle_seeds(17);
        assert_eq!(bases17.len(), 7);
        for &(puzzle, solution) in bases17 {
            assert_eq!(clue_count(&puzzle), MIN_CLUE_COUNT);
            let mut solved = puzzle;
            assert!(solve(&mut solved));
            assert_eq!(solved, solution);
        }

        let bases18 = minimal_puzzle_seeds(18);
        assert_eq!(bases18.len(), 135);
        for (puzzle, solution) in bases18 {
            assert_eq!(clue_count(puzzle), 18);
            assert!((0..81).all(|position| {
                let clue = puzzle[position / 9][position % 9];
                clue == 0 || clue == solution[position / 9][position % 9]
            }));
        }

        let selected: std::collections::HashSet<_> =
            (0..128).map(|seed| verified_17_clue_base(seed).0).collect();
        assert_eq!(selected.len(), bases17.len());
    }

    #[test]
    fn fast_generation_selects_from_all_minimal_pools_up_to_the_target() {
        let mut selected_18 = HashSet::new();
        for seed in 0..10_000 {
            let mut rng = SplitMix64(seed);
            selected_18.insert(verified_minimal_base_up_to(&mut rng, 18).0);
        }
        assert_eq!(selected_18.len(), 7 + 135);

        let mut selected_17 = HashSet::new();
        for seed in 0..128 {
            let mut rng = SplitMix64(seed);
            selected_17.insert(verified_minimal_base_up_to(&mut rng, 17).0);
        }
        assert_eq!(selected_17.len(), 7);
    }

    #[test]
    fn adaptive_search_scales_intermediate_clue_counts_before_swap_routes() {
        assert_eq!(
            adaptive_schedule(20),
            [
                DiverseStrategy::ViaIntermediate,
                DiverseStrategy::ViaIntermediate,
                DiverseStrategy::ViaIntermediate,
                DiverseStrategy::SwapTwo,
                DiverseStrategy::SwapOne,
                DiverseStrategy::SwapThree,
            ]
        );
    }

    #[test]
    fn drift_walk_biases_each_direction_toward_seventeen_clues() {
        assert!(drift_walk_add_probability(16, 80) > 0.5);
        assert_eq!(drift_walk_add_probability(17, 80), 0.5);
        assert!(drift_walk_add_probability(18, 80) < 0.5);
        assert_eq!(drift_walk_add_probability(80, 80), 0.0);
        assert_eq!(drift_walk_add_probability(0, 80), 1.0);
    }

    #[test]
    fn position_combinations_iterate_without_materializing_the_search_space() {
        let positions = [4, 7, 9, 13];
        assert_eq!(
            PositionCombinations::new(&positions, 2).collect::<Vec<_>>(),
            vec![
                vec![4, 7],
                vec![4, 9],
                vec![4, 13],
                vec![7, 9],
                vec![7, 13],
                vec![9, 13]
            ]
        );
        assert_eq!(
            PositionCombinations::new(&positions, 0).collect::<Vec<_>>(),
            vec![Vec::<usize>::new()]
        );
        assert_eq!(PositionCombinations::new(&positions, 5).count(), 0);
    }

    #[test]
    fn triple_witness_pruning_keeps_only_sets_that_distinguish_the_solution() {
        let mut alternative = minimal_puzzle_seeds(17)[0].1;
        for position in [0, 4, 8] {
            let value = alternative[position / 9][position % 9];
            alternative[position / 9][position % 9] = if value == 1 { 2 } else { 1 };
        }
        let mut candidates = vec![(0, 1, 2), (1, 2, 3), (4, 5, 6), (9, 10, 11)];
        prune_triple_witnesses(
            &minimal_puzzle_seeds(17)[0].1,
            &alternative,
            &mut candidates,
        );
        assert_eq!(candidates, vec![(0, 1, 2), (4, 5, 6)]);
    }

    #[test]
    fn adding_three_known_solution_clues_preserves_the_selected_values() {
        let reduced = [[0; 9]; 9];
        let candidate =
            with_three_solution_clues(&reduced, &minimal_puzzle_seeds(17)[0].1, 0, 1, 2);
        assert_eq!(candidate[0][0..3], minimal_puzzle_seeds(17)[0].1[0][0..3]);
        assert_eq!(clue_count(&candidate), 3);
    }

    #[test]
    fn nineteen_clue_bridge_stage_respects_the_shared_candidate_budget() {
        let mut bridge_counts = Vec::new();
        let error = generate_diverse_from_parent_with_progress(
            19,
            minimal_puzzle_seeds(17)[0].0,
            minimal_puzzle_seeds(17)[0].1,
            DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: 100_000,
                max_hitting_set_nodes: 0,
                max_neighborhood_checks: 1,
                max_intermediate_clues: 19,
                strategy: DiverseStrategy::ViaIntermediate,
            },
            |progress| bridge_counts.push(progress.clues_remaining),
        )
        .expect_err("one bridge check cannot exhaust a 19-clue route");
        assert!(matches!(
            error,
            super::DiverseGenerationError::BudgetExhausted {
                neighborhood_checks: 2,
                ..
            }
        ));
        assert!(bridge_counts.contains(&18));
        assert!(bridge_counts.contains(&19));
    }

    #[test]
    fn generated_puzzle_has_one_solution_and_removes_clues() {
        let puzzle = generate_puzzle(7);
        assert_eq!(clue_count(&puzzle), DEFAULT_CLUE_COUNT);
        assert_eq!(solution_count(&puzzle), 1);
    }

    #[test]
    fn requested_clue_counts_are_exact_and_uniquely_solvable() {
        for requested_count in [MIN_CLUE_COUNT, 18, 19, 30, MAX_CLUE_COUNT] {
            let puzzle = generate_puzzle_with_clues(7, requested_count).unwrap();
            assert_eq!(clue_count(&puzzle), requested_count);
            assert_eq!(solution_count(&puzzle), 1);
        }
    }

    #[test]
    fn generated_puzzles_keep_their_clues_and_match_both_solver_apis() {
        for seed in [0, 1, u64::MAX] {
            for requested_count in [MIN_CLUE_COUNT, DEFAULT_CLUE_COUNT, MAX_CLUE_COUNT] {
                let puzzle = generate_puzzle_with_clues(seed, requested_count).unwrap();
                assert_eq!(clue_count(&puzzle), requested_count);

                let mut solved = puzzle;
                assert!(solve(&mut solved));
                for (given_row, solved_row) in puzzle.iter().zip(solved.iter()) {
                    for (&given, &value) in given_row.iter().zip(solved_row.iter()) {
                        assert!(given == 0 || given == value);
                    }
                }

                let solutions: Vec<_> = SolutionSearch::new(&puzzle)
                    .unwrap()
                    .filter_map(|event| match event {
                        SearchEvent::SolutionFound { board, .. } => Some(board),
                        SearchEvent::Progress { .. } => None,
                    })
                    .collect();
                assert_eq!(
                    solutions,
                    vec![solved],
                    "seed={seed}, clues={requested_count}"
                );
            }
        }
    }

    #[test]
    fn rejects_clue_counts_outside_the_supported_range() {
        assert!(generate_puzzle_with_clues(7, MIN_CLUE_COUNT - 1).is_err());
        assert!(generate_puzzle_with_clues(7, MAX_CLUE_COUNT + 1).is_err());
    }

    #[test]
    fn diverse_generation_returns_exact_reproducible_unique_puzzles() {
        let budget = DiverseBudget {
            max_attempts: 2,
            max_nodes_per_uniqueness_check: 100_000,
            max_hitting_set_nodes: 100_000,
            max_neighborhood_checks: 1_088,
            max_intermediate_clues: 18,
            strategy: DiverseStrategy::Adaptive,
        };
        let first = generate_diverse(937, 45, budget).unwrap();
        let second = generate_diverse(937, 45, budget).unwrap();
        assert_eq!(first, second);
        assert_eq!(clue_count(&first), 45);
        assert_eq!(solution_count(&first), 1);
    }

    #[test]
    fn diverse_generation_reports_budget_failure_without_nearby_result() {
        let error = generate_diverse(
            12,
            17,
            DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: 1,
                max_hitting_set_nodes: 100,
                max_neighborhood_checks: 1_088,
                max_intermediate_clues: 18,
                strategy: DiverseStrategy::Adaptive,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            super::DiverseGenerationError::BudgetExhausted { .. }
        ));
    }

    #[test]
    fn unavoidable_set_hitting_search_finds_and_pads_a_valid_clue_set() {
        let constraints = [0b0000_0011u128, 0b0000_0110, 0b0000_1000];
        let mut rng = SplitMix64(71);
        let (selected, nodes) = find_hitting_set(&constraints, 3, 100, &mut rng);
        let selected = selected.expect("a three-cell hitting set exists");
        assert!(nodes > 0);
        assert_eq!(selected.count_ones(), 3);
        assert!(constraints.iter().all(|set| selected & set != 0));
    }

    #[test]
    fn alternative_solution_difference_becomes_an_unavoidable_set() {
        let mut alternative = minimal_puzzle_seeds(17)[0].1;
        for value in alternative.iter_mut().flatten() {
            if *value == 1 {
                *value = 2;
            } else if *value == 2 {
                *value = 1;
            }
        }
        let mut sets = Vec::new();
        add_unavoidable_set(&minimal_puzzle_seeds(17)[0].1, &alternative, &mut sets);
        assert_eq!(sets.len(), 1);
        assert!(sets[0].count_ones() > 0);
        add_unavoidable_set(&minimal_puzzle_seeds(17)[0].1, &alternative, &mut sets);
        assert_eq!(sets.len(), 1);
    }

    #[test]
    fn witness_guided_swap_adds_a_differing_cell_and_keeps_seventeen_clues() {
        let removed = (0..81)
            .find(|&position| minimal_puzzle_seeds(17)[0].0[position / 9][position % 9] != 0)
            .unwrap();
        let mut reduced = minimal_puzzle_seeds(17)[0].0;
        reduced[removed / 9][removed % 9] = 0;
        let ProbeResult::Multiple { alternative, .. } =
            find_alternative_solution(&reduced, &minimal_puzzle_seeds(17)[0].1, 100_000)
        else {
            panic!("expected a second completion after removing a clue");
        };
        let difference = (0..81).fold(0u128, |mask, position| {
            if minimal_puzzle_seeds(17)[0].1[position / 9][position % 9]
                != alternative[position / 9][position % 9]
            {
                mask | (1u128 << position)
            } else {
                mask
            }
        });
        let mut current_mask = 0u128;
        for position in 0..81 {
            if difference & (1u128 << position) == 0 && current_mask.count_ones() < 17 {
                current_mask |= 1u128 << position;
            }
        }
        assert_eq!(current_mask.count_ones(), 17);

        let mut rng = SplitMix64(5);
        let next = witness_guided_swap(
            current_mask,
            difference,
            &[difference],
            &HashSet::from([current_mask]),
            &mut rng,
        )
        .expect("a new witness-guided swap exists");
        assert_eq!(next.count_ones(), 17);
        assert_ne!(next, current_mask);
        assert_eq!((next ^ current_mask).count_ones(), 2);
        assert_ne!(next & difference, 0);
    }

    #[test]
    fn random_17_witness_strategy_obeys_its_probe_budget() {
        let result = generate_diverse(
            20_261_027,
            17,
            DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: 100_000,
                max_hitting_set_nodes: 0,
                max_neighborhood_checks: 1,
                max_intermediate_clues: 18,
                strategy: DiverseStrategy::Random17Witness,
            },
        );
        match result {
            Ok(puzzle) => {
                assert_eq!(clue_count(&puzzle), 17);
                assert_eq!(solution_count(&puzzle), 1);
            }
            Err(super::DiverseGenerationError::BudgetExhausted {
                neighborhood_checks,
                ..
            }) => assert_eq!(neighborhood_checks, 1),
            Err(error) => panic!("unexpected random-witness result: {error:?}"),
        }
    }

    #[test]
    fn counterexample_prunes_only_added_clues_that_leave_it_valid() {
        let removed = (0..81)
            .find(|&position| minimal_puzzle_seeds(17)[0].0[position / 9][position % 9] != 0)
            .unwrap();
        let mut reduced = minimal_puzzle_seeds(17)[0].0;
        reduced[removed / 9][removed % 9] = 0;
        let super::ProbeResult::Multiple { alternative, .. } =
            find_alternative_solution(&reduced, &minimal_puzzle_seeds(17)[0].1, 100_000)
        else {
            panic!("expected a counterexample completion");
        };
        let mut candidates: Vec<_> = (0..81)
            .filter(|&position| {
                minimal_puzzle_seeds(17)[0].0[position / 9][position % 9] == 0
                    && position != removed
            })
            .collect();
        let before = candidates.len();
        prune_witness_survivors(
            &minimal_puzzle_seeds(17)[0].1,
            &alternative,
            &mut candidates,
        );
        assert!(candidates.len() < before);
        assert!(candidates.iter().all(|&position| {
            minimal_puzzle_seeds(17)[0].1[position / 9][position % 9]
                != alternative[position / 9][position % 9]
        }));
    }

    #[test]
    fn two_swap_pairs_are_unique_and_counterexamples_filter_them_soundly() {
        let pairs = position_pairs(&[1, 4, 7]);
        assert_eq!(pairs, vec![(1, 4), (1, 7), (4, 7)]);
        let mut alternative = minimal_puzzle_seeds(17)[0].1;
        let first = alternative[0][0];
        alternative[0][0] = if first == 1 { 2 } else { 1 };
        let second = alternative[0][1];
        alternative[0][1] = if second == 1 { 2 } else { 1 };
        let mut candidates = vec![(0, 1), (0, 2), (2, 3)];
        prune_pair_witnesses(
            &minimal_puzzle_seeds(17)[0].1,
            &alternative,
            &mut candidates,
        );
        assert_eq!(candidates, vec![(0, 1), (0, 2)]);
    }

    #[test]
    fn pair_bitset_and_difference_antichain_prune_by_counterexamples() {
        let mut candidates = PairCandidateSet::new(vec![(1, 5), (2, 6), (3, 7), (4, 5)]);
        let mut constraints = DifferenceAntichain::default();
        constraints.insert((1 << 1) | (1 << 2));
        constraints.insert((1 << 1) | (1 << 2) | (1 << 3));
        assert_eq!(constraints.0.len(), 1, "superset constraint is redundant");
        constraints.prune_pairs(&mut candidates);
        let mut remaining = Vec::new();
        while let Some(pair) = candidates.pop() {
            remaining.push(pair);
        }
        remaining.sort_unstable();
        assert_eq!(remaining, vec![(1, 5), (2, 6)]);
    }

    #[test]
    fn unavoidable_witness_cache_reuses_only_compatible_witnesses_and_is_bounded() {
        let alternative = minimal_puzzle_seeds(17)[0].1.map(|row| {
            row.map(|digit| match digit {
                1 => 2,
                2 => 1,
                other => other,
            })
        });
        let mut cache = UnavoidableWitnessCache::default();
        cache.insert(alternative);

        let compatible_core = [[0; 9]; 9];
        assert_eq!(cache.compatible_with(&compatible_core).count(), 1);

        let mut incompatible_core = compatible_core;
        incompatible_core[0][7] = minimal_puzzle_seeds(17)[0].1[0][7];
        assert_eq!(cache.compatible_with(&incompatible_core).count(), 0);

        for marker in 0..=UNAVOIDABLE_WITNESS_CACHE_CAPACITY {
            let mut distinct = [[0; 9]; 9];
            distinct[0][0] = marker as u8;
            cache.insert(distinct);
        }
        assert_eq!(cache.alternatives.len(), UNAVOIDABLE_WITNESS_CACHE_CAPACITY);
    }

    #[test]
    fn bounded_exchange_and_eighteen_routes_respect_the_candidate_cap() {
        for strategy in [
            DiverseStrategy::SwapTwo,
            DiverseStrategy::PlusOneMinusOne,
            DiverseStrategy::SwapThree,
            DiverseStrategy::ViaEighteen,
            DiverseStrategy::DriftWalk,
        ] {
            let budget = DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: 100_000,
                max_hitting_set_nodes: 0,
                max_neighborhood_checks: 1,
                max_intermediate_clues: 18,
                strategy,
            };
            let error = generate_diverse_from_parent(
                7,
                minimal_puzzle_seeds(17)[0].0,
                minimal_puzzle_seeds(17)[0].1,
                budget,
            )
            .expect_err("one check cannot exhaust either strategy");
            assert!(matches!(
                error,
                super::DiverseGenerationError::BudgetExhausted {
                    neighborhood_checks: 1,
                    ..
                }
            ));
        }
    }

    #[test]
    fn eighteen_clue_route_finds_and_proves_a_new_unique_class() {
        let budget = DiverseBudget {
            max_attempts: 1,
            max_nodes_per_uniqueness_check: 100_000,
            max_hitting_set_nodes: 0,
            max_neighborhood_checks: 500,
            max_intermediate_clues: 18,
            strategy: DiverseStrategy::ViaEighteen,
        };
        let puzzle = generate_diverse_from_parent(
            20_261_027,
            minimal_puzzle_seeds(17)[0].0,
            minimal_puzzle_seeds(17)[0].1,
            budget,
        )
        .expect("the fixed 18-clue path has a verified child");
        assert_eq!(clue_count(&puzzle), MIN_CLUE_COUNT);
        assert_eq!(solution_count(&puzzle), 1);
        assert_ne!(
            crate::symmetry::canonical_key(&puzzle),
            crate::symmetry::canonical_key(&minimal_puzzle_seeds(17)[0].0)
        );
    }

    #[test]
    fn configurable_intermediate_route_finds_a_solution_changing_unique_child() {
        let puzzle = generate_diverse_from_parent(
            20_261_027,
            minimal_puzzle_seeds(17)[0].0,
            minimal_puzzle_seeds(17)[0].1,
            DiverseBudget {
                max_attempts: 1,
                max_nodes_per_uniqueness_check: 100_000,
                max_hitting_set_nodes: 0,
                max_neighborhood_checks: 500,
                max_intermediate_clues: 18,
                strategy: DiverseStrategy::ViaIntermediate,
            },
        )
        .expect("the configurable 18-clue stage should find a child");
        assert_eq!(clue_count(&puzzle), MIN_CLUE_COUNT);
        assert_eq!(solution_count(&puzzle), 1);
        let mut solution = puzzle;
        assert!(solve(&mut solution));
        assert_ne!(solution, minimal_puzzle_seeds(17)[0].1);
    }

    #[test]
    fn evolutionary_parent_must_be_a_verified_17_clue_problem() {
        let budget = DiverseBudget {
            max_attempts: 1,
            max_nodes_per_uniqueness_check: 100_000,
            max_hitting_set_nodes: 0,
            max_neighborhood_checks: 1,
            max_intermediate_clues: 18,
            strategy: DiverseStrategy::Adaptive,
        };
        assert!(matches!(
            generate_diverse_from_parent(1, [[0; 9]; 9], minimal_puzzle_seeds(17)[0].1, budget),
            Err(super::DiverseGenerationError::InvalidParent)
        ));
    }

    #[test]
    fn random_witness_strategy_is_rejected_by_parent_generation_api() {
        let budget = DiverseBudget {
            strategy: DiverseStrategy::Random17Witness,
            ..DiverseBudget::default()
        };
        assert_eq!(
            generate_diverse_from_parent(
                7,
                minimal_puzzle_seeds(17)[0].0,
                minimal_puzzle_seeds(17)[0].1,
                budget,
            ),
            Err(super::DiverseGenerationError::StrategyDoesNotUseParent)
        );
    }

    #[test]
    fn evolutionary_search_obeys_its_candidate_budget() {
        let budget = DiverseBudget {
            max_attempts: 1,
            max_nodes_per_uniqueness_check: 100_000,
            max_hitting_set_nodes: 0,
            max_neighborhood_checks: 1,
            max_intermediate_clues: 18,
            strategy: DiverseStrategy::SwapOne,
        };
        let error = generate_diverse_from_parent(
            1,
            minimal_puzzle_seeds(17)[0].0,
            minimal_puzzle_seeds(17)[0].1,
            budget,
        )
        .expect_err("one neighborhood check is not enough to establish a new class");
        assert!(matches!(
            error,
            super::DiverseGenerationError::BudgetExhausted {
                neighborhood_checks: 1,
                ..
            }
        ));
    }

    #[test]
    fn shared_generation_interface_dispatches_both_methods() {
        let fast = generate(GenerationConfig {
            seed: 91,
            clue_count: 30,
            method: GenerationMethod::Fast,
            diverse_budget: DiverseBudget::default(),
        })
        .unwrap();
        assert_eq!(clue_count(&fast), 30);

        let diverse = generate(GenerationConfig {
            seed: 91,
            clue_count: 81,
            method: GenerationMethod::Diverse,
            diverse_budget: DiverseBudget::default(),
        })
        .unwrap();
        assert_eq!(clue_count(&diverse), 81);
        assert!(solve(&mut diverse.clone()));
    }

    #[test]
    fn cached_base_solution_matches_the_solver() {
        let mut solved = minimal_puzzle_seeds(17)[0].0;
        assert!(solve(&mut solved));
        assert_eq!(solved, minimal_puzzle_seeds(17)[0].1);
    }

    #[test]
    fn transpose_transform_writes_cells_to_their_final_positions() {
        let transform = SudokuTransform {
            rows: std::array::from_fn(|index| index),
            columns: std::array::from_fn(|index| index),
            digits: std::array::from_fn(|digit| digit as u8),
            transpose: true,
        };

        let transformed = transform.apply(&minimal_puzzle_seeds(17)[0].1);
        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(transformed[x][y], minimal_puzzle_seeds(17)[0].1[y][x]);
            }
        }
    }

    #[test]
    fn randomized_group_permutations_preserve_completed_sudoku_boards() {
        for seed in 0..256 {
            let mut rng = SplitMix64(seed);
            let transform = SudokuTransform::random(&mut rng);
            let mut transformed = transform.apply(&minimal_puzzle_seeds(17)[0].1);
            let expected = transformed;

            assert!(solve(&mut transformed), "seed={seed}");
            assert_eq!(transformed, expected, "seed={seed}");

            for permutation in [&transform.rows, &transform.columns] {
                let mut source_groups = [0; 3];
                for (destination_group, triple) in permutation.chunks_exact(3).enumerate() {
                    let source_group = triple[0] / 3;
                    source_groups[destination_group] = source_group;
                    assert!(triple.iter().all(|index| index / 3 == source_group));

                    let mut offsets = [triple[0] % 3, triple[1] % 3, triple[2] % 3];
                    offsets.sort_unstable();
                    assert_eq!(offsets, [0, 1, 2]);
                }
                source_groups.sort_unstable();
                assert_eq!(source_groups, [0, 1, 2]);
            }
        }
    }
}
