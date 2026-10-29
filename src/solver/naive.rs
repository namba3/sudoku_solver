//! Straightforward row-major solver retained as a reference backend.

use super::backend::SolverBackend;
use super::{AlternativeSolutionCheck, SearchError, SearchEvent, UniquenessCheck};
use crate::Matrix;

const PROGRESS_INTERVAL: u64 = 2048;

#[derive(Clone, Copy)]
struct Frame {
    cell: (usize, usize),
    next_digit: u8,
    preferred_digit: Option<u8>,
    try_preferred: bool,
}

/// Lazy row-major depth-first search using direct row/column/box scans.
pub struct SolutionSearch {
    board: Matrix,
    empty_cells: Vec<(usize, usize)>,
    frames: Vec<Frame>,
    depth: usize,
    preferred_solution: Option<Matrix>,
    pending_node: bool,
    completed: bool,
    explored_nodes: u64,
    solutions_found: usize,
}

impl SolutionSearch {
    pub fn new(input: &Matrix) -> Result<Self, SearchError> {
        Self::with_preferred_solution(input, None)
    }

    pub fn new_preferring_solution(
        input: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self, SearchError> {
        Self::with_preferred_solution(input, Some(preferred_solution))
    }

    fn with_preferred_solution(
        input: &Matrix,
        preferred_solution: Option<Matrix>,
    ) -> Result<Self, SearchError> {
        let mut board = *input;
        let mut empty_cells = Vec::new();
        for y in 0..9 {
            for x in 0..9 {
                if !(1..=9).contains(&board[y][x]) {
                    board[y][x] = 0;
                    empty_cells.push((x, y));
                }
            }
        }
        if has_conflicts(&board) {
            return Err(SearchError::ConflictingValues);
        }
        Ok(Self {
            board,
            frames: Vec::with_capacity(empty_cells.len()),
            empty_cells,
            depth: 0,
            preferred_solution,
            pending_node: true,
            completed: false,
            explored_nodes: 0,
            solutions_found: 0,
        })
    }

    pub fn explored_nodes(&self) -> u64 {
        self.explored_nodes
    }

    pub fn solutions_found(&self) -> usize {
        self.solutions_found
    }

    pub fn is_exhausted(&self) -> bool {
        self.completed
    }
}

impl Iterator for SolutionSearch {
    type Item = SearchEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.completed {
                return None;
            }
            if self.pending_node {
                self.pending_node = false;
                self.explored_nodes = self.explored_nodes.saturating_add(1);
                if self.explored_nodes % PROGRESS_INTERVAL == 0 {
                    return Some(SearchEvent::Progress {
                        nodes: self.explored_nodes,
                        solutions_found: self.solutions_found,
                    });
                }
            }

            if self.depth == self.empty_cells.len() {
                self.solutions_found += 1;
                let solution = self.board;
                if self.depth == 0 {
                    self.completed = true;
                } else {
                    self.depth -= 1;
                    let (x, y) = self.empty_cells[self.depth];
                    self.board[y][x] = 0;
                }
                return Some(SearchEvent::SolutionFound {
                    index: self.solutions_found,
                    board: solution,
                });
            }

            if self.frames.len() == self.depth {
                self.frames.push(Frame {
                    cell: self.empty_cells[self.depth],
                    next_digit: 1,
                    preferred_digit: self.preferred_solution.map(|solution| {
                        solution[self.empty_cells[self.depth].1][self.empty_cells[self.depth].0]
                    }),
                    try_preferred: self.preferred_solution.is_some(),
                });
            }

