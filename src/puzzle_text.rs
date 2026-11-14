use std::fmt;
use sudoku_solver::Matrix;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PuzzleTextError {
    CellCount { actual: usize },
    InvalidCharacter { row: usize, column: usize },
}

impl fmt::Display for PuzzleTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CellCount { actual } => write!(
                formatter,
                "expected 81 cells after removing line breaks, got {actual}"
            ),
            Self::InvalidCharacter { row, column } => {
                write!(
                    formatter,
                    "unsupported character at row {row}, column {column}"
                )
            }
        }
    }
}

/// Parse a puzzle as 81 cells, ignoring line-feed and carriage-return characters.
///
/// Digits 1 through 9 are clues. Zero, period, and underscore represent empty
/// cells. Line breaks may be used to separate rows, but are not required.
pub fn parse_board_text(text: &str) -> Result<Matrix, PuzzleTextError> {
    let cells = text
        .chars()
        .filter(|&character| !matches!(character, '\n' | '\r'))
        .collect::<Vec<_>>();
    if cells.len() != 81 {
        return Err(PuzzleTextError::CellCount {
            actual: cells.len(),
        });
    }

    let mut board = Matrix::default();
    for (index, character) in cells.into_iter().enumerate() {
        let row = index / 9;
        let column = index % 9;
        board[row][column] = match character {
            '1'..='9' => character as u8 - b'0',
            '0' | '.' | '_' => 0,
            _ => {
                return Err(PuzzleTextError::InvalidCharacter {
                    row: row + 1,
                    column: column + 1,
                });
            }
        };
    }
    Ok(board)
}

#[cfg(test)]
mod tests {
    use super::{parse_board_text, PuzzleTextError};

    #[test]
    fn parses_crlf_text_and_all_empty_markers() {
        let text = concat!(
            "10._56789\r\n",
            ".........\r\n",
            "_________\r\n",
            "000000000\r\n",
            "123456789\r\n",
            "123456789\r\n",
            "123456789\r\n",
            "123456789\r\n",
            "123456789\r\n",
        );
        let board = parse_board_text(text).unwrap();
        assert_eq!(board[0], [1, 0, 0, 0, 5, 6, 7, 8, 9]);
        assert!(board[1..4].iter().flatten().all(|&cell| cell == 0));
        assert_eq!(board[4], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn parses_flat_text_and_arbitrary_line_breaks() {
        let flat = "123456789".repeat(9);
        let expected = parse_board_text(&flat).unwrap();
        let with_breaks = format!("{}\r\n{}\n", &flat[..27], &flat[27..]);
        assert_eq!(parse_board_text(&with_breaks), Ok(expected));
    }

    #[test]
    fn reports_cell_count_and_character_errors() {
        assert_eq!(
            parse_board_text("123456789\n"),
            Err(PuzzleTextError::CellCount { actual: 9 })
        );
        let invalid = "12345678x".to_owned() + &".........".repeat(8);
        assert_eq!(
            parse_board_text(&invalid),
            Err(PuzzleTextError::InvalidCharacter { row: 1, column: 9 })
        );
    }
}
