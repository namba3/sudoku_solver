use crate::Matrix;

/// Solve the Sudoku
pub fn solve(mtx: &mut Matrix) -> bool {
    let mut manager = StateManager::new();
    let mut empty_cells = Vec::new();

    for y in 0..9 {
        for x in 0..9 {
            let val = &mut mtx[y][x];
            if !(1..=9).contains(val) {
                empty_cells.push((x, y));
                *val = 0;
                continue;
            }
            if !manager.set(x, y, *val) {
                return false;
            }
        }
    }

    fill(mtx, &mut empty_cells, &mut manager)
}

const PROGRESS_INTERVAL: u64 = 2048;

/// Values yielded by an unrestricted solution search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchEvent {
    SolutionFound { index: usize, board: Matrix },
    Progress { nodes: u64, solutions_found: usize },
}

struct SearchFrame {
    cell: (usize, usize),
    candidates: Vec<u8>,
    next_candidate: usize,
    assigned: Option<u8>,
}

/// A lazy, unrestricted depth-first enumeration of a Sudoku's solutions.
///
/// The iterator yields progress events every 2,048 visited nodes and a
/// `SolutionFound` event for each solution. Consumers can stop early by
/// dropping the iterator. The supplied board is never modified.
pub struct SolutionSearch {
    board: Matrix,
    manager: StateManager,
    empty_cells: Vec<(usize, usize)>,
    frames: Vec<SearchFrame>,
    node_pending: bool,
    node_counted: bool,
    advance_branch: bool,
    completed: bool,
    explored_nodes: u64,
    solutions_found: usize,
}

/// Errors detected before solution enumeration begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchError {
    ConflictingValues,
}

impl SolutionSearch {
    /// Create an unrestricted search. Duplicate given values are rejected.
    pub fn new(mtx: &Matrix) -> Result<Self, SearchError> {
        let mut board = *mtx;
        let mut manager = StateManager::new();
        let mut empty_cells = Vec::new();
        for y in 0..9 {
            for x in 0..9 {
                let value = &mut board[y][x];
                if !(1..=9).contains(value) {
                    empty_cells.push((x, y));
                    *value = 0;
                } else if !manager.set(x, y, *value) {
                    return Err(SearchError::ConflictingValues);
                }
            }
        }
        Ok(Self {
            board,
            manager,
            empty_cells,
            frames: Vec::new(),
            node_pending: true,
            node_counted: false,
            advance_branch: false,
            completed: false,
            explored_nodes: 0,
            solutions_found: 0,
        })
    }

    /// Number of search-tree nodes visited so far.
    pub fn explored_nodes(&self) -> u64 {
        self.explored_nodes
    }

    /// Number of solutions yielded so far.
    pub fn solutions_found(&self) -> usize {
        self.solutions_found
    }

    /// Whether the iterator reached the end of the complete search tree.
    pub fn is_exhausted(&self) -> bool {
        self.completed
    }

    fn advance_to_next_branch(&mut self) -> bool {
        loop {
            let Some(frame_index) = self.frames.len().checked_sub(1) else {
                return false;
            };

            let (cell, previous, candidate) = {
                let frame = &mut self.frames[frame_index];
                let previous = frame.assigned.take();
                let candidate = if frame.next_candidate < frame.candidates.len() {
                    let value = frame.candidates[frame.next_candidate];
                    frame.next_candidate += 1;
                    Some(value)
                } else {
                    None
                };
                (frame.cell, previous, candidate)
            };

            let (x, y) = cell;
            if let Some(value) = previous {
                self.manager.remove(x, y, value);
                self.board[y][x] = 0;
            }

            if let Some(value) = candidate {
                self.manager.set(x, y, value);
                self.board[y][x] = value;
                self.frames[frame_index].assigned = Some(value);
                return true;
            }

            self.frames.pop();
            self.empty_cells.push(cell);
        }
    }
}

impl Iterator for SolutionSearch {
    type Item = SearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.completed {
                return None;
            }

