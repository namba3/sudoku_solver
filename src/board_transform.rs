#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoardTransform {
    SwapBands {
        first: usize,
        second: usize,
    },
    SwapStacks {
        first: usize,
        second: usize,
    },
    SwapRows {
        band: usize,
        first: usize,
        second: usize,
    },
    SwapColumns {
        stack: usize,
        first: usize,
        second: usize,
    },
    SwapDigits {
        first: u8,
        second: u8,
    },
    Transpose,
    Rotate90,
    Rotate180,
    Rotate270,
    ReflectHorizontal,
    ReflectVertical,
}

pub(super) fn row_drag_transform(source: usize, target: usize) -> Option<BoardTransform> {
    if source >= 9 || target >= 9 || source == target {
        return None;
    }

    let source_band = source / 3;
    let target_band = target / 3;
    if source_band == target_band {
        Some(BoardTransform::SwapRows {
            band: source_band,
            first: source % 3,
            second: target % 3,
        })
    } else {
        None
    }
}

pub(super) fn column_drag_transform(source: usize, target: usize) -> Option<BoardTransform> {
    if source >= 9 || target >= 9 || source == target {
        return None;
    }

    let source_stack = source / 3;
    let target_stack = target / 3;
    if source_stack == target_stack {
        Some(BoardTransform::SwapColumns {
            stack: source_stack,
            first: source % 3,
            second: target % 3,
        })
    } else {
        None
    }
}

pub(super) fn band_drag_transform(source: usize, target: usize) -> Option<BoardTransform> {
    (source < 3 && target < 3 && source != target).then_some(BoardTransform::SwapBands {
        first: source,
        second: target,
    })
}

pub(super) fn stack_drag_transform(source: usize, target: usize) -> Option<BoardTransform> {
    (source < 3 && target < 3 && source != target).then_some(BoardTransform::SwapStacks {
        first: source,
        second: target,
    })
}

