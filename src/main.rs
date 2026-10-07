use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

mod i18n;

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

#[derive(Clone, Copy, PartialEq)]
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
                                        r#type: "number",
                                        max: "9",
                                        min: "1",
                                        inputmode: "numeric",
                                        value: "{cell_text(cell)}",
                                        aria_label: "{cell_aria_label(lang, y, x)}",
                                        aria_invalid: "{analysis.conflicts[y][x] || analysis.no_candidates[y][x]}",
                                        oninput: move |evt| {
                                            update_cell_value(
                                                y,
                                                x,
                                                cell_value(&evt.value()),
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
    mtx: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    undo_stack: &mut Signal<Vec<BoardSnapshot>>,
    redo_stack: &mut Signal<Vec<BoardSnapshot>>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    let before = BoardSnapshot {
        board: *mtx.read(),
        givens: *givens.read(),
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
            board[y][x] = u8::try_from(cell.as_u64()?).ok()?;
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

fn undo_board(
    undo: &mut Signal<Vec<BoardSnapshot>>,
    redo: &mut Signal<Vec<BoardSnapshot>>,
    board: &mut Signal<sudoku_solver::Matrix>,
    givens: &mut Signal<sudoku_solver::Matrix>,
    msg: &mut Signal<String>,
    is_ok: &mut Signal<bool>,
    language: Language,
) {
    if let Some(previous) = undo.write().pop() {
        let current = BoardSnapshot {
            board: *board.read(),
            givens: *givens.read(),
        };
        redo.write().push(current);
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
    if let Some(next) = redo.write().pop() {
        let current = BoardSnapshot {
            board: *board.read(),
            givens: *givens.read(),
        };
        undo.write().push(current);
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

struct BoardAnalysis {
    conflicts: [[bool; 9]; 9],
    candidate_masks: [[u16; 9]; 9],
    no_candidates: [[bool; 9]; 9],
}

impl BoardAnalysis {
    fn new(board: &sudoku_solver::Matrix) -> Self {
        const ALL_CANDIDATES: u16 = (1 << 9) - 1;

        let mut row_masks = [0u16; 9];
        let mut column_masks = [0u16; 9];
        let mut block_masks = [0u16; 9];
        let mut row_duplicates = [0u16; 9];
        let mut column_duplicates = [0u16; 9];
        let mut block_duplicates = [0u16; 9];

        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                if !(1..=9).contains(&value) {
                    continue;
                }

                let bit = 1 << (value - 1);
                let block = (y / 3) * 3 + x / 3;
                if row_masks[y] & bit != 0 {
                    row_duplicates[y] |= bit;
                }
                if column_masks[x] & bit != 0 {
                    column_duplicates[x] |= bit;
                }
                if block_masks[block] & bit != 0 {
                    block_duplicates[block] |= bit;
                }
                row_masks[y] |= bit;
                column_masks[x] |= bit;
                block_masks[block] |= bit;
            }
        }

        let mut conflicts = [[false; 9]; 9];
        let mut candidate_masks = [[0u16; 9]; 9];
        let mut no_candidates = [[false; 9]; 9];
        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                let block = (y / 3) * 3 + x / 3;
                if (1..=9).contains(&value) {
                    let bit = 1 << (value - 1);
                    conflicts[y][x] = row_duplicates[y] & bit != 0
                        || column_duplicates[x] & bit != 0
                        || block_duplicates[block] & bit != 0;
                } else {
                    let candidates =
                        ALL_CANDIDATES & !(row_masks[y] | column_masks[x] | block_masks[block]);
                    candidate_masks[y][x] = candidates;
                    no_candidates[y][x] = candidates == 0;
                }
            }
        }

        Self {
            conflicts,
            candidate_masks,
            no_candidates,
        }
    }

    fn conflict_count(&self) -> usize {
        self.conflicts
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }

    fn no_candidate_count(&self) -> usize {
        self.no_candidates
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }
}

fn candidate_digits(mask: u16) -> impl Iterator<Item = u8> {
    (1..=9).filter(move |digit| mask & (1 << (digit - 1)) != 0)
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
    use super::{candidate_digits, parse_puzzle, to_txt, BoardAnalysis, Language};
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

    #[test]
    fn board_analysis_candidates_match_the_public_solver_api() {
        let board = parse_puzzle(
            concat!(
                "530070000\n",
                "600195000\n",
                "098000060\n",
                "800060003\n",
                "400803001\n",
                "700020006\n",
                "060000280\n",
                "000419005\n",
                "000080079\n",
            ),
            Language::English,
        )
        .unwrap();
        let analysis = BoardAnalysis::new(&board);

        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(
                    candidate_digits(analysis.candidate_masks[y][x]).collect::<Vec<_>>(),
                    sudoku_solver::candidates_for(&board, x, y),
                    "candidate mismatch at ({y}, {x})"
                );
            }
        }
    }

    #[test]
    fn board_analysis_marks_all_cells_in_duplicate_units() {
        let mut board = Matrix::default();
        board[0][0] = 5;
        board[0][1] = 5;
        board[3][3] = 6;
        board[4][3] = 6;
        board[6][6] = 7;
        board[7][7] = 7;

        let analysis = BoardAnalysis::new(&board);
        assert!(analysis.conflicts[0][0]);
        assert!(analysis.conflicts[0][1]);
        assert!(analysis.conflicts[3][3]);
        assert!(analysis.conflicts[4][3]);
        assert!(analysis.conflicts[6][6]);
        assert!(analysis.conflicts[7][7]);
        assert!(!analysis.conflicts[0][2]);
        assert_eq!(analysis.conflict_count(), 6);
    }

    #[test]
    fn board_analysis_detects_empty_cells_with_no_legal_digits() {
        let mut board = Matrix::default();
        board[0] = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        for (y, value) in [4, 6, 1, 2, 3, 5, 7, 8].into_iter().enumerate() {
            board[y + 1][0] = value;
        }
        board[1][1] = 9;

        let analysis = BoardAnalysis::new(&board);
        assert!(analysis.no_candidates[0][0]);
        assert_eq!(analysis.candidate_masks[0][0], 0);
        assert_eq!(analysis.no_candidate_count(), 1);
    }
}
