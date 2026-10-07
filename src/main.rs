use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

mod board_analysis;
mod board_transform;
mod i18n;

use board_analysis::{candidate_digits, BoardAnalysis};
use board_transform::{transform_matrix, BoardTransform};
use i18n::{t, t_args, t_plural, Language};

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct BoardSnapshot {
    board: sudoku_solver::Matrix,
    givens: sudoku_solver::Matrix,
}

const HISTORY_LIMIT: usize = 100;
const UI_MAX_SOLUTIONS: usize = 100;

fn random_seed() -> u32 {
    let mut bytes = [0_u8; 4];
    let crypto_seed = web_sys::window()
        .and_then(|window| window.crypto().ok())
        .and_then(|crypto| {
            crypto
                .get_random_values_with_u8_array(&mut bytes)
                .ok()
                .map(|_| u32::from_ne_bytes(bytes))
        });

    crypto_seed.unwrap_or_else(|| (js_sys::Math::random() * (f64::from(u32::MAX) + 1.0)) as u32)
}

fn cell_aria_label(language: Language, row: usize, column: usize) -> String {
    t_args(
        language,
        "message.cell_aria",
        &[
            ("row", (row + 1).to_string()),
            ("column", (column + 1).to_string()),
        ],
    )
}

fn solution_range_label(language: Language, selected: usize, total: usize) -> String {
    t_args(
        language,
        "message.solution_range",
        &[
            ("selected", selected.to_string()),
            ("total", total.to_string()),
        ],
    )
}

fn solution_preview_label(language: Language, selected: usize) -> String {
    t_args(
        language,
        "message.solution_preview",
        &[("selected", selected.to_string())],
    )
}

fn conflict_message(language: Language, count: usize) -> String {
    t_plural(language, "message.conflicts", count as u64, &[])
}

fn no_candidates_message(language: Language, count: usize) -> String {
    t_plural(language, "message.no_candidates", count as u64, &[])
}

fn puzzle_loaded_message(
    language: Language,
    conflicts: Option<usize>,
    no_candidates: Option<usize>,
) -> String {
    match (conflicts, no_candidates) {
        (Some(count), _) => t_plural(
            language,
            "message.puzzle_loaded_conflicts",
            count as u64,
            &[],
        ),
        (_, Some(count)) => t_plural(
            language,
            "message.puzzle_loaded_no_candidates",
            count as u64,
            &[],
        ),
        _ => t(language, "message.puzzle_loaded"),
    }
}

#[derive(Clone, PartialEq)]
enum SearchStatus {
    Starting,
    Generating,
    Progress {
        solutions: u64,
        nodes: u64,
    },
    Finished {
        solutions: u64,
        nodes: u64,
        termination: String,
        elapsed_seconds: f64,
    },
    Cancelled {
        solutions: usize,
        elapsed_seconds: f64,
    },
    Generated {
        clues: usize,
        elapsed_seconds: f64,
    },
    GenerationCancelled {
        elapsed_seconds: f64,
    },
    GenerationError {
        message: String,
        elapsed_seconds: f64,
    },
    Error {
        message: String,
        elapsed_seconds: f64,
    },
    WorkerError(String),
    StartError(String),
}

impl SearchStatus {
    fn text(&self, language: Language) -> String {
        match self {
            Self::Starting => t(language, "search.starting"),
            Self::Generating => t(language, "search.generating"),
            Self::Progress { solutions, nodes } => t_plural(
                language,
                "search.progress",
                *solutions,
                &[("nodes", nodes.to_string())],
            ),
            Self::Finished {
                solutions,
                nodes,
                termination,
                elapsed_seconds,
            } => {
                let seconds = format!("{elapsed_seconds:.2}");
                let key = match termination.as_str() {
                    "solution_limit" => "search.finished.solution_limit",
                    "node_limit" => "search.finished.node_limit",
                    _ => "search.finished.complete",
                };
                t_plural(
                    language,
                    key,
                    *solutions,
                    &[("nodes", nodes.to_string()), ("seconds", seconds)],
                )
            }
            Self::Cancelled {
                solutions,
                elapsed_seconds,
            } => t_plural(
                language,
                "search.cancelled",
                *solutions as u64,
                &[("seconds", format!("{elapsed_seconds:.2}"))],
            ),
            Self::Generated {
                clues,
                elapsed_seconds,
            } => t_plural(
                language,
                "search.generated",
                *clues as u64,
                &[("seconds", format!("{elapsed_seconds:.2}"))],
            ),
            Self::GenerationCancelled { elapsed_seconds } => t_args(
                language,
                "search.generation_cancelled",
                &[("seconds", format!("{elapsed_seconds:.2}"))],
            ),
            Self::GenerationError {
                message,
                elapsed_seconds,
            } => t_args(
                language,
                "search.generation_error",
                &[
                    ("message", message.clone()),
                    ("seconds", format!("{elapsed_seconds:.2}")),
                ],
            ),
            Self::Error {
                message,
                elapsed_seconds,
            } => {
                let message = localize_search_error(message, language);
                t_args(
                    language,
                    "search.error",
                    &[
                        ("message", message),
                        ("seconds", format!("{elapsed_seconds:.2}")),
                    ],
                )
            }
            Self::WorkerError(message) => t_args(
                language,
                "search.worker_error",
                &[("message", message.clone())],
            ),
            Self::StartError(message) => t_args(
                language,
                "search.start_error",
                &[("message", message.clone())],
            ),
        }
    }
}

fn localize_search_error(message: &str, language: Language) -> String {
    match message {
        "Conflicting values." => t(language, "search.error.conflict"),
        "Expected 81 cell values and non-zero UI search limits." => {
            t(language, "search.error.invalid_request")
        }
        _ => message.to_string(),
    }
}

struct ActiveSearch {
    worker: web_sys::Worker,
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onerror: Closure<dyn FnMut(web_sys::ErrorEvent)>,
}