pub(super) fn transform_matrix(
    board: &sudoku_solver::Matrix,
    transform: BoardTransform,
) -> sudoku_solver::Matrix {
    match transform {
        BoardTransform::SwapBands { first, second } => {
            let mut transformed = *board;
            if first < 3 && second < 3 {
                for offset in 0..3 {
                    transformed.swap(first * 3 + offset, second * 3 + offset);
                }
            }
            transformed
        }
        BoardTransform::SwapStacks { first, second } => {
            let mut transformed = *board;
            if first < 3 && second < 3 {
                for row in &mut transformed {
                    for offset in 0..3 {
                        row.swap(first * 3 + offset, second * 3 + offset);
                    }
                }
            }
            transformed
        }
        BoardTransform::SwapRows {
            band,
            first,
            second,
        } => {
            let mut transformed = *board;
            if band < 3 && first < 3 && second < 3 {
                transformed.swap(band * 3 + first, band * 3 + second);
            }
            transformed
        }
        BoardTransform::SwapColumns {
            stack,
            first,
            second,
        } => {
            let mut transformed = *board;
            if stack < 3 && first < 3 && second < 3 {
                for row in &mut transformed {
                    row.swap(stack * 3 + first, stack * 3 + second);
                }
            }
            transformed
        }
        BoardTransform::SwapDigits { first, second } => {
            let mut transformed = *board;
            if (1..=9).contains(&first) && (1..=9).contains(&second) {
                for value in transformed.iter_mut().flatten() {
                    if *value == first {
                        *value = second;
                    } else if *value == second {
                        *value = first;
                    }
                }
            }
            transformed
        }
        BoardTransform::Transpose => std::array::from_fn(|y| std::array::from_fn(|x| board[x][y])),
        BoardTransform::Rotate90 => {
            std::array::from_fn(|y| std::array::from_fn(|x| board[8 - x][y]))
        }
        BoardTransform::Rotate180 => {
            std::array::from_fn(|y| std::array::from_fn(|x| board[8 - y][8 - x]))
        }
        BoardTransform::Rotate270 => {
            std::array::from_fn(|y| std::array::from_fn(|x| board[x][8 - y]))
        }
        BoardTransform::ReflectHorizontal => {
            std::array::from_fn(|y| std::array::from_fn(|x| board[8 - y][x]))
        }
        BoardTransform::ReflectVertical => {
            std::array::from_fn(|y| std::array::from_fn(|x| board[y][8 - x]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        band_drag_transform, column_drag_transform, row_drag_transform, stack_drag_transform,
        transform_matrix, BoardTransform,
    };
    use sudoku_solver::Matrix;

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
    fn every_symmetry_transform_preserves_a_completed_sudoku() {
        let transforms = [
            BoardTransform::SwapBands {
                first: 0,
                second: 2,
            },
            BoardTransform::SwapStacks {
                first: 0,
                second: 2,
            },
            BoardTransform::SwapRows {
                band: 1,
                first: 0,
                second: 2,
            },
            BoardTransform::SwapColumns {
                stack: 1,
                first: 0,
                second: 2,
            },
            BoardTransform::SwapDigits {
                first: 1,
                second: 9,
            },
            BoardTransform::Transpose,
            BoardTransform::Rotate90,
            BoardTransform::Rotate180,
            BoardTransform::Rotate270,
            BoardTransform::ReflectHorizontal,
            BoardTransform::ReflectVertical,
        ];

        for transform in transforms {
            let transformed = transform_matrix(&SOLUTION, transform);
            let mut checked = transformed;
            assert!(
                sudoku_solver::solve(&mut checked),
                "transform={transform:?}"
            );
            assert_eq!(checked, transformed, "transform={transform:?}");
        }
    }

    #[test]
    fn orientation_transforms_map_corners_to_expected_positions() {
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::Transpose)[1][0],
            3
        );
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::Rotate90)[0][8],
            5
        );
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::Rotate180)[8][8],
            5
        );
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::Rotate270)[8][0],
            5
        );
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::ReflectHorizontal)[8][0],
            5
        );
        assert_eq!(
            transform_matrix(&SOLUTION, BoardTransform::ReflectVertical)[0][8],
            5
        );
    }

    #[test]
    fn row_indices_only_swap_rows_within_their_band() {
        assert_eq!(
            row_drag_transform(3, 5),
            Some(BoardTransform::SwapRows {
                band: 1,
                first: 0,
                second: 2,
            })
        );
        assert_eq!(row_drag_transform(1, 7), None);
        assert_eq!(row_drag_transform(4, 4), None);
        assert_eq!(row_drag_transform(9, 0), None);
    }

    #[test]
    fn band_indices_swap_only_whole_bands() {
        assert_eq!(
            band_drag_transform(0, 2),
            Some(BoardTransform::SwapBands {
                first: 0,
                second: 2
            })
        );
        assert_eq!(band_drag_transform(1, 1), None);
        assert_eq!(band_drag_transform(3, 0), None);
    }

    #[test]
    fn column_indices_only_swap_columns_within_their_stack() {
        assert_eq!(
            column_drag_transform(6, 8),
            Some(BoardTransform::SwapColumns {
                stack: 2,
                first: 0,
                second: 2,
            })
        );
        assert_eq!(column_drag_transform(1, 7), None);
        assert_eq!(column_drag_transform(2, 2), None);
        assert_eq!(column_drag_transform(0, 9), None);
    }

    #[test]
    fn stack_indices_swap_only_whole_stacks() {
        assert_eq!(
            stack_drag_transform(0, 2),
            Some(BoardTransform::SwapStacks {
                first: 0,
                second: 2
            })
        );
        assert_eq!(stack_drag_transform(1, 1), None);
        assert_eq!(stack_drag_transform(3, 0), None);
    }

    #[test]
    fn band_stack_row_column_and_digit_swaps_map_values_as_selected() {
        let bands = transform_matrix(
            &SOLUTION,
            BoardTransform::SwapBands {
                first: 0,
                second: 2,
            },
        );
        assert_eq!(bands[0], SOLUTION[6]);
        assert_eq!(bands[6], SOLUTION[0]);

        let stacks = transform_matrix(
            &SOLUTION,
            BoardTransform::SwapStacks {
                first: 0,
                second: 2,
            },
        );
        for row in 0..9 {
            assert_eq!(&stacks[row][0..3], &SOLUTION[row][6..9]);
            assert_eq!(&stacks[row][6..9], &SOLUTION[row][0..3]);
        }

        let rows = transform_matrix(
            &SOLUTION,
            BoardTransform::SwapRows {
                band: 1,
                first: 0,
                second: 2,
            },
        );
        assert_eq!(rows[3], SOLUTION[5]);
        assert_eq!(rows[5], SOLUTION[3]);

        let columns = transform_matrix(
            &SOLUTION,
            BoardTransform::SwapColumns {
                stack: 1,
                first: 0,
                second: 2,
            },
        );
        for row in 0..9 {
            assert_eq!(columns[row][3], SOLUTION[row][5]);
            assert_eq!(columns[row][5], SOLUTION[row][3]);
        }

        let digits = transform_matrix(
            &SOLUTION,
            BoardTransform::SwapDigits {
                first: 1,
                second: 9,
            },
        );
        for y in 0..9 {
            for x in 0..9 {
                let expected = match SOLUTION[y][x] {
                    1 => 9,
                    9 => 1,
                    value => value,
                };
                assert_eq!(digits[y][x], expected);
            }
        }
    }

    #[test]
    fn invalid_transform_indices_leave_the_board_unchanged() {
        for transform in [
            BoardTransform::SwapBands {
                first: 0,
                second: 3,
            },
            BoardTransform::SwapStacks {
                first: 0,
                second: 3,
            },
            BoardTransform::SwapRows {
                band: 3,
                first: 0,
                second: 1,
            },
            BoardTransform::SwapColumns {
                stack: 0,
                first: 3,
                second: 1,
            },
            BoardTransform::SwapDigits {
                first: 0,
                second: 1,
            },
        ] {
            assert_eq!(transform_matrix(&SOLUTION, transform), SOLUTION);
        }
    }
}