            if self.node_pending {
                if !self.node_counted {
                    self.explored_nodes += 1;
                    self.node_counted = true;
                    if self.explored_nodes % PROGRESS_INTERVAL == 0 {
                        return Some(SearchEvent::Progress {
                            nodes: self.explored_nodes,
                            solutions_found: self.solutions_found,
                        });
                    }
                }

                self.node_counted = false;
                self.node_pending = false;
                if self.empty_cells.is_empty() {
                    self.solutions_found += 1;
                    self.advance_branch = true;
                    return Some(SearchEvent::SolutionFound {
                        index: self.solutions_found,
                        board: self.board,
                    });
                }

                let min_index = self
                    .empty_cells
                    .iter()
                    .copied()
                    .enumerate()
                    .min_by_key(|&(_, (x, y))| self.manager.num_candidates(x, y))
                    .map(|(index, _)| index)
                    .expect("non-empty cell list has a minimum");
                let cell = self.empty_cells.swap_remove(min_index);
                let candidates = self.manager.candidates(cell.0, cell.1).collect();
                self.frames.push(SearchFrame {
                    cell,
                    candidates,
                    next_candidate: 0,
                    assigned: None,
                });
                self.advance_branch = true;
            }

            if self.advance_branch {
                if self.advance_to_next_branch() {
                    self.advance_branch = false;
                    self.node_pending = true;
                    continue;
                }
            }

            self.completed = true;
            return None;
        }
    }
}

impl std::iter::FusedIterator for SolutionSearch {}

/// Return the allowed digits for an empty cell at column `x`, row `y`.
///
/// Coordinates outside the 9×9 board and already-filled cells have no candidates.
pub fn candidates_for(mtx: &Matrix, x: usize, y: usize) -> Vec<u8> {
    if x >= 9 || y >= 9 || (1..=9).contains(&mtx[y][x]) {
        return Vec::new();
    }

    let mut used = 0;
    for i in 0..9 {
        let box_y = (y / 3) * 3 + i / 3;
        let box_x = (x / 3) * 3 + i % 3;
        for value in [mtx[y][i], mtx[i][x], mtx[box_y][box_x]] {
            if (1..=9).contains(&value) {
                used |= StateManager::flag(value);
            }
        }
    }

    Candidates::new(used).collect()
}