            let frame = &mut self.frames[self.depth];
            let cell = frame.cell;
            let preferred = if frame.try_preferred {
                frame.try_preferred = false;
                frame.preferred_digit.filter(|&digit| {
                    (1..=9).contains(&digit) && is_legal(&self.board, cell.0, cell.1, digit)
                })
            } else {
                None
            };
            let candidate = preferred.or_else(|| {
                (frame.next_digit..=9).find(|&digit| {
                    Some(digit) != frame.preferred_digit
                        && is_legal(&self.board, cell.0, cell.1, digit)
                })
            });
            if let Some(digit) = candidate {
                if preferred.is_none() {
                    self.frames[self.depth].next_digit = digit + 1;
                }
                self.board[cell.1][cell.0] = digit;
                self.depth += 1;
                self.pending_node = true;
                continue;
            }

            self.frames.pop();
            if self.depth == 0 {
                self.completed = true;
                return None;
            }
            self.depth -= 1;
            let (x, y) = self.empty_cells[self.depth];
            self.board[y][x] = 0;
        }
    }
}

impl std::iter::FusedIterator for SolutionSearch {}

/// Reference backend that checks constraints by scanning the board.
pub struct Backend;

impl SolverBackend for Backend {
    type Search = SolutionSearch;

    fn search(board: &Matrix) -> Result<Self::Search, SearchError> {
        SolutionSearch::new(board)
    }

    fn search_with_preferred_solution(
        board: &Matrix,
        preferred_solution: Matrix,
    ) -> Result<Self::Search, SearchError> {
        SolutionSearch::new_preferring_solution(board, preferred_solution)
    }

    fn solve(board: &mut Matrix) -> bool {
        for cell in board.iter_mut().flatten() {
            if !(1..=9).contains(cell) {
                *cell = 0;
            }
        }
        let Ok(mut search) = SolutionSearch::new(board) else {
            return false;
        };
        while let Some(event) = search.next() {
            if let SearchEvent::SolutionFound {
                board: solution, ..
            } = event
            {
                *board = solution;
                return true;
            }
        }
        false
    }

    fn candidates_for(board: &Matrix, x: usize, y: usize) -> Vec<u8> {
        if x >= 9 || y >= 9 || (1..=9).contains(&board[y][x]) {
            return Vec::new();
        }
        (1..=9)
            .filter(|&digit| is_legal(board, x, y, digit))
            .collect()
    }

    fn explored_nodes(search: &Self::Search) -> u64 {
        search.explored_nodes()
    }

    fn solutions_found(search: &Self::Search) -> usize {
        search.solutions_found()
    }
}

pub fn solve(board: &mut Matrix) -> bool {
    Backend::solve(board)
}

pub fn candidates_for(board: &Matrix, x: usize, y: usize) -> Vec<u8> {
    Backend::candidates_for(board, x, y)
}

pub fn check_uniqueness(board: &Matrix, max_nodes: u64) -> Result<UniquenessCheck, SearchError> {
    super::backend::check_uniqueness_with::<Backend>(board, max_nodes)
}

pub fn find_alternative_solution(
    puzzle: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
) -> AlternativeSolutionCheck {
    super::backend::find_alternative_solution_with::<Backend>(puzzle, known_solution, max_nodes)
}

pub fn find_alternative_solution_with_order(
    puzzle: &Matrix,
    known_solution: &Matrix,
    max_nodes: u64,
    order: super::AlternativeSearchOrder,
) -> AlternativeSolutionCheck {
    super::backend::find_alternative_solution_with_order::<Backend>(
        puzzle,
        known_solution,
        max_nodes,
        order,
    )
}

fn has_conflicts(board: &Matrix) -> bool {
    for index in 0..9 {
        for offset in 0..9 {
            let value = board[index][offset];
            if value != 0 && (0..offset).any(|other| board[index][other] == value) {
                return true;
            }
            let value = board[offset][index];
            if value != 0 && (0..offset).any(|other| board[other][index] == value) {
                return true;
            }
            let box_y = (index / 3) * 3 + offset / 3;
            let box_x = (index % 3) * 3 + offset % 3;
            let value = board[box_y][box_x];
            if value != 0
                && (0..offset).any(|other| {
                    board[(index / 3) * 3 + other / 3][(index % 3) * 3 + other % 3] == value
                })
            {
                return true;
            }
        }
    }
    false
}