impl Drop for ActiveSearch {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

fn app() -> Element {
    let mut language = use_signal(|| Language::Japanese);
    let mut mtx = use_signal(|| INITIAL_MTX);
    let mut givens = use_signal(|| INITIAL_MTX);
    let mut txt = use_signal(|| to_txt(&INITIAL_MTX));
    let mut msg = use_signal(String::new);
    let mut is_ok = use_signal(|| true);
    let mut show_candidates = use_signal(|| false);
    let mut lock_givens = use_signal(|| true);
    let mut band_first = use_signal(|| 0u8);
    let mut band_second = use_signal(|| 1u8);
    let mut row_band = use_signal(|| 0u8);
    let mut row_first = use_signal(|| 0u8);
    let mut row_second = use_signal(|| 1u8);
    let mut stack_first = use_signal(|| 0u8);
    let mut stack_second = use_signal(|| 1u8);
    let mut column_stack = use_signal(|| 0u8);
    let mut column_first = use_signal(|| 0u8);
    let mut column_second = use_signal(|| 1u8);
    let mut digit_first = use_signal(|| 1u8);
    let mut digit_second = use_signal(|| 2u8);
    let mut undo_stack = use_signal(Vec::<BoardSnapshot>::new);
    let mut redo_stack = use_signal(Vec::<BoardSnapshot>::new);
    let mut search_solutions = use_signal(Vec::<sudoku_solver::Matrix>::new);
    let mut search_board = use_signal(|| None::<sudoku_solver::Matrix>);
    let mut search_status = use_signal(|| None::<SearchStatus>);
    let mut search_started_at = use_signal(|| 0.0f64);
    let mut copy_feedback = use_signal(|| None::<bool>);
    let mut is_searching = use_signal(|| false);
    let mut is_generating = use_signal(|| false);
    let mut generation_clue_count = use_signal(|| "30".to_string());
    let mut search_job_id = use_signal(|| 0u32);
    let mut active_search = use_signal(|| None::<ActiveSearch>);
    let mut selected_solution = use_signal(|| 0usize);

    let lang = language();
    let msg_class = if is_ok() { "msg ok" } else { "msg error" };
    let board = *mtx.read();
    let found_solutions = search_solutions.read();
    let solution_source_board = *search_board.read();
    let solution_count = found_solutions.len();
    let can_apply_selected_solution =
        search_board.read().as_ref() == Some(&board) || found_solutions.contains(&board);
    let analysis = BoardAnalysis::new(&board);
    let candidate_toggle_label = if show_candidates() {
        t(lang, "action.hide_candidates")
    } else {
        t(lang, "action.show_candidates")
    };
    let givens_lock_label = if lock_givens() {
        t(lang, "action.unlock_givens")
    } else {
        t(lang, "action.lock_givens")
    };
    let language_switch_label = t(lang, "language.switch");
    let title_label = t(lang, "app.title");
    let input_help = t(lang, "help.input");
    let undo_label = format!("↶ {}", t(lang, "action.undo"));
    let redo_label = format!("↷ {}", t(lang, "action.redo"));
    let load_text_label = t(lang, "action.load_text");
    let count_solutions_label = t(lang, "action.count_solutions");
    let generate_puzzle_label = if is_generating() {
        t(lang, "action.generating")
    } else {
        t(lang, "action.generate")
    };
    let generation_clue_count_label = t(lang, "label.clues");
    let generation_clue_count_help = t(lang, "label.clue_range");
    let invalid_generation_clue_count = t(lang, "message.invalid_clue_count");
    let cancel_task_label = if is_generating() {
        t(lang, "action.cancel_generation")
    } else {
        t(lang, "action.cancel_search")
    };
    let solve_label = t(lang, "action.solve");
    let reset_label = t(lang, "action.reset");
    let clear_label = t(lang, "action.clear");
    let reset_message = t(lang, "message.reset");
    let save_text_label = t(lang, "action.save_text");
    let found_solutions_label = t(lang, "label.found_solutions");
    let previous_solution_label = t(lang, "action.previous_solution");
    let select_solution_label = t(lang, "action.select_solution");
    let next_solution_label = t(lang, "action.next_solution");
    let apply_solution_label = t(lang, "action.apply_solution");
    let puzzle_text_label = t(lang, "label.puzzle_text");
    let copy_text_label = t(lang, "action.copy_text");
    let copy_success_label = t(lang, "message.copied");
    let copy_error_label = t(lang, "message.copy_failed");
    let history_group_label = t(lang, "group.history");
    let board_group_label = t(lang, "group.board");
    let transform_group_label = t(lang, "group.transform");
    let bands_label = t(lang, "label.bands");
    let stacks_label = t(lang, "label.stacks");
    let rows_label = t(lang, "label.rows_in_band");
    let columns_label = t(lang, "label.columns_in_stack");
    let digits_label = t(lang, "label.digits");
    let orientation_label = t(lang, "label.orientation");
    let first_band_label = t(lang, "label.first_band");
    let second_band_label = t(lang, "label.second_band");
    let row_band_label = t(lang, "label.row_band");
    let first_row_label = t(lang, "label.first_row");
    let second_row_label = t(lang, "label.second_row");
    let first_stack_label = t(lang, "label.first_stack");
    let second_stack_label = t(lang, "label.second_stack");
    let column_stack_label = t(lang, "label.column_stack");
    let first_column_label = t(lang, "label.first_column");
    let second_column_label = t(lang, "label.second_column");
    let first_digit_label = t(lang, "label.first_digit");
    let second_digit_label = t(lang, "label.second_digit");
    let swap_bands_label = t(lang, "action.swap_bands");
    let swap_stacks_label = t(lang, "action.swap_stacks");
    let swap_rows_label = t(lang, "action.swap_rows");
    let swap_columns_label = t(lang, "action.swap_columns");
    let swap_digits_label = t(lang, "action.swap_digits");
    let transpose_label = t(lang, "action.transpose");
    let rotate_90_label = t(lang, "action.rotate_90");
    let rotate_180_label = t(lang, "action.rotate_180");
    let rotate_270_label = t(lang, "action.rotate_270");
    let reflect_horizontal_label = t(lang, "action.reflect_horizontal");
    let reflect_vertical_label = t(lang, "action.reflect_vertical");
    let solve_group_label = t(lang, "group.solve");
    let text_group_label = t(lang, "group.text");
    let puzzle_text_help = t(lang, "help.puzzle_text");
    let stale_solution_message = t(lang, "message.stale_solutions");

    rsx! {
        document::Link {
            rel: "icon",
            href: "favicon.svg",
            r#type: "image/svg+xml",
        }
        document::Link {
            rel: "icon",
            href: "favicon.ico",
            r#type: "image/x-icon",
        }
        div {
            class: "container",
            lang: "{lang.code()}",
            div {
                class: "language-control",
                button {
                    aria_label: "Switch display language / 表示言語を切り替え",
                    onclick: move |_| {
                        language.set(language().toggle());
                        msg.set(String::new());
                    },
                    "{language_switch_label}"
                }
            }
            h1 { "{title_label}" }
            p { class: "{msg_class}", role: "status", " {msg} " }
            p {
                class: "input-help",
                "{input_help}"
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
                                    class: "{cell_classes(y, x, givens.read()[y][x] != 0, analysis.conflicts[y][x], analysis.no_candidates[y][x], show_candidates() && !(1..=9).contains(&cell))}",
                                    key: "{y}-{x}",
                                    if show_candidates() && !(1..=9).contains(&cell) {
                                        div {
                                            class: "candidate-grid",
                                            for digit in candidate_digits(analysis.candidate_masks[y][x]) {
                                                span {
                                                    class: "candidate",
                                                    style: "{candidate_position(digit)}",
                                                    "{digit}"
                                                }
                                            }
                                            if analysis.no_candidates[y][x] {
                                                span { class: "candidate-empty", "×" }
                                            }
                                        }
                                    }
                                    input {
                                        id: "cell-{y}-{x}",
                                        r#type: "text",
                                        maxlength: "1",
                                        inputmode: "numeric",
                                        value: "{cell_text(cell)}",
                                        aria_label: "{cell_aria_label(lang, y, x)}",
                                        aria_invalid: "{analysis.conflicts[y][x] || analysis.no_candidates[y][x]}",
                                        readonly: is_cell_locked(lock_givens(), *givens.read(), y, x),
                                        oninput: move |evt| {
                                            update_cell_value(
                                                y,
                                                x,
                                                cell_value(&evt.value()),
                                                lock_givens(),
                                                &mut mtx,
                                                &mut givens,
                                                &mut undo_stack,
                                                &mut redo_stack,
                                                &mut msg,
                                                &mut is_ok,
                                                lang,
                                            );
                                        },
                                        onkeydown: move |evt| {
                                            let key = evt.key().to_string();
                                            let modifiers = evt.modifiers();
                                            let command_key = dioxus::html::input_data::keyboard_types::Modifiers::CONTROL;
                                            let meta_key = dioxus::html::input_data::keyboard_types::Modifiers::META;
                                            let shift_key = dioxus::html::input_data::keyboard_types::Modifiers::SHIFT;
                                            let alt_key = dioxus::html::input_data::keyboard_types::Modifiers::ALT;
                                            if (modifiers.contains(command_key) || modifiers.contains(meta_key))
                                                && matches!(key.to_lowercase().as_str(), "z" | "y")
                                            {
                                                evt.prevent_default();
                                                let is_redo = key.eq_ignore_ascii_case("y") || modifiers.contains(shift_key);
                                                if is_redo {
                                                    redo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok, lang);
                                                } else {
                                                    undo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok, lang);
                                                }
                                                return;
                                            }

                                            if !modifiers.contains(command_key)
                                                && !modifiers.contains(meta_key)
                                                && !modifiers.contains(alt_key)
                                            {
                                                if let Some(digit) = key
                                                    .parse::<u8>()
                                                    .ok()
                                                    .filter(|digit| (1..=9).contains(digit))
                                                {
                                                    evt.prevent_default();
                                                    update_cell_value(
                                                        y,
                                                        x,
                                                        digit,
                                                        lock_givens(),
                                                        &mut mtx,
                                                        &mut givens,
                                                        &mut undo_stack,
                                                        &mut redo_stack,
                                                        &mut msg,
                                                        &mut is_ok,
                                                        lang,
                                                    );
                                                    return;
                                                }
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
                                                return;
                                            }

                                            if modifiers.contains(command_key) || modifiers.contains(meta_key) {
                                                return;
                                            }

                                            if !matches!(
                                                key.as_str(),
                                                "Backspace"
                                                    | "Delete"
                                                    | "Tab"
                                                    | "Home"
                                                    | "End"
                                                    | "Shift"
                                                    | "Control"
                                                    | "Alt"
                                                    | "Meta"
                                            ) {
                                                evt.prevent_default();
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
                class: "action-groups",
                div {
                    class: "button-group",
                    role: "group",
                    aria_label: "{history_group_label}",
                    h2 { class: "button-group-title", "{history_group_label}" }
                    button {
                        disabled: undo_stack.read().is_empty(),
                        onclick: move |_| {
                            undo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok, lang);
                        },
                        "{undo_label}"
                    }
                    button {
                        disabled: redo_stack.read().is_empty(),
                        onclick: move |_| {
                            redo_board(&mut undo_stack, &mut redo_stack, &mut mtx, &mut givens, &mut msg, &mut is_ok, lang);
                        },
                        "{redo_label}"
                    }
                }
                div {
                    class: "button-group",
                    role: "group",
                    aria_label: "{board_group_label}",
                    h2 { class: "button-group-title", "{board_group_label}" }
                    button {
                        aria_pressed: "{show_candidates()}",
                        onclick: move |_| show_candidates.set(!show_candidates()),
                        "{candidate_toggle_label}"
                    }
                    button {
                        class: "given-lock-toggle",
                        aria_pressed: "{lock_givens()}",
                        onclick: move |_| lock_givens.set(!lock_givens()),
                        "{givens_lock_label}"
                    }
                    div {
                        class: "generator-options",
                        label {
                            r#for: "generation-clue-count",
                            "{generation_clue_count_label}"
                        }
                        input {
                            id: "generation-clue-count",
                            r#type: "number",
                            min: "17",
                            max: "81",
                            step: "1",
                            value: "{generation_clue_count}",
                            aria_label: "{generation_clue_count_label}",
                            oninput: move |event| generation_clue_count.set(event.value()),
                        }
                        span { "{generation_clue_count_help}" }
                    }
                button {
                    disabled: is_searching(),
                    onclick: move |_| {
                        let Some(clue_count) = generation_clue_count()
                            .parse::<usize>()
                            .ok()
                            .filter(|count| (17..=81).contains(count))
                        else {
                            msg.set(invalid_generation_clue_count.to_string());
                            is_ok.set(false);
                            return;
                        };

                        active_search.set(None);
                        let job_id = search_job_id().wrapping_add(1);
                        search_job_id.set(job_id);
                        search_solutions.set(Vec::new());
                        selected_solution.set(0);
                        search_board.set(None);
                        msg.set(String::new());
                        is_ok.set(true);
                        let started_at = web_sys::window()
                            .and_then(|window| window.performance())
                            .map(|performance| performance.now())
                            .unwrap_or(0.0);
                        search_started_at.set(started_at);
                        search_status.set(Some(SearchStatus::Generating));
                        is_searching.set(true);
                        is_generating.set(true);

                        match create_search_worker() {
                            Ok(worker) => {
                                let mut status = search_status;
                                let mut searching = is_searching;
                                let mut generating = is_generating;
                                let current_job_id = search_job_id;
                                let started_at = search_started_at();
                                let mut board_signal = mtx;
                                let mut givens_signal = givens;
                                let mut text_signal = txt;
                                let mut undo = undo_stack;
                                let mut redo = redo_stack;
                                let language_signal = language;
                                let mut message = msg;
                                let mut message_ok = is_ok;
                                let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                                    let Some(raw_message) = event.data().as_string() else {
                                        return;
                                    };
                                    let Ok(response) = serde_json::from_str::<serde_json::Value>(&raw_message) else {
                                        return;
                                    };
                                    if response["job_id"].as_u64() != Some(job_id as u64)
                                        || current_job_id() != job_id
                                    {
                                        return;
                                    }

                                    match response["type"].as_str() {
                                        Some("generated") => {
                                            let Some(generated_board) = parse_solution(&response["board"]) else {
                                                status.set(Some(SearchStatus::GenerationError {
                                                    message: t(language_signal(), "worker.error.invalid_board"),
                                                    elapsed_seconds: elapsed_seconds_since(started_at),
                                                }));
                                                searching.set(false);
                                                generating.set(false);
                                                return;
                                            };
                                            let before = BoardSnapshot {
                                                board: *board_signal.read(),
                                                givens: *givens_signal.read(),
                                            };
                                            record_board_change(
                                                &mut undo.write(),
                                                &mut redo.write(),
                                                before,
                                                BoardSnapshot { board: generated_board, givens: generated_board },
                                            );
                                            board_signal.set(generated_board);
                                            givens_signal.set(generated_board);
                                            text_signal.set(to_txt(&generated_board));
                                            message.set(t(language_signal(), "message.puzzle_generated"));
                                            message_ok.set(true);
                                            status.set(Some(SearchStatus::Generated {
                                                clues: response["clues"].as_u64().unwrap_or(0) as usize,
                                                elapsed_seconds: elapsed_seconds_since(started_at),
                                            }));
                                            searching.set(false);
                                            generating.set(false);
                                            focus_cell(0, 0);
                                        }
                                        Some("error") => {
                                            status.set(Some(SearchStatus::GenerationError {
                                                message: response["message"].as_str().map_or_else(
                                                    || t(language_signal(), "worker.error.unknown_generation"),
                                                    str::to_owned,
                                                ),
                                                elapsed_seconds: elapsed_seconds_since(started_at),
                                            }));
                                            searching.set(false);
                                            generating.set(false);
                                        }
                                        _ => {}
                                    }
                                });
                                let mut error_status = search_status;
                                let mut error_searching = is_searching;
                                let mut error_generating = is_generating;
                                let onerror = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |event: web_sys::ErrorEvent| {
                                    if current_job_id() == job_id {
                                        error_status.set(Some(SearchStatus::GenerationError {
                                            message: event.message(),
                                            elapsed_seconds: elapsed_seconds_since(started_at),
                                        }));
                                        error_searching.set(false);
                                        error_generating.set(false);
                                    }
                                });

                                worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                                worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                                active_search.set(Some(ActiveSearch {
                                    worker: worker.clone(),
                                    _onmessage: onmessage,
                                    _onerror: onerror,
                                }));
                                let seed = random_seed();
                                let request = serde_json::json!({
                                    "type": "generate",
                                    "job_id": job_id,
                                    "seed": seed,
                                    "clue_count": clue_count,
                                });
                                if let Err(error) = worker.post_message(&wasm_bindgen::JsValue::from_str(&request.to_string())) {
                                    active_search.set(None);
                                    search_status.set(Some(SearchStatus::GenerationError {
                                        message: format!("{error:?}"),
                                        elapsed_seconds: elapsed_seconds_since(started_at),
                                    }));
                                    is_searching.set(false);
                                    is_generating.set(false);
                                }
                            }
                            Err(error) => {
                                search_status.set(Some(SearchStatus::GenerationError {
                                    message: format!("{error:?}"),
                                    elapsed_seconds: elapsed_seconds_since(started_at),
                                }));
                                is_searching.set(false);
                                is_generating.set(false);
                            }
                        }
                    },
                    "{generate_puzzle_label}"
                }
                button {
                    disabled: board == *givens.read(),
                    onclick: move |_| {
                        let initial_clues = *givens.read();
                        record_board_change(
                            &mut undo_stack.write(),
                            &mut redo_stack.write(),
                            BoardSnapshot {
                                board: *mtx.read(),
                                givens: initial_clues,
                            },
                            BoardSnapshot {
                                board: initial_clues,
                                givens: initial_clues,
                            },
                        );
                        mtx.set(initial_clues);
                        msg.set(reset_message.to_string());
                        is_ok.set(true);
                        focus_cell(0, 0);
                    },
                    "{reset_label}"
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
                    "{clear_label}"
                }
                }
                details {
                    class: "transform-panel",
                    summary { "{transform_group_label}" }
                    div {
                        class: "transform-controls",
                        fieldset {
                            class: "transform-control",
                            legend { "{bands_label}" }
                            label {
                                "{first_band_label}"
                                select {
                                    aria_label: "{first_band_label}",
                                    value: "{band_first()}",
                                    onchange: move |event| band_first.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{second_band_label}"
                                select {
                                    aria_label: "{second_band_label}",
                                    value: "{band_second()}",
                                    onchange: move |event| band_second.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            button {
                                disabled: band_first() == band_second(),
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::SwapBands {
                                        first: band_first() as usize,
                                        second: band_second() as usize,
                                    },
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{swap_bands_label}"
                            }
                        }
                        fieldset {
                            class: "transform-control",
                            legend { "{rows_label}" }
                            label {
                                "{row_band_label}"
                                select {
                                    aria_label: "{row_band_label}",
                                    value: "{row_band()}",
                                    onchange: move |event| row_band.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{first_row_label}"
                                select {
                                    aria_label: "{first_row_label}",
                                    value: "{row_first()}",
                                    onchange: move |event| row_first.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{second_row_label}"
                                select {
                                    aria_label: "{second_row_label}",
                                    value: "{row_second()}",
                                    onchange: move |event| row_second.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            button {
                                disabled: row_first() == row_second(),
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::SwapRows {
                                        band: row_band() as usize,
                                        first: row_first() as usize,
                                        second: row_second() as usize,
                                    },
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{swap_rows_label}"
                            }
                        }
                        fieldset {
                            class: "transform-control",
                            legend { "{stacks_label}" }
                            label {
                                "{first_stack_label}"
                                select {
                                    aria_label: "{first_stack_label}",
                                    value: "{stack_first()}",
                                    onchange: move |event| stack_first.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{second_stack_label}"
                                select {
                                    aria_label: "{second_stack_label}",
                                    value: "{stack_second()}",
                                    onchange: move |event| stack_second.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            button {
                                disabled: stack_first() == stack_second(),
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::SwapStacks {
                                        first: stack_first() as usize,
                                        second: stack_second() as usize,
                                    },
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{swap_stacks_label}"
                            }
                        }
                        fieldset {
                            class: "transform-control",
                            legend { "{columns_label}" }
                            label {
                                "{column_stack_label}"
                                select {
                                    aria_label: "{column_stack_label}",
                                    value: "{column_stack()}",
                                    onchange: move |event| column_stack.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{first_column_label}"
                                select {
                                    aria_label: "{first_column_label}",
                                    value: "{column_first()}",
                                    onchange: move |event| column_first.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            label {
                                "{second_column_label}"
                                select {
                                    aria_label: "{second_column_label}",
                                    value: "{column_second()}",
                                    onchange: move |event| column_second.set(event.value().parse().unwrap_or(0)),
                                    for value in 0u8..3 {
                                        option { value: "{value}", "{value + 1}" }
                                    }
                                }
                            }
                            button {
                                disabled: column_first() == column_second(),
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::SwapColumns {
                                        stack: column_stack() as usize,
                                        first: column_first() as usize,
                                        second: column_second() as usize,
                                    },
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{swap_columns_label}"
                            }
                        }
                        fieldset {
                            class: "transform-control",
                            legend { "{digits_label}" }
                            label {
                                "{first_digit_label}"
                                select {
                                    aria_label: "{first_digit_label}",
                                    value: "{digit_first()}",
                                    onchange: move |event| digit_first.set(event.value().parse().unwrap_or(1)),
                                    for value in 1u8..=9 {
                                        option { value: "{value}", "{value}" }
                                    }
                                }
                            }
                            label {
                                "{second_digit_label}"
                                select {
                                    aria_label: "{second_digit_label}",
                                    value: "{digit_second()}",
                                    onchange: move |event| digit_second.set(event.value().parse().unwrap_or(2)),
                                    for value in 1u8..=9 {
                                        option { value: "{value}", "{value}" }
                                    }
                                }
                            }
                            button {
                                disabled: digit_first() == digit_second(),
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::SwapDigits {
                                        first: digit_first(),
                                        second: digit_second(),
                                    },
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{swap_digits_label}"
                            }
                        }
                        fieldset {
                            class: "transform-control transform-orientation",
                            legend { "{orientation_label}" }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::Transpose,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{transpose_label}"
                            }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::Rotate90,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{rotate_90_label}"
                            }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::Rotate180,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{rotate_180_label}"
                            }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::Rotate270,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{rotate_270_label}"
                            }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::ReflectHorizontal,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{reflect_horizontal_label}"
                            }
                            button {
                                onclick: move |_| apply_board_transform(
                                    BoardTransform::ReflectVertical,
                                    &mut mtx, &mut givens, &mut undo_stack, &mut redo_stack,
                                    &mut msg, &mut is_ok, lang,
                                ),
                                "{reflect_vertical_label}"
                            }
                        }
                    }
                }
                div {
                    class: "button-group",
                    role: "group",
                    aria_label: "{solve_group_label}",
                    h2 { class: "button-group-title", "{solve_group_label}" }
                button {
                    disabled: is_searching(),
                    onclick: move |_| {
                        active_search.set(None);
                        let job_id = search_job_id().wrapping_add(1);
                        search_job_id.set(job_id);
                        search_solutions.set(Vec::new());
                        selected_solution.set(0);
                        search_board.set(Some(board));
                        let started_at = web_sys::window()
                            .and_then(|window| window.performance())
                            .map(|performance| performance.now())
                            .unwrap_or(0.0);
                        search_started_at.set(started_at);
                        search_status.set(Some(SearchStatus::Starting));
                        is_searching.set(true);
                        is_generating.set(false);

                        match create_search_worker() {
                            Ok(worker) => {
                                let mut solutions = search_solutions;
                                let mut status = search_status;
                                let mut searching = is_searching;
                                let current_job_id = search_job_id;
                                let mut error_status = search_status;
                                let mut error_searching = is_searching;
                                let started_at = search_started_at();
                                let language_for_worker = lang;
                                let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                                    let Some(raw_message) = event.data().as_string() else {
                                        return;
                                    };
                                    let Ok(message) = serde_json::from_str::<serde_json::Value>(&raw_message) else {
                                        return;
                                    };
                                    if message["job_id"].as_u64() != Some(job_id as u64)
                                        || current_job_id() != job_id
                                    {
                                        return;
                                    }

                                    match message["type"].as_str() {
                                        Some("solution") => {
                                            if let Some(solution) = parse_solution(&message["board"]) {
                                                solutions.write().push(solution);
                                            }
                                        }
                                        Some("progress") => {
                                            let count = message["solutions_found"].as_u64().unwrap_or(0);
                                            let nodes = message["nodes"].as_u64().unwrap_or(0);
                                            status.set(Some(SearchStatus::Progress { solutions: count, nodes }));
                                        }
                                        Some("finished") => {
                                            status.set(Some(SearchStatus::Finished {
                                                solutions: message["solutions_found"].as_u64().unwrap_or(0),
                                                nodes: message["explored_nodes"].as_u64().unwrap_or(0),
                                                termination: message["termination"].as_str().unwrap_or("exhausted").to_string(),
                                                elapsed_seconds: elapsed_seconds_since(started_at),
                                            }));
                                            searching.set(false);
                                        }
                                        Some("error") => {
                                            status.set(Some(SearchStatus::Error {
                                                message: message["message"].as_str().map_or_else(
                                                    || t(language_for_worker, "worker.error.search_failed"),
                                                    str::to_owned,
                                                ),
                                                elapsed_seconds: elapsed_seconds_since(started_at),
                                            }));
                                            searching.set(false);
                                        }
                                        _ => {}
                                    }
                                });
                                let onerror = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |event: web_sys::ErrorEvent| {
                                    if current_job_id() == job_id {
                                        error_status.set(Some(SearchStatus::WorkerError(event.message())));
                                        error_searching.set(false);
                                    }
                                });

                                worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
                                worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                                active_search.set(Some(ActiveSearch {
                                    worker: worker.clone(),
                                    _onmessage: onmessage,
                                    _onerror: onerror,
                                }));

                                let request = serde_json::json!({
                                    "type": "start",
                                    "job_id": job_id,
                                    "board": board.iter().flatten().copied().collect::<Vec<_>>(),
                                    "max_solutions": UI_MAX_SOLUTIONS,
                                    "max_nodes": 500_000u32,
                                });
                                if let Err(error) = worker.post_message(&wasm_bindgen::JsValue::from_str(&request.to_string())) {
                                    active_search.set(None);
                                    search_status.set(Some(SearchStatus::StartError(format!("{error:?}"))));
                                    is_searching.set(false);
                                }
                            }
                            Err(error) => {
                                search_status.set(Some(SearchStatus::StartError(format!("{error:?}"))));
                                is_searching.set(false);
                            }
                        }
                    },
                    "{count_solutions_label}"
                }
                if is_searching() {
                    button {
                        onclick: move |_| {
                            let was_generating = is_generating();
                            search_job_id.set(search_job_id().wrapping_add(1));
                            active_search.set(None);
                            is_searching.set(false);
                            is_generating.set(false);
                            let elapsed_seconds = elapsed_seconds_since(search_started_at());
                            if was_generating {
                                search_status.set(Some(SearchStatus::GenerationCancelled { elapsed_seconds }));
                            } else {
                                let count = search_solutions.read().len();
                                search_status.set(Some(SearchStatus::Cancelled { solutions: count, elapsed_seconds }));
                            }
                        },
                        "{cancel_task_label}"
                    }
                }
                button {
                    onclick: move |_| {
                        let mut solver_mtx = *mtx.read();
                        let analysis = BoardAnalysis::new(&solver_mtx);
                        let conflict_count = analysis.conflict_count();
                        let stuck_count = analysis.no_candidate_count();

                        if conflict_count > 0 {
                            msg.set(conflict_message(lang, conflict_count));
                            is_ok.set(false);
                        } else if stuck_count > 0 {
                            msg.set(no_candidates_message(lang, stuck_count));
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
                                msg.set(t_args(lang, "solve.success", &[("milliseconds", format!("{ms:.0}"))]));
                                is_ok.set(true);
                            } else {
                                msg.set(t_args(lang, "solve.no_solution", &[("milliseconds", format!("{ms:.0}"))]));
                                is_ok.set(false);
                            }
                        }
                    },
                    "{solve_label}"
                }
                }
                div {
                    class: "button-group",
                    role: "group",
                    aria_label: "{text_group_label}",
                    h2 { class: "button-group-title", "{text_group_label}" }
                button {
                    onclick: move |_| {
                        match parse_puzzle(&txt.read(), lang) {
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
                                let analysis = BoardAnalysis::new(&board);
                                let conflict_count = analysis.conflict_count();
                                if conflict_count > 0 {
                                    msg.set(format!("{}", puzzle_loaded_message(lang, Some(conflict_count), None)));
                                    is_ok.set(false);
                                } else {
                                    let stuck_count = analysis.no_candidate_count();
                                    if stuck_count > 0 {
                                        msg.set(puzzle_loaded_message(lang, None, Some(stuck_count)));
                                        is_ok.set(false);
                                    } else {
                                        msg.set(puzzle_loaded_message(lang, None, None));
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
                    "{load_text_label}"
                }
                button {
                    onclick: move |_| {
                        txt.set(to_txt(&mtx.read()));
                    },
                    "{save_text_label}"
                }
                }
            }
            if let Some(status) = search_status() {
                div {
                    class: "search-panel",
                    p { role: "status", "{status.text(lang)}" }
                    if !found_solutions.is_empty() {
                        div {
                            p { class: "solution-heading", "{found_solutions_label}" }
                            div {
                                class: "solution-carousel-controls",
                                button {
                                    class: "solution-step",
                                    aria_label: "{previous_solution_label}",
                                    disabled: selected_solution() == 0,
                                    onclick: move |_| {
                                        selected_solution.set(selected_solution().saturating_sub(1));
                                    },
                                    "‹"
                                }
                                div {
                                    class: "solution-range-control",
                                    input {
                                        id: "solution-range",
                                        r#type: "range",
                                        min: "0",
                                        max: "{solution_count - 1}",
                                        step: "1",
                                        value: "{selected_solution()}",
                                        aria_label: "{select_solution_label}",
                                        aria_valuetext: "{solution_range_label(lang, selected_solution() + 1, solution_count)}",
                                        oninput: move |evt| {
                                            let index = evt.value().parse::<usize>().unwrap_or(0);
                                            selected_solution.set(index.min(solution_count - 1));
                                        }
                                    }
                                    output { "{selected_solution() + 1} / {solution_count}" }
                                },
                                button {
                                    class: "solution-step",
                                    aria_label: "{next_solution_label}",
                                    disabled: selected_solution() + 1 >= solution_count,
                                    onclick: move |_| {
                                        selected_solution.set((selected_solution() + 1).min(solution_count - 1));
                                    },
                                    "›"
                                }
                            }
                            if let Some(solution) = found_solutions.get(selected_solution()) {
                                div {
                                    class: "solution-slide",
                                    key: "solution-slide-{selected_solution()}",
                                    table {
                                        class: "solution-preview",
                                        aria_label: "{solution_preview_label(lang, selected_solution() + 1)}",
                                        tbody {
                                            for (row_index, row) in solution.iter().enumerate() {
                                                tr {
                                                    key: "preview-row-{row_index}",
                                                    for (column_index, value) in row.iter().enumerate() {
                                                        td {
                                                            key: "preview-cell-{row_index}-{column_index}",
                                                            class: if solution_source_board.is_some_and(|source| source[row_index][column_index] != 0) { "solution-preview-given" } else { "" },
                                                            "{value}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            button {
                                disabled: !can_apply_selected_solution,
                                onclick: move |_| {
                                    if let Some(solution) = search_solutions.read().get(selected_solution()).copied() {
                                        record_board_change(
                                            &mut undo_stack.write(),
                                            &mut redo_stack.write(),
                                            BoardSnapshot {
                                                board: *mtx.read(),
                                                givens: *givens.read(),
                                            },
                                            BoardSnapshot {
                                                board: solution,
                                                givens: *givens.read(),
                                            },
                                        );
                                        mtx.set(solution);
                                        msg.set(t(lang, "message.solution_applied"));
                                        is_ok.set(true);
                                    }
                                },
                                "{apply_solution_label}"
                            }
                            if !can_apply_selected_solution {
                                p { "{stale_solution_message}" }
                            }
                        }
                    }
                }
            }
            div {
                class: "text-panel",
                div {
                    class: "text-label-row",
                    label {
                        r#for: "puzzle-text",
                        "{puzzle_text_label}"
                    }
                    button {
                        class: "copy-button",
                        r#type: "button",
                        aria_label: "{copy_text_label}",
                        title: "{copy_text_label}",
                        onclick: move |_| {
                            copy_feedback.set(None);
                            let text_to_copy = txt.read().clone();
                            let mut feedback = copy_feedback;
                            spawn(async move {
                                let result = async {
                                    let window = web_sys::window()
                                        .ok_or_else(|| wasm_bindgen::JsValue::from_str("Window is unavailable"))?;
                                    let clipboard = window.navigator().clipboard();
                                    wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text_to_copy))
                                        .await
                                        .map(|_| ())
                                }
                                .await;
                                feedback.set(Some(result.is_ok()));
                            });
                        },
                        svg {
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            rect { x: "8", y: "8", width: "13", height: "13", rx: "2" }
                            path { d: "M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" }
                        }
                    }
                }
                if let Some(copied) = copy_feedback() {
                    p {
                        class: "copy-feedback",
                        role: "status",
                        if copied { "{copy_success_label}" } else { "{copy_error_label}" }
                    }
                }
                p {
                    class: "text-help",
                    "{puzzle_text_help}"
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

fn update_cell_value(
    y: usize,
    x: usize,
    value: u8,
    lock_givens: bool,
    mtx: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    undo_stack: &mut Signal<Vec<BoardSnapshot>>,
    redo_stack: &mut Signal<Vec<BoardSnapshot>>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    let current_givens = *givens.read();
    if is_cell_locked(lock_givens, current_givens, y, x) {
        return;
    }

    let before = BoardSnapshot {
        board: *mtx.read(),
        givens: current_givens,
    };
    let mut board = *mtx.read();
    board[y][x] = value;

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

    let analysis = BoardAnalysis::new(&board);
    let conflict_count = analysis.conflict_count();
    if conflict_count > 0 {
        msg.set(conflict_message(language, conflict_count));
        is_ok.set(false);
    } else {
        let stuck_count = analysis.no_candidate_count();
        if stuck_count > 0 {
            msg.set(no_candidates_message(language, stuck_count));
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
}

fn is_cell_locked(lock_givens: bool, givens: sudoku_solver::Matrix, y: usize, x: usize) -> bool {
    lock_givens && (1..=9).contains(&givens[y][x])
}

fn apply_board_transform(
    transform: BoardTransform,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    message: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    let before = BoardSnapshot {
        board: *board.read(),
        givens: *givens.read(),
    };
    let after = transform_snapshot(before, transform);
    if before == after {
        return;
    }

    record_board_change(&mut undo.write(), &mut redo.write(), before, after);
    board.set(after.board);
    givens.set(after.givens);
    set_board_message(
        message,
        is_ok,
        &after.board,
        &t(language, "message.transform_applied"),
        language,
    );
}

fn transform_snapshot(before: BoardSnapshot, transform: BoardTransform) -> BoardSnapshot {
    BoardSnapshot {
        board: transform_matrix(&before.board, transform),
        givens: transform_matrix(&before.givens, transform),
    }
}

fn create_search_worker() -> Result<web_sys::Worker, wasm_bindgen::JsValue> {
    let window = web_sys::window()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("Window is unavailable"))?;
    let pathname = window.location().pathname()?;
    let base_path = if pathname == "/sudoku_solver" || pathname.starts_with("/sudoku_solver/") {
        "/sudoku_solver"
    } else {
        ""
    };
    let options = web_sys::WorkerOptions::new();
    options.set_type(web_sys::WorkerType::Module);
    web_sys::Worker::new_with_options(&format!("{base_path}/worker/entry.js"), &options)
}

fn parse_solution(value: &serde_json::Value) -> Option<sudoku_solver::Matrix> {
    let rows = value.as_array()?;
    if rows.len() != 9 {
        return None;
    }

    let mut board = [[0; 9]; 9];
    for (y, row) in rows.iter().enumerate() {
        let cells = row.as_array()?;
        if cells.len() != 9 {
            return None;
        }
        for (x, cell) in cells.iter().enumerate() {
            let value = u8::try_from(cell.as_u64()?).ok()?;
            if value > 9 {
                return None;
            }
            board[y][x] = value;
        }
    }
    Some(board)
}

fn elapsed_seconds_since(started_at: f64) -> f64 {
    let now = web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or(started_at);
    (now - started_at).max(0.0) / 1000.0
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

fn undo_snapshot(
    undo: &mut Vec<BoardSnapshot>,
    redo: &mut Vec<BoardSnapshot>,
    current: BoardSnapshot,
) -> Option<BoardSnapshot> {
    let previous = undo.pop()?;
    redo.push(current);
    Some(previous)
}

fn redo_snapshot(
    undo: &mut Vec<BoardSnapshot>,
    redo: &mut Vec<BoardSnapshot>,
    current: BoardSnapshot,
) -> Option<BoardSnapshot> {
    let next = redo.pop()?;
    undo.push(current);
    Some(next)
}

fn undo_board(
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    let current = BoardSnapshot {
        board: *board.read(),
        givens: *givens.read(),
    };
    let previous = {
        let mut undo_stack = undo.write();
        let mut redo_stack = redo.write();
        undo_snapshot(&mut undo_stack, &mut redo_stack, current)
    };
    if let Some(previous) = previous {
        board.set(previous.board);
        givens.set(previous.givens);
        set_board_message(
            msg,
            is_ok,
            &previous.board,
            &t(language, "message.undo"),
            language,
        );
    }
}

fn redo_board(
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    let current = BoardSnapshot {
        board: *board.read(),
        givens: *givens.read(),
    };
    let next = {
        let mut undo_stack = undo.write();
        let mut redo_stack = redo.write();
        redo_snapshot(&mut undo_stack, &mut redo_stack, current)
    };
    if let Some(next) = next {
        board.set(next.board);
        givens.set(next.givens);
        set_board_message(
            msg,
            is_ok,
            &next.board,
            &t(language, "message.redo"),
            language,
        );
    }
}

fn set_board_message(
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    board: &sudoku_solver::Matrix,
    success_message: &str,
    language: Language,
) {
    let analysis = BoardAnalysis::new(board);
    let conflict_count = analysis.conflict_count();
    if conflict_count > 0 {
        msg.set(conflict_message(language, conflict_count));
        is_ok.set(false);
        return;
    }

    let stuck_count = analysis.no_candidate_count();
    if stuck_count > 0 {
        msg.set(no_candidates_message(language, stuck_count));
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

fn parse_puzzle(txt: &str, language: Language) -> Result<sudoku_solver::Matrix, String> {
    let mut mtx = sudoku_solver::Matrix::default();
    let content = txt.trim_end_matches(|ch| ch == '\n' || ch == '\r');
    let lines: Vec<_> = content.lines().collect();
    if lines.len() != 9 {
        return Err(t_args(
            language,
            "parse.line_count",
            &[("actual", lines.len().to_string())],
        ));
    }

    for (y, line) in lines.iter().enumerate() {
        let chars: Vec<_> = line.chars().collect();
        if chars.len() != 9 {
            return Err(t_args(
                language,
                "parse.line_length",
                &[
                    ("row", (y + 1).to_string()),
                    ("actual", chars.len().to_string()),
                ],
            ));
        }
        for (x, ch) in chars.into_iter().enumerate() {
            mtx[y][x] = match ch {
                '1'..='9' => ch as u8 - b'0',
                '0' | '.' | '_' => 0,
                _ => {
                    return Err(t_args(
                        language,
                        "parse.invalid_character",
                        &[
                            ("row", (y + 1).to_string()),
                            ("column", (x + 1).to_string()),
                        ],
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

#[cfg(test)]
mod tests {
    use super::{
        cell_classes, cell_text, cell_value, is_cell_locked, moved_cell, next_empty_cell,
        parse_puzzle, parse_solution, record_board_change, redo_snapshot, to_txt,
        transform_snapshot, undo_snapshot, BoardSnapshot, BoardTransform, Language, SearchStatus,
        HISTORY_LIMIT,
    };
    use serde_json::json;
    use sudoku_solver::Matrix;

    const SOLVED_TEXT: &str = concat!(
        "534678912\n",
        "672195348\n",
        "198342567\n",
        "859761423\n",
        "426853791\n",
        "713924856\n",
        "961537284\n",
        "287419635\n",
        "345286179\n",
    );

    #[test]
    fn given_cells_are_locked_by_default_and_can_be_unlocked() {
        let mut givens = Matrix::default();
        givens[0][0] = 5;

        assert!(is_cell_locked(true, givens, 0, 0));
        assert!(!is_cell_locked(false, givens, 0, 0));
        assert!(!is_cell_locked(true, givens, 0, 1));
    }

    #[test]
    fn cell_value_accepts_only_single_digits_from_one_through_nine() {
        for (text, expected) in [
            ("1", 1),
            ("9", 9),
            ("", 0),
            ("0", 0),
            ("10", 0),
            ("1a", 0),
            ("-1", 0),
            ("x", 0),
            ("１", 0),
        ] {
            assert_eq!(cell_value(text), expected, "input={text:?}");
        }
    }

    #[test]
    fn cell_text_hides_values_outside_the_supported_digit_range() {
        assert_eq!(cell_text(1), "1");
        assert_eq!(cell_text(9), "9");
        assert_eq!(cell_text(0), "");
        assert_eq!(cell_text(10), "");
    }

    #[test]
    fn cell_classes_include_each_requested_visual_state() {
        assert_eq!(
            cell_classes(0, 0, true, true, true, true),
            "cell even given conflict no-candidates with-candidates"
        );
        assert_eq!(cell_classes(0, 3, false, false, false, false), "cell odd");
    }

    #[test]
    fn cell_movement_stops_at_board_edges() {
        assert_eq!(moved_cell(0, 0, -1, 0), None);
        assert_eq!(moved_cell(0, 0, 0, -1), None);
        assert_eq!(moved_cell(8, 8, 1, 0), None);
        assert_eq!(moved_cell(8, 8, 0, 1), None);
        assert_eq!(moved_cell(4, 4, -1, 1), Some((3, 5)));
    }

    #[test]
    fn next_empty_cell_searches_forward_and_wraps_once() {
        let mut board = [[1; 9]; 9];
        board[0][2] = 0;
        board[8][8] = 0;

        assert_eq!(next_empty_cell(&board, (0, 0)), Some((0, 2)));
        assert_eq!(next_empty_cell(&board, (0, 3)), Some((8, 8)));
        assert_eq!(next_empty_cell(&board, (8, 8)), Some((0, 2)));

        board[0][2] = 1;
        assert_eq!(next_empty_cell(&board, (8, 8)), None);
    }

    #[test]
    fn parses_worker_boards_only_when_the_shape_and_values_are_valid() {
        let mut valid = json!([
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0]
        ]);
        valid[0][0] = json!(1);
        valid[8][8] = json!(9);
        let board = parse_solution(&valid).unwrap();
        assert_eq!(board[0][0], 1);
        assert_eq!(board[8][8], 9);

        assert!(parse_solution(&json!([])).is_none());
        assert!(parse_solution(&json!([[0]])).is_none());

        let mut short_row = valid.clone();
        short_row[0] = json!([0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(parse_solution(&short_row).is_none());

        let mut invalid_cell = valid.clone();
        invalid_cell[0][0] = json!("1");
        assert!(parse_solution(&invalid_cell).is_none());

        let mut out_of_range = valid;
        out_of_range[0][0] = json!(10);
        assert!(parse_solution(&out_of_range).is_none());
    }

    #[test]
    fn search_status_formats_limits_and_localizes_known_errors() {
        let solution_limit = SearchStatus::Finished {
            solutions: 100,
            nodes: 500_000,
            termination: "solution_limit".to_owned(),
            elapsed_seconds: 1.25,
        }
        .text(Language::English);
        assert!(solution_limit.contains("At least 100 solutions"));
        assert!(solution_limit.contains("500000 nodes in 1.25 s"));

        let node_limit = SearchStatus::Finished {
            solutions: 2,
            nodes: 500_000,
            termination: "node_limit".to_owned(),
            elapsed_seconds: 1.25,
        }
        .text(Language::English);
        assert!(node_limit.contains("exact count is unknown"));

        let complete = SearchStatus::Finished {
            solutions: 1,
            nodes: 42,
            termination: "exhausted".to_owned(),
            elapsed_seconds: 0.5,
        }
        .text(Language::Japanese);
        assert!(complete.contains("探索完了"));

        let localized = SearchStatus::Error {
            message: "Conflicting values.".to_owned(),
            elapsed_seconds: 0.25,
        }
        .text(Language::Japanese);
        assert!(localized.contains("同じ行・列・ブロック"));

        let unknown = SearchStatus::Error {
            message: "unknown worker error".to_owned(),
            elapsed_seconds: 0.25,
        }
        .text(Language::English);
        assert!(unknown.contains("unknown worker error"));
    }

    #[test]
    fn search_status_formats_progress_with_locale_specific_plural_rules() {
        let singular = SearchStatus::Progress {
            solutions: 1,
            nodes: 2048,
        }
        .text(Language::English);
        assert!(singular.contains("1 solution found"));
        assert!(singular.contains("2048 nodes"));

        let plural = SearchStatus::Progress {
            solutions: 2,
            nodes: 4096,
        }
        .text(Language::English);
        assert!(plural.contains("2 solutions found"));

        let japanese = SearchStatus::Progress {
            solutions: 2,
            nodes: 4096,
        }
        .text(Language::Japanese);
        assert!(japanese.contains("4096 ノード"));
        assert!(japanese.contains("2 件の解"));
    }

    #[test]
    fn search_status_formats_generation_cancellation_and_failures() {
        assert_eq!(
            SearchStatus::Starting.text(Language::English),
            "Starting solution search…"
        );
        assert_eq!(
            SearchStatus::Generating.text(Language::English),
            "Generating a unique puzzle…"
        );
        assert!(SearchStatus::Cancelled {
            solutions: 2,
            elapsed_seconds: 1.5,
        }
        .text(Language::English)
        .contains("2 solutions"));
        assert!(SearchStatus::Generated {
            clues: 30,
            elapsed_seconds: 0.5,
        }
        .text(Language::English)
        .contains("30 clues"));
        assert!(SearchStatus::GenerationCancelled {
            elapsed_seconds: 0.5,
        }
        .text(Language::Japanese)
        .contains("問題の生成を中断"));
        assert!(SearchStatus::GenerationError {
            message: "test failure".to_owned(),
            elapsed_seconds: 0.5,
        }
        .text(Language::English)
        .contains("test failure"));
        assert!(SearchStatus::WorkerError("worker failed".to_owned())
            .text(Language::English)
            .contains("worker failed"));
        assert!(SearchStatus::StartError("startup failed".to_owned())
            .text(Language::English)
            .contains("startup failed"));
    }

    #[test]
    fn parses_a_valid_board_and_all_supported_empty_markers() {
        let text = concat!(
            "10._56789\n",
            ".........\n",
            "_________\n",
            "000000000\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
        );

        let board = parse_puzzle(text, Language::Japanese).unwrap();
        assert_eq!(board[0], [1, 0, 0, 0, 5, 6, 7, 8, 9]);
        assert!(board[1..4].iter().flatten().all(|&cell| cell == 0));
        assert_eq!(board[4], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn parses_the_text_export_format_with_a_trailing_newline() {
        let board = parse_puzzle(SOLVED_TEXT, Language::English).unwrap();

        assert_eq!(to_txt(&board), SOLVED_TEXT);
        assert_eq!(parse_puzzle(&to_txt(&board), Language::English), Ok(board));
    }

    #[test]
    fn parses_crlf_input_and_rejects_whitespace() {
        let text = SOLVED_TEXT.replace('\n', "\r\n");
        let parsed = parse_puzzle(&text, Language::English).unwrap();
        assert_eq!(to_txt(&parsed), SOLVED_TEXT);

        let text_with_space = SOLVED_TEXT.replacen('5', " ", 1);
        assert!(parse_puzzle(&text_with_space, Language::English)
            .unwrap_err()
            .contains("Unsupported character"));
    }

    #[test]
    fn reports_localized_line_count_errors() {
        assert_eq!(
            parse_puzzle("123456789\n", Language::Japanese),
            Err("9行必要ですが、1行あります。".to_owned())
        );
        assert_eq!(
            parse_puzzle("123456789\n", Language::English),
            Err("Expected 9 lines, found 1.".to_owned())
        );
    }

    #[test]
    fn reports_the_row_and_expected_length_for_short_lines() {
        let text = concat!(
            "123456789\n",
            "12345678\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
        );

        assert_eq!(
            parse_puzzle(text, Language::Japanese),
            Err("2 行目は9文字必要ですが、8文字あります。".to_owned())
        );
        assert_eq!(
            parse_puzzle(text, Language::English),
            Err("Line 2 must contain 9 characters, found 8.".to_owned())
        );
    }

    #[test]
    fn reports_the_row_and_column_of_an_unsupported_character() {
        let text = concat!(
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "123456789\n",
            "12345x789\n",
        );

        assert_eq!(
            parse_puzzle(text, Language::Japanese),
            Err("9 行 6 列に使えない文字があります。数字、0、.、_を使ってください。".to_owned())
        );
        assert_eq!(
            parse_puzzle(text, Language::English),
            Err("Unsupported character at row 9, column 6. Use digits, 0, . or _.".to_owned())
        );
    }

    #[test]
    fn text_export_preserves_empty_cells_as_underscores() {
        let mut board = Matrix::default();
        board[0][0] = 7;
        board[8][8] = 2;

        let text = to_txt(&board);
        assert_eq!(text.lines().count(), 9);
        assert_eq!(text.lines().next(), Some("7________"));
        assert_eq!(text.lines().last(), Some("________2"));
        assert_eq!(parse_puzzle(&text, Language::Japanese), Ok(board));
    }

    fn snapshot(board_value: u8, given_value: u8) -> BoardSnapshot {
        BoardSnapshot {
            board: [[board_value; 9]; 9],
            givens: [[given_value; 9]; 9],
        }
    }

    #[test]
    fn transform_snapshot_applies_the_same_change_to_board_and_givens() {
        let mut before = snapshot(0, 0);
        before.board[0][0] = 4;
        before.givens[0][0] = 7;

        let after = transform_snapshot(
            before,
            BoardTransform::SwapBands {
                first: 0,
                second: 2,
            },
        );

        assert_eq!(after.board[6][0], 4);
        assert_eq!(after.givens[6][0], 7);
        assert_eq!(after.board[0][0], 0);
        assert_eq!(after.givens[0][0], 0);
    }

    #[test]
    fn recording_an_unchanged_board_preserves_both_history_stacks() {
        let current = snapshot(1, 2);
        let mut undo = vec![snapshot(3, 4)];
        let mut redo = vec![snapshot(5, 6)];

        record_board_change(&mut undo, &mut redo, current, current);

        assert_eq!(undo, vec![snapshot(3, 4)]);
        assert_eq!(redo, vec![snapshot(5, 6)]);
    }

    #[test]
    fn recording_a_change_pushes_undo_and_clears_redo() {
        let before = snapshot(1, 2);
        let after = snapshot(3, 4);
        let mut undo = Vec::new();
        let mut redo = vec![snapshot(5, 6)];

        record_board_change(&mut undo, &mut redo, before, after);

        assert_eq!(undo, vec![before]);
        assert!(redo.is_empty());
    }

    #[test]
    fn undo_and_redo_round_trip_board_and_givens_together() {
        let original = snapshot(1, 2);
        let changed = snapshot(3, 4);
        let mut undo = vec![original];
        let mut redo = Vec::new();

        assert_eq!(undo_snapshot(&mut undo, &mut redo, changed), Some(original));
        assert!(undo.is_empty());
        assert_eq!(redo, vec![changed]);

        assert_eq!(redo_snapshot(&mut undo, &mut redo, original), Some(changed));
        assert_eq!(undo, vec![original]);
        assert!(redo.is_empty());
    }

    #[test]
    fn undo_and_redo_without_history_leave_the_other_stack_untouched() {
        let current = snapshot(1, 2);
        let mut undo = Vec::new();
        let mut redo = vec![snapshot(3, 4)];

        assert_eq!(undo_snapshot(&mut undo, &mut redo, current), None);
        assert!(undo.is_empty());
        assert_eq!(redo, vec![snapshot(3, 4)]);

        redo.clear();
        undo.push(snapshot(5, 6));
        assert_eq!(redo_snapshot(&mut undo, &mut redo, current), None);
        assert_eq!(undo, vec![snapshot(5, 6)]);
        assert!(redo.is_empty());
    }

    #[test]
    fn board_history_discards_the_oldest_entry_at_its_limit() {
        let mut undo = Vec::new();
        let mut redo = Vec::new();

        for value in 0..=HISTORY_LIMIT as u8 {
            record_board_change(
                &mut undo,
                &mut redo,
                snapshot(value, value),
                snapshot(value + 1, value + 1),
            );
        }

        assert_eq!(undo.len(), HISTORY_LIMIT);
        assert_eq!(undo.first(), Some(&snapshot(1, 1)));
        assert_eq!(
            undo.last(),
            Some(&snapshot(HISTORY_LIMIT as u8, HISTORY_LIMIT as u8))
        );
    }
}
