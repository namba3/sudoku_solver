use crate::Matrix;

pub const MIN_CLUE_COUNT: usize = 17;
pub const MAX_CLUE_COUNT: usize = 81;
pub const DEFAULT_CLUE_COUNT: usize = 30;

const MINIMAL_PUZZLE: Matrix = [
    [0, 0, 0, 0, 0, 0, 0, 1, 0],
    [4, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 2, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 5, 0, 4, 0, 7],
    [0, 0, 8, 0, 0, 0, 3, 0, 0],
    [0, 0, 1, 0, 9, 0, 0, 0, 0],
    [3, 0, 0, 4, 0, 0, 2, 0, 0],
    [0, 5, 0, 1, 0, 0, 0, 0, 0],
    [0, 0, 0, 8, 0, 6, 0, 0, 0],
];

const MINIMAL_SOLUTION: Matrix = [
    [6, 9, 3, 7, 8, 4, 5, 1, 2],
    [4, 8, 7, 5, 1, 2, 9, 3, 6],
    [1, 2, 5, 9, 6, 3, 8, 7, 4],
    [9, 3, 2, 6, 5, 1, 4, 8, 7],
    [5, 6, 8, 2, 4, 7, 3, 9, 1],
    [7, 4, 1, 3, 9, 8, 6, 2, 5],
    [3, 1, 9, 4, 7, 5, 2, 6, 8],
    [8, 5, 6, 1, 2, 9, 7, 4, 3],
    [2, 7, 4, 8, 3, 6, 1, 5, 9],
];

/// Generate a randomized uniquely solvable puzzle with the default 30 clues.
pub fn generate_puzzle(seed: u64) -> Matrix {
    generate_puzzle_with_clues(seed, DEFAULT_CLUE_COUNT).expect("the default clue count is valid")
}

/// Generate a randomized uniquely solvable puzzle with exactly `clue_count` clues.
///
/// The supported range is 17 through 81. The randomized puzzle starts from a
/// transformed 17-clue unique puzzle, then adds digits from its solution. Adding
/// clues preserves uniqueness while guaranteeing the requested clue count.
pub fn generate_puzzle_with_clues(seed: u64, clue_count: usize) -> Result<Matrix, &'static str> {
    if !(MIN_CLUE_COUNT..=MAX_CLUE_COUNT).contains(&clue_count) {
        return Err("clue count must be between 17 and 81");
    }

    let mut rng = SplitMix64(seed);
    let transform = SudokuTransform::random(&mut rng);

    let mut puzzle = transform.apply(&MINIMAL_PUZZLE);
    let solution = transform.apply(&MINIMAL_SOLUTION);

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
        .take(clue_count - MIN_CLUE_COUNT)
    {
        let y = position / 9;
        let x = position % 9;
        puzzle[y][x] = solution[y][x];
    }

    Ok(puzzle)
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
        generate_puzzle, generate_puzzle_with_clues, SudokuTransform, DEFAULT_CLUE_COUNT,
        MAX_CLUE_COUNT, MINIMAL_PUZZLE, MINIMAL_SOLUTION, MIN_CLUE_COUNT,
    };
    use crate::{solve, SearchEvent, SolutionSearch};

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
    fn generated_puzzle_has_one_solution_and_removes_clues() {
        let puzzle = generate_puzzle(7);
        assert_eq!(clue_count(&puzzle), DEFAULT_CLUE_COUNT);
        assert_eq!(solution_count(&puzzle), 1);
    }

    #[test]
    fn requested_clue_counts_are_exact_and_uniquely_solvable() {
        for requested_count in [MIN_CLUE_COUNT, 30, MAX_CLUE_COUNT] {
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
    fn cached_base_solution_matches_the_solver() {
        let mut solved = MINIMAL_PUZZLE;
        assert!(solve(&mut solved));
        assert_eq!(solved, MINIMAL_SOLUTION);
    }

    #[test]
    fn transpose_transform_writes_cells_to_their_final_positions() {
        let transform = SudokuTransform {
            rows: std::array::from_fn(|index| index),
            columns: std::array::from_fn(|index| index),
            digits: std::array::from_fn(|digit| digit as u8),
            transpose: true,
        };

        let transformed = transform.apply(&MINIMAL_SOLUTION);
        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(transformed[x][y], MINIMAL_SOLUTION[y][x]);
            }
        }
    }
}