/// Explain a deterministic next move without guessing or modifying the board.
///
/// Naked singles are checked first, followed by hidden singles in rows, columns,
/// and 3×3 boxes. `Ok(None)` means the board is valid but these techniques do
/// not currently reveal a move.
pub fn find_hint(mtx: &Matrix) -> Result<Option<Hint>, HintError> {
    if has_duplicate_values(mtx) {
        return Err(HintError::ConflictingValues);
    }

    let mut candidates = std::array::from_fn(|_| std::array::from_fn(|_| Vec::new()));
    for y in 0..9 {
        for x in 0..9 {
            if !(1..=9).contains(&mtx[y][x]) {
                candidates[y][x] = candidates_for(mtx, x, y);
                if candidates[y][x].is_empty() {
                    return Err(HintError::NoCandidates);
                }
            }
        }
    }

    for y in 0..9 {
        for x in 0..9 {
            if candidates[y][x].len() == 1 {
                return Ok(Some(Hint {
                    x,
                    y,
                    digit: candidates[y][x][0],
                    technique: HintTechnique::NakedSingle,
                }));
            }
        }
    }

    for y in 0..9 {
        if let Some(hint) = find_hidden_single(mtx, &candidates, (0..9).map(|x| (x, y))) {
            return Ok(Some(Hint {
                technique: HintTechnique::HiddenSingleRow,
                ..hint
            }));
        }
    }

    for x in 0..9 {
        if let Some(hint) = find_hidden_single(mtx, &candidates, (0..9).map(|y| (x, y))) {
            return Ok(Some(Hint {
                technique: HintTechnique::HiddenSingleColumn,
                ..hint
            }));
        }
    }

    for box_y in (0..9).step_by(3) {
        for box_x in (0..9).step_by(3) {
            let cells = (0..3).flat_map(|dy| (0..3).map(move |dx| (box_x + dx, box_y + dy)));
            if let Some(hint) = find_hidden_single(mtx, &candidates, cells) {
                return Ok(Some(Hint {
                    technique: HintTechnique::HiddenSingleBox,
                    ..hint
                }));
            }
        }
    }

    Ok(None)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hint {
    /// Column, from 0 to 8.
    pub x: usize,
    /// Row, from 0 to 8.
    pub y: usize,
    pub digit: u8,
    pub technique: HintTechnique,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HintTechnique {
    NakedSingle,
    HiddenSingleRow,
    HiddenSingleColumn,
    HiddenSingleBox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HintError {
    ConflictingValues,
    NoCandidates,
}

fn find_hidden_single(
    mtx: &Matrix,
    candidates: &[[Vec<u8>; 9]; 9],
    cells: impl Iterator<Item = (usize, usize)>,
) -> Option<Hint> {
    let cells: Vec<_> = cells.collect();
    for digit in 1..=9 {
        let mut possible_cell = None;
        for &(x, y) in &cells {
            if !(1..=9).contains(&mtx[y][x]) && candidates[y][x].contains(&digit) {
                if possible_cell.is_some() {
                    possible_cell = None;
                    break;
                }
                possible_cell = Some((x, y));
            }
        }
        if let Some((x, y)) = possible_cell {
            return Some(Hint {
                x,
                y,
                digit,
                technique: HintTechnique::HiddenSingleRow,
            });
        }
    }
    None
}

fn has_duplicate_values(mtx: &Matrix) -> bool {
    for index in 0..9 {
        let mut row = 0u16;
        let mut column = 0u16;
        let mut box_values = 0u16;
        for offset in 0..9 {
            let box_y = (index / 3) * 3 + offset / 3;
            let box_x = (index % 3) * 3 + offset % 3;
            for (mask, value) in [
                (&mut row, mtx[index][offset]),
                (&mut column, mtx[offset][index]),
                (&mut box_values, mtx[box_y][box_x]),
            ] {
                if (1..=9).contains(&value) {
                    let flag = 1u16 << (value - 1);
                    if *mask & flag != 0 {
                        return true;
                    }
                    *mask |= flag;
                }
            }
        }
    }
    false
}

/// Fill the Sudoku matrix with temporary placement method
fn fill(
    mtx: &mut Matrix,
    empty_cells: &mut Vec<(usize, usize)>,
    manager: &mut StateManager,
) -> bool {
    if empty_cells.is_empty() {
        return true;
    }

    // Select the cell with fewest number of candidates
    let (x, y) = {
        let min_idx = empty_cells
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|&(_idx, (x, y))| manager.num_candidates(x, y))
            .map(|(idx, _)| idx)
            .unwrap();
        empty_cells.swap_remove(min_idx)
    };

    // Backtrack if the cell has no candidates
    if manager.num_candidates(x, y) <= 0 {
        empty_cells.push((x, y));
        return false;
    }

    // Temporarily place candidates and self-recurse
    for val in manager.candidates(x, y) {
        manager.set(x, y, val);
        mtx[y][x] = val;
        if fill(mtx, empty_cells, manager) {
            return true;
        }
        manager.remove(x, y, val);
    }

    // Backtrack if the temporary placement method fails
    empty_cells.push((x, y));
    mtx[y][x] = 0;
    false
}

/// A state manager that manages whether each cell in the matrix can have candidates
///
/// This uses bitwise operations to find candidates faster than using naive `for` statements.
struct StateManager {
    row: [u16; 9],
    col: [u16; 9],
    sub_mtx: [[u16; 3]; 3],
}
impl StateManager {
    pub const fn new() -> Self {
        Self {
            row: [0; 9],
            col: [0; 9],
            sub_mtx: [[0; 3]; 3],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: u8) -> bool {
        if !self.is_settable(x, y, val) {
            return false;
        }

        let flag = Self::flag(val);
        self.row[y] |= flag;
        self.col[x] |= flag;
        self.sub_mtx[y / 3][x / 3] |= flag;

        true
    }

    pub fn remove(&mut self, x: usize, y: usize, val: u8) {
        let mask = !Self::flag(val);
        self.row[y] &= mask;
        self.col[x] &= mask;
        self.sub_mtx[y / 3][x / 3] &= mask;
    }

    pub const fn candidates(&self, x: usize, y: usize) -> Candidates {
        let bits = self.bits(x, y);
        Candidates::new(bits)
    }

    pub const fn num_candidates(&self, x: usize, y: usize) -> u8 {
        let bits = self.bits(x, y);
        9 - bits.count_ones() as u8
    }

    const fn flag(val: u8) -> u16 {
        1u16 << (val - 1)
    }

    const fn bits(&self, x: usize, y: usize) -> u16 {
        self.row[y] | self.col[x] | self.sub_mtx[y / 3][x / 3]
    }

    const fn is_settable(&self, x: usize, y: usize, val: u8) -> bool {
        let flag = Self::flag(val);
        let bits = self.bits(x, y);
        (bits & flag) == 0
    }
}

pub struct Candidates {
    bits: u16,
    i: u8,
}
impl Candidates {
    pub const fn new(bits: u16) -> Self {
        Self { bits: !bits, i: 0 }
    }
}
impl Iterator for Candidates {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        while self.bits != 0 && self.i < 9 {
            let current = self.bits;
            self.bits >>= 1;
            self.i += 1;

            if current & 1 == 1 {
                return Some(self.i);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        find_hint, solve, Candidates, HintError, HintTechnique, SearchEvent, SolutionSearch,
        StateManager,
    };
    use crate::Matrix;

    const PUZZLE: Matrix = [
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

    #[test]
    fn solves_a_standard_puzzle() {
        let mut puzzle = PUZZLE;

        assert!(solve(&mut puzzle));
        assert_eq!(puzzle, SOLUTION);
    }

    #[test]
    fn rejects_duplicate_givens_in_a_row() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[0][1] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn rejects_duplicate_givens_in_a_column() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[1][0] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn rejects_duplicate_givens_in_a_subgrid() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 4;
        puzzle[1][1] = 4;

        assert!(!solve(&mut puzzle));
    }

    #[test]
    fn candidates_yield_available_digits_in_order() {
        let blocked = (1 << (2 - 1)) | (1 << (5 - 1)) | (1 << (8 - 1));
        let candidates: Vec<_> = Candidates::new(blocked).collect();

        assert_eq!(candidates, vec![1, 3, 4, 6, 7, 9]);
    }

    #[test]
    fn state_manager_restores_candidates_after_removal() {
        let mut manager = StateManager::new();
        assert!(manager.set(0, 0, 1));
        assert!(!manager.is_settable(0, 1, 1));

        manager.remove(0, 0, 1);

        assert!(manager.is_settable(0, 1, 1));
    }

    #[test]
    fn candidates_for_excludes_digits_in_the_row_column_and_subgrid() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 1;
        puzzle[3][2] = 2;
        puzzle[1][1] = 3;

        assert_eq!(super::candidates_for(&puzzle, 2, 2), vec![4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn candidates_for_returns_empty_for_filled_or_out_of_bounds_cells() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[4][5] = 7;

        assert!(super::candidates_for(&puzzle, 5, 4).is_empty());
        assert!(super::candidates_for(&puzzle, 9, 4).is_empty());
        assert!(super::candidates_for(&puzzle, 5, 9).is_empty());
    }

    #[test]
    fn find_hint_reports_a_naked_single_without_changing_the_board() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let original = puzzle;

        let hint = find_hint(&puzzle).unwrap().unwrap();

        assert_eq!(hint.x, 8);
        assert_eq!(hint.y, 0);
        assert_eq!(hint.digit, 9);
        assert_eq!(hint.technique, HintTechnique::NakedSingle);
        assert_eq!(puzzle, original);
    }

    #[test]
    fn find_hint_returns_none_when_no_supported_single_is_available() {
        assert_eq!(find_hint(&[[0; 9]; 9]), Ok(None));
    }

    #[test]
    fn find_hint_reports_conflicting_values() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][0] = 5;
        puzzle[0][8] = 5;

        assert_eq!(find_hint(&puzzle), Err(HintError::ConflictingValues));
    }

    #[test]
    fn find_hint_reports_an_empty_cell_with_no_candidates() {
        let mut puzzle = [[0; 9]; 9];
        puzzle[0][1..9].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        for (y, value) in [4, 6, 1, 2, 3, 5, 7, 8].into_iter().enumerate() {
            puzzle[y + 1][0] = value;
        }
        puzzle[1][1] = 9;

        assert_eq!(find_hint(&puzzle), Err(HintError::NoCandidates));
    }

    #[test]
    fn find_hint_finds_a_hidden_single_in_a_row() {
        let puzzle: Matrix = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [6, 0, 0, 1, 0, 0, 0, 0, 0],
            [0, 0, 8, 0, 4, 0, 0, 0, 0],
            [8, 0, 9, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [7, 1, 0, 9, 0, 0, 0, 5, 0],
            [0, 0, 0, 0, 3, 7, 0, 8, 4],
            [0, 0, 0, 0, 0, 0, 0, 3, 5],
            [0, 0, 0, 2, 8, 0, 0, 0, 0],
        ];

        let hint = find_hint(&puzzle).unwrap().unwrap();

        assert_eq!((hint.x, hint.y, hint.digit), (1, 7, 8));
        assert_eq!(hint.technique, HintTechnique::HiddenSingleRow);
    }

    #[test]
    fn find_hint_finds_a_hidden_single_in_a_column() {
        let puzzle: Matrix = [
            [0, 0, 0, 6, 0, 0, 0, 0, 2],
            [0, 0, 2, 1, 9, 0, 0, 0, 8],
            [0, 0, 0, 0, 0, 0, 5, 0, 0],
            [0, 0, 0, 0, 0, 1, 0, 0, 0],
            [4, 2, 6, 0, 0, 0, 0, 0, 0],
            [0, 1, 0, 0, 2, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 1, 9, 6, 0, 5],
            [0, 0, 0, 0, 0, 6, 0, 0, 0],
        ];

        let hint = find_hint(&puzzle).unwrap().unwrap();

        assert_eq!((hint.x, hint.y, hint.digit), (4, 3, 6));
        assert_eq!(hint.technique, HintTechnique::HiddenSingleColumn);
    }

    #[test]
    fn find_hint_finds_a_hidden_single_in_a_box() {
        let puzzle: Matrix = [
            [0, 0, 0, 0, 0, 8, 0, 0, 0],
            [0, 0, 0, 1, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 6, 0],
            [8, 0, 0, 0, 6, 1, 4, 0, 0],
            [0, 2, 0, 0, 5, 3, 0, 0, 0],
            [7, 0, 0, 0, 0, 4, 0, 0, 0],
            [0, 0, 0, 5, 0, 0, 2, 8, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 5, 2, 8, 0, 0, 0, 0],
        ];

        let hint = find_hint(&puzzle).unwrap().unwrap();

        assert_eq!((hint.x, hint.y, hint.digit), (4, 5, 2));
        assert_eq!(hint.technique, HintTechnique::HiddenSingleBox);
    }

    #[test]
    fn enumerates_all_solutions_for_a_unique_puzzle_without_changing_the_board() {
        let original = PUZZLE;
        let mut search = SolutionSearch::new(&PUZZLE).unwrap();
        let found: Vec<_> = search
            .by_ref()
            .filter_map(|event| match event {
                SearchEvent::SolutionFound { board, .. } => Some(board),
                SearchEvent::Progress { .. } => None,
            })
            .collect();

        assert_eq!(search.solutions_found(), 1);
        assert!(search.explored_nodes() > 0);
        assert!(search.is_exhausted());
        assert_eq!(found, vec![SOLUTION]);
        assert_eq!(PUZZLE, original);
    }

    #[test]
    fn a_consumer_can_stop_after_the_desired_number_of_solutions() {
        let mut search = SolutionSearch::new(&[[0; 9]; 9]).unwrap();
        let found: Vec<_> = search
            .by_ref()
            .filter_map(|event| match event {
                SearchEvent::SolutionFound { board, .. } => Some(board),
                SearchEvent::Progress { .. } => None,
            })
            .take(2)
            .collect();

        assert_eq!(found.len(), 2);
        assert_eq!(search.solutions_found(), 2);
        assert!(!search.is_exhausted());
    }

    #[test]
    fn reports_periodic_progress_while_searching() {
        let mut search = SolutionSearch::new(&[[0; 9]; 9]).unwrap();
        let progress = search.find_map(|event| match event {
            SearchEvent::Progress {
                nodes,
                solutions_found,
            } => Some((nodes, solutions_found)),
            SearchEvent::SolutionFound { .. } => None,
        });

        assert_eq!(progress, Some((2048, search.solutions_found())));
        assert_eq!(search.explored_nodes(), 2048);
        assert!(!search.is_exhausted());
    }

    #[test]
    fn rejects_conflicting_boards() {
        let mut conflicting = [[0; 9]; 9];
        conflicting[0][0] = 3;
        conflicting[0][1] = 3;

        assert_eq!(
            SolutionSearch::new(&conflicting).map(|_| ()),
            Err(super::SearchError::ConflictingValues)
        );
    }
}