fn is_legal(board: &Matrix, x: usize, y: usize, digit: u8) -> bool {
    if (0..9).any(|index| board[y][index] == digit || board[index][x] == digit) {
        return false;
    }
    let box_x = x / 3 * 3;
    let box_y = y / 3 * 3;
    !(0..3).any(|dy| (0..3).any(|dx| board[box_y + dy][box_x + dx] == digit))
}

#[cfg(test)]
mod tests {
    use super::{check_uniqueness, find_alternative_solution, solve, Backend, SolutionSearch};
    use crate::solver::{
        backend::SolverBackend, optimized, AlternativeSolutionStatus, SearchEvent, UniquenessStatus,
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

    fn solutions<B: SolverBackend>(board: &Matrix, limit: usize) -> Vec<Matrix> {
        let mut search = B::search(board).unwrap();
        let mut found = Vec::new();
        for event in &mut search {
            if let SearchEvent::SolutionFound { board, .. } = event {
                found.push(board);
                if found.len() == limit {
                    break;
                }
            }
        }
        found.sort_unstable();
        found
    }

    #[test]
    fn reference_and_optimized_backends_find_the_same_unique_solution() {
        let naive = solutions::<Backend>(&PUZZLE, 2);
        let optimized = solutions::<crate::solver::backend::Optimized>(&PUZZLE, 2);
        assert_eq!(naive, vec![SOLUTION]);
        assert_eq!(optimized, naive);
    }

    #[test]
    fn reference_and_optimized_backends_agree_on_multiple_solutions() {
        let mut puzzle = SOLUTION;
        for (y, x) in [(3, 5), (3, 8), (4, 5), (4, 8)] {
            puzzle[y][x] = 0;
        }
        let naive = solutions::<Backend>(&puzzle, 2);
        let optimized = solutions::<crate::solver::backend::Optimized>(&puzzle, 2);
        assert_eq!(naive.len(), 2);
        assert_eq!(naive, optimized);
        assert_eq!(
            check_uniqueness(&puzzle, 100).unwrap().status,
            UniquenessStatus::Multiple
        );
    }

    #[test]
    fn bounded_alternative_queries_agree() {
        let mut reduced = PUZZLE;
        reduced[0][0] = 0;
        let naive = find_alternative_solution(&reduced, &SOLUTION, 100_000);
        let fast = optimized::find_alternative_solution(&reduced, &SOLUTION, 100_000);
        assert_eq!(naive.status, AlternativeSolutionStatus::Unique);
        assert_eq!(fast.status, naive.status);
    }

    #[test]
    fn candidate_and_preferred_order_apis_match_the_optimized_contract() {
        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(
                    super::candidates_for(&PUZZLE, x, y),
                    optimized::candidates_for(&PUZZLE, x, y)
                );
            }
        }
        let mut reduced = PUZZLE;
        reduced[0][0] = 0;
        let naive = super::find_alternative_solution_with_order(
            &reduced,
            &SOLUTION,
            100_000,
            super::super::AlternativeSearchOrder::PreferKnownValues,
        );
        let fast = optimized::find_alternative_solution_with_order(
            &reduced,
            &SOLUTION,
            100_000,
            super::super::AlternativeSearchOrder::PreferKnownValues,
        );
        assert_eq!(naive.status, AlternativeSolutionStatus::Unique);
        assert_eq!(naive.status, fast.status);
    }

    #[test]
    fn solve_mutates_the_board_to_a_solution() {
        let mut puzzle = PUZZLE;
        assert!(solve(&mut puzzle));
        assert_eq!(puzzle, SOLUTION);
        let search = SolutionSearch::new(&puzzle).unwrap();
        assert_eq!(search.solutions_found(), 0);
    }
}
