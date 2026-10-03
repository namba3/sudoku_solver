use dioxus::prelude::*;
use wasm_bindgen::JsCast;

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();
    dioxus::launch(app);
}

const INITIAL_MTX: sudoku_solver::Matrix = [
    [0, 8, 0, 0, 0, 0, 1, 5, 0],
    [4, 0, 6, 5, 0, 9, 0, 8, 0],
    [0, 0, 0, 0, 0, 8, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 2, 0, 4, 0, 0, 0, 3],
    [3, 0, 0, 8, 0, 1, 0, 0, 0],
    [9, 0, 0, 0, 7, 0, 0, 0, 0],
    [6, 0, 0, 0, 0, 0, 0, 0, 4],
    [1, 5, 0, 0, 0, 0, 0, 9, 0],
];

#[derive(Clone, Copy, PartialEq)]
struct BoardSnapshot {
    board: sudoku_solver::Matrix,
    givens: sudoku_solver::Matrix,
}

const HISTORY_LIMIT: usize = 100;

fn app() -> Element {
    let mut mtx = use_signal(|| INITIAL_MTX);
    let mut givens = use_signal(|| INITIAL_MTX);
    let mut txt = use_signal(|| to_txt(&INITIAL_MTX));
    let mut msg = use_signal(String::new);
    let mut is_ok = use_signal(|| true);
    let mut show_candidates = use_signal(|| false);
    let mut undo_stack = use_signal(Vec::<BoardSnapshot>::new);
    let mut redo_stack = use_signal(Vec::<BoardSnapshot>::new);

    let msg_class = if is_ok() { "msg ok" } else { "msg error" };
    let board = *mtx.read();
    let conflicts = conflicting_cells(&board);
    let no_candidates = no_candidate_cells(&board);
    let candidate_toggle_label = if show_candidates() {
        "Hide candidates"
    } else {
        "Show candidates"
    };

    rsx! {
        div {
            class: "container",
            h1 { "Sudoku Solver" }
            p { class: "{msg_class}", role: "status", " {msg} " }
            p {
                class: "input-help",
                "Use arrow keys to move. Enter a digit to jump to the next empty cell. Ctrl/Cmd+Z: Undo; Ctrl/Cmd+Y or Ctrl/Cmd+Shift+Z: Redo."
            }
            ul {
                class: "matrix",
                for (y, row) in board.iter().enumerate() {
                    li {
                        key: "{y}",
                        ul {
                            class: "row",
                            for (x, cell) in row.iter().copied().enumerate() {
                                li {
                                    class: "{cell_classes(y, x, givens.read()[y][x] != 0, conflicts[y][x], no_candidates[y][x], show_candidates() && !(1..=9).contains(&cell))}",
                                    key: "{y}-{x}",
                                    if show_candidates() && !(1..=9).contains(&cell) {
                                        div {
                                            class: "candidate-grid",
                                            for digit in sudoku_solver::candidates_for(&board, x, y) {
                                                span {
                                                    class: "candidate",
                                                    style: "{candidate_position(digit)}",
                                                    "{digit}"
                                                }
                                            }
                                            if no_candidates[y][x] {
                                                span { class: "candidate-empty", "×" }
                                            }
                                        }
                                    }
                                    input {
                                        id: "cell-{y}-{x}",
                                        r#type: "number",
                                        max: "9",
                                        min: "1",
                                        inputmode: "numeric",
                                        value: "{cell_text(cell)}",
                                        aria_label: "Row {y + 1}, column {x + 1}",
                                        aria_invalid: "{conflicts[y][x] || no_candidates[y][x]}",
                                        oninput: move |evt| {
                                            let before = BoardSnapshot {
                                                board: *mtx.read(),
                                                givens: *givens.read(),
                                            };
                                            let mut board = *mtx.read();
                                            board[y][x] = cell_value(&evt.value());

                                            let mut original_clues = *givens.read();
                                            original_clues[y][x] = 0;

                                            record_board_change(
                                                &mut undo_stack.write(),
                                                &mut redo_stack.write(),
                                                before,
                                                BoardSnapshot {
                                                    board,
                                                    givens: original_clues,
                                                },
                                            );
                                            mtx.set(board);
                                            givens.set(original_clues);

                                            let conflict_count = count_conflict_cells(&board);
                                            if conflict_count > 0 {
                                                msg.set(format!("Resolve conflicts in {conflict_count} highlighted cells."));
                                                is_ok.set(false);
                                            } else {
                                                let stuck_count = count_no_candidate_cells(&board);
                                                if stuck_count > 0 {
                                                    msg.set(format!("No candidates remain in {stuck_count} empty cells."));
                                                    is_ok.set(false);
                                                } else {
                                                    msg.set(String::new());
                                                    is_ok.set(true);
                                                }
                                            }

                                            if board[y][x] != 0 {
                                                if let Some((next_y, next_x)) = next_empty_cell(&board, (y, x)) {
                                                    focus_cell(next_y, next_x);
                                                }
                                            }
                                        },
                                        onkeydown: move |evt| {
                                            let key = evt.key().to_string();
                                            let modifiers = evt.modifiers();
                                            let command_key = dioxus::html::input_data::keyboard_types::Modifiers::CONTROL;
                                            let meta_key = dioxus::html::input_data::keyboard_types::Modifiers::META;
                                            let shift_key = dioxus::html::input_data::keyboard_types::Modifiers::SHIFT;
                                            if (modifiers.contains(command_key) || modifiers.contains(meta_key))
                                                && matches!(key.to_lowercase().as_str(), "z" | "y")
                                            {
                                                evt.prevent_default();
                                                let is_redo = key.eq_ignore_ascii_case("y") || modifiers.contains(shift_key);
                                                if is_redo {
                                                    redo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok);
                                                } else {
                                                    undo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok);
                                                }
                                                return;
                                            }

                                            let movement = match key.as_str() {
                                                "ArrowUp" => Some((-1, 0)),
                                                "ArrowDown" => Some((1, 0)),
                                                "ArrowLeft" => Some((0, -1)),
                                                "ArrowRight" => Some((0, 1)),
                                                _ => None,
                                            };

                                            if let Some((dy, dx)) = movement {
                                                evt.prevent_default();
                                                if let Some((next_y, next_x)) = moved_cell(y, x, dy, dx) {
                                                    focus_cell(next_y, next_x);
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: "buttons",
                button {
                    disabled: undo_stack.read().is_empty(),
                    onclick: move |_| {
                        undo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok);
                    },
                    "↶ Undo"
                }
                button {
                    disabled: redo_stack.read().is_empty(),
                    onclick: move |_| {
                        redo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok);
                    },
                    "↷ Redo"
                }
                button {
                    class: "left",
                    onclick: move |_| {
                        match parse_puzzle(&txt.read()) {
                            Ok(board) => {
                                let before = BoardSnapshot {
                                    board: *mtx.read(),
                                    givens: *givens.read(),
                                };
                                record_board_change(
                                    &mut undo_stack.write(),
                                    &mut redo_stack.write(),
                                    before,
                                    BoardSnapshot { board, givens: board },
                                );
                                mtx.set(board);
                                givens.set(board);
                                let conflict_count = count_conflict_cells(&board);
                                if conflict_count > 0 {
                                    msg.set(format!("Puzzle loaded. Resolve conflicts in {conflict_count} highlighted cells."));
                                    is_ok.set(false);
                                } else {
                                    let stuck_count = count_no_candidate_cells(&board);
                                    if stuck_count > 0 {
                                        msg.set(format!("Puzzle loaded. No candidates remain in {stuck_count} empty cells."));
                                        is_ok.set(false);
                                    } else {
                                        msg.set("Puzzle loaded.".to_string());
                                        is_ok.set(true);
                                    }
                                }
                                focus_cell(0, 0);
                            }
                            Err(error) => {
                                msg.set(error);
                                is_ok.set(false);
                            }
                        }
                    },
                    "↑ Load text"
                }
                button {
                    aria_pressed: "{show_candidates()}",
                    onclick: move |_| show_candidates.set(!show_candidates()),
                    "{candidate_toggle_label}"
                }
                button {
                    onclick: move |_| {
                        let mut solver_mtx = *mtx.read();
                        let conflict_count = count_conflict_cells(&solver_mtx);
                        let stuck_count = count_no_candidate_cells(&solver_mtx);

                        if conflict_count > 0 {
                            msg.set(format!("Resolve conflicts in {conflict_count} highlighted cells before solving."));
                            is_ok.set(false);
                        } else if stuck_count > 0 {
                            msg.set(format!("No candidates remain in {stuck_count} empty cells."));
                            is_ok.set(false);
                        } else {
                            let window = web_sys::window().unwrap();
                            let performance = window.performance().unwrap();
                            let t_start = performance.now();
                            let succeeded = sudoku_solver::solve(&mut solver_mtx);
                            let ms = performance.now() - t_start;
                            log::debug!("time: {ms:.0} ms");

                            if succeeded {
                                record_board_change(
                                    &mut undo_stack.write(),
                                    &mut redo_stack.write(),
                                    BoardSnapshot {
                                        board: *mtx.read(),
                                        givens: *givens.read(),
                                    },
                                    BoardSnapshot {
                                        board: solver_mtx,
                                        givens: *givens.read(),
                                    },
                                );
                                mtx.set(solver_mtx);
                                msg.set(format!("Solved in {ms:.0} ms!"));
                                is_ok.set(true);
                            } else {
                                msg.set(format!("No solution found, {ms:.0} ms."));
                                is_ok.set(false);
                            }
                        }
                    },
                    "Solve"
                }
                button {
                    onclick: move |_| {
                        let before = BoardSnapshot {
                            board: *mtx.read(),
                            givens: *givens.read(),
                        };
                        record_board_change(
                            &mut undo_stack.write(),
                            &mut redo_stack.write(),
                            before,
                            BoardSnapshot {
                                board: [[0; 9]; 9],
                                givens: [[0; 9]; 9],
                            },
                        );
                        mtx.set([[0; 9]; 9]);
                        givens.set([[0; 9]; 9]);
                        msg.set(String::new());
                        is_ok.set(true);
                        focus_cell(0, 0);
                    },
                    "Clear"
                }
                button {
                    class: "right",
                    onclick: move |_| {
                        txt.set(to_txt(&mtx.read()));
                    },
                    "Save text ↓"
                }
            }
            div {
                class: "text-panel",
                label {
                    r#for: "puzzle-text",
                    "Puzzle text"
                }
                p {
                    class: "text-help",
                    "Enter exactly 9 lines of 9 characters. Digits 1–9 are clues; 0, ., and _ are blank cells."
                }
                textarea {
                    id: "puzzle-text",
                    class: "text",
                    value: "{txt}",
                    onchange: move |evt| {
                        txt.set(evt.value());
                    }
                }
            }
        }
    }
}

fn cell_value(s: &str) -> u8 {
    s.parse::<u8>()
        .ok()
        .filter(|value| (1..=9).contains(value))
        .unwrap_or(0)
}

fn record_board_change(
    undo: &mut Vec<BoardSnapshot>,
    redo: &mut Vec<BoardSnapshot>,
    before: BoardSnapshot,
    after: BoardSnapshot,
) {
    if before == after {
        return;
    }

    undo.push(before);
    if undo.len() > HISTORY_LIMIT {
        undo.remove(0);
    }
    redo.clear();
}

fn undo_board(
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
) {
    if let Some(previous) = undo.write().pop() {
        let current = BoardSnapshot {
            board: *board.read(),
            givens: *givens.read(),
        };
        redo.write().push(current);
        board.set(previous.board);
        givens.set(previous.givens);
        set_board_message(msg, is_ok, &previous.board, "Change undone.");
    }
}

fn redo_board(
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
) {
    if let Some(next) = redo.write().pop() {
        let current = BoardSnapshot {
            board: *board.read(),
            givens: *givens.read(),
        };
        undo.write().push(current);
        board.set(next.board);
        givens.set(next.givens);
        set_board_message(msg, is_ok, &next.board, "Change redone.");
    }
}

fn set_board_message(
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    board: &sudoku_solver::Matrix,
    success_message: &str,
) {
    let conflict_count = count_conflict_cells(board);
    if conflict_count > 0 {
        msg.set(format!(
            "Resolve conflicts in {conflict_count} highlighted cells."
        ));
        is_ok.set(false);
        return;
    }

    let stuck_count = count_no_candidate_cells(board);
    if stuck_count > 0 {
        msg.set(format!(
            "No candidates remain in {stuck_count} empty cells."
        ));
        is_ok.set(false);
    } else {
        msg.set(success_message.to_string());
        is_ok.set(true);
    }
}

fn cell_text(cell: u8) -> String {
    if (1..=9).contains(&cell) {
        cell.to_string()
    } else {
        String::new()
    }
}

fn cell_classes(
    y: usize,
    x: usize,
    is_given: bool,
    is_conflict: bool,
    no_candidates: bool,
    show_candidates: bool,
) -> String {
    let shade = if ((y / 3) + (x / 3)) % 2 == 1 {
        "odd"
    } else {
        "even"
    };
    let given = if is_given { " given" } else { "" };
    let conflict = if is_conflict { " conflict" } else { "" };
    let stuck = if no_candidates { " no-candidates" } else { "" };
    let candidates = if show_candidates {
        " with-candidates"
    } else {
        ""
    };
    format!("cell {shade}{given}{conflict}{stuck}{candidates}")
}

fn candidate_position(digit: u8) -> String {
    let index = digit.saturating_sub(1);
    format!("grid-area: {} / {}", index / 3 + 1, index % 3 + 1)
}

fn no_candidate_cells(board: &sudoku_solver::Matrix) -> [[bool; 9]; 9] {
    std::array::from_fn(|y| {
        std::array::from_fn(|x| {
            !(1..=9).contains(&board[y][x]) && sudoku_solver::candidates_for(board, x, y).is_empty()
        })
    })
}

fn count_no_candidate_cells(board: &sudoku_solver::Matrix) -> usize {
    no_candidate_cells(board)
        .iter()
        .flatten()
        .filter(|&&has_no_candidates| has_no_candidates)
        .count()
}

fn parse_puzzle(txt: &str) -> Result<sudoku_solver::Matrix, String> {
    let mut mtx = sudoku_solver::Matrix::default();
    let content = txt.trim_end_matches(|ch| ch == '\n' || ch == '\r');
    let lines: Vec<_> = content.lines().collect();
    if lines.len() != 9 {
        return Err(format!("Expected 9 lines, found {}.", lines.len()));
    }

    for (y, line) in lines.iter().enumerate() {
        let chars: Vec<_> = line.chars().collect();
        if chars.len() != 9 {
            return Err(format!(
                "Line {} must contain 9 characters, found {}.",
                y + 1,
                chars.len()
            ));
        }
        for (x, ch) in chars.into_iter().enumerate() {
            mtx[y][x] = match ch {
                '1'..='9' => ch as u8 - b'0',
                '0' | '.' | '_' => 0,
                _ => {
                    return Err(format!(
                        "Unsupported character at row {}, column {}. Use digits, 0, . or _.",
                        y + 1,
                        x + 1
                    ));
                }
            };
        }
    }
    Ok(mtx)
}

fn to_txt(mtx: &sudoku_solver::Matrix) -> String {
    let mut s = String::with_capacity((9 + 1) * 9);
    for row in mtx {
        for &cell in row {
            s.push(match cell {
                1..=9 => char::from(b'0' + cell),
                _ => '_',
            });
        }
        s.push('\n');
    }
    s
}

fn conflicting_cells(board: &sudoku_solver::Matrix) -> [[bool; 9]; 9] {
    let mut conflicts = [[false; 9]; 9];
    for y in 0..9 {
        for x in 0..9 {
            let value = board[y][x];
            if !(1..=9).contains(&value) {
                continue;
            }
            for other_y in 0..9 {
                for other_x in 0..9 {
                    if (other_y, other_x) <= (y, x) {
                        continue;
                    }
                    let shares_unit = y == other_y
                        || x == other_x
                        || (y / 3 == other_y / 3 && x / 3 == other_x / 3);
                    if shares_unit && board[other_y][other_x] == value {
                        conflicts[y][x] = true;
                        conflicts[other_y][other_x] = true;
                    }
                }
            }
        }
    }
    conflicts
}

fn count_conflict_cells(board: &sudoku_solver::Matrix) -> usize {
    conflicting_cells(board)
        .iter()
        .flatten()
        .filter(|&&has_conflict| has_conflict)
        .count()
}

fn moved_cell(y: usize, x: usize, dy: isize, dx: isize) -> Option<(usize, usize)> {
    let next_y = y as isize + dy;
    let next_x = x as isize + dx;
    if (0..9).contains(&next_y) && (0..9).contains(&next_x) {
        Some((next_y as usize, next_x as usize))
    } else {
        None
    }
}

fn next_empty_cell(
    board: &sudoku_solver::Matrix,
    (y, x): (usize, usize),
) -> Option<(usize, usize)> {
    let current = y * 9 + x;
    (1..81)
        .map(|offset| (current + offset) % 81)
        .map(|index| (index / 9, index % 9))
        .find(|&(next_y, next_x)| board[next_y][next_x] == 0)
}

fn focus_cell(y: usize, x: usize) {
    let id = format!("cell-{y}-{x}");
    let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(&id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };
    let _ = input.focus();
}
