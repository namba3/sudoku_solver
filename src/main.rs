use dioxus::prelude::*;
use wasm_bindgen::closure::Closure;
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
const UI_MAX_SOLUTIONS: usize = 100;

#[derive(Clone, Copy, PartialEq)]
enum Language {
    Japanese,
    English,
}

impl Language {
    fn toggle(self) -> Self {
        match self {
            Self::Japanese => Self::English,
            Self::English => Self::Japanese,
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Japanese => "ja",
            Self::English => "en",
        }
    }
}

fn tr(language: Language, japanese: &'static str, english: &'static str) -> &'static str {
    match language {
        Language::Japanese => japanese,
        Language::English => english,
    }
}

fn cell_aria_label(language: Language, row: usize, column: usize) -> String {
    match language {
        Language::Japanese => format!("{} 行 {} 列", row + 1, column + 1),
        Language::English => format!("Row {}, column {}", row + 1, column + 1),
    }
}

fn solution_range_label(language: Language, selected: usize, total: usize) -> String {
    match language {
        Language::Japanese => format!("{total} 件中 {selected} 件目"),
        Language::English => format!("Solution {selected} of {total}"),
    }
}

fn solution_preview_label(language: Language, selected: usize) -> String {
    match language {
        Language::Japanese => format!("{selected} 件目の解のプレビュー"),
        Language::English => format!("Preview of solution {selected}"),
    }
}

fn conflict_message(language: Language, count: usize) -> String {
    match language {
        Language::Japanese => format!(
            "重複する数字があるセルは {count} 個です。強調表示されたセルを修正してください。"
        ),
        Language::English => format!("Resolve conflicts in {count} highlighted cells."),
    }
}

fn no_candidates_message(language: Language, count: usize) -> String {
    match language {
        Language::Japanese => {
            format!("候補がない空欄が {count} 個あります。強調表示されたセルを修正してください。")
        }
        Language::English => format!("No candidates remain in {count} empty cells."),
    }
}

fn puzzle_loaded_message(
    language: Language,
    conflicts: Option<usize>,
    no_candidates: Option<usize>,
) -> String {
    match (language, conflicts, no_candidates) {
        (Language::Japanese, Some(count), _) => {
            format!("問題を読み込みました。重複する数字があるセルは {count} 個です。")
        }
        (Language::Japanese, _, Some(count)) => {
            format!("問題を読み込みました。候補がない空欄が {count} 個あります。")
        }
        (Language::Japanese, _, _) => "問題を読み込みました。".to_string(),
        (Language::English, Some(count), _) => {
            format!("Puzzle loaded. Resolve conflicts in {count} highlighted cells.")
        }
        (Language::English, _, Some(count)) => {
            format!("Puzzle loaded. No candidates remain in {count} empty cells.")
        }
        (Language::English, _, _) => "Puzzle loaded.".to_string(),
    }
}

#[derive(Clone, PartialEq)]
enum SearchStatus {
    Starting,
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
            Self::Starting => tr(language, "解を探索しています…", "Starting solution search…").to_string(),
            Self::Progress { solutions, nodes } => match language {
                Language::Japanese => format!("探索中… {nodes} ノードを調べ、{solutions} 件の解が見つかりました。"),
                Language::English => format!("Searching… {solutions} solutions found across {nodes} nodes."),
            },
            Self::Finished { solutions, nodes, termination, elapsed_seconds } => {
                let seconds = format!("{elapsed_seconds:.2}");
                match (language, termination.as_str()) {
                    (Language::Japanese, "solution_limit") => format!("{solutions} 件以上の解が見つかりました。{nodes} ノード、{seconds} 秒で解数上限に達しました。"),
                    (Language::Japanese, "node_limit") => format!("{nodes} ノード、{seconds} 秒で探索を停止しました。{solutions} 件の解が見つかりましたが、正確な解数は不明です。"),
                    (Language::Japanese, _) => format!("探索完了: {nodes} ノードを{seconds} 秒で調べ、解は {solutions} 件です。"),
                    (Language::English, "solution_limit") => format!("At least {solutions} solutions found; the solution limit was reached after {nodes} nodes in {seconds} s."),
                    (Language::English, "node_limit") => format!("Search stopped at {nodes} nodes after finding {solutions} solutions in {seconds} s. The exact count is unknown."),
                    (Language::English, _) => format!("Search complete: exactly {solutions} solutions found across {nodes} nodes in {seconds} s."),
                }
            }
            Self::Cancelled { solutions, elapsed_seconds } => match language {
                Language::Japanese => format!("解を {solutions} 件見つけた時点で探索を中断しました（{elapsed_seconds:.2} 秒）。"),
                Language::English => format!("Search cancelled after finding {solutions} solutions in {elapsed_seconds:.2} s."),
            },
            Self::Error { message, elapsed_seconds } => {
                let message = localize_search_error(message, language);
                match language {
                    Language::Japanese => format!("{message}（{elapsed_seconds:.2} 秒）"),
                    Language::English => format!("{message} ({elapsed_seconds:.2} s)"),
                }
            }
            Self::WorkerError(message) => match language {
                Language::Japanese => format!("検索ワーカーでエラーが発生しました: {message}"),
                Language::English => format!("Solution worker failed: {message}"),
            },
            Self::StartError(message) => match language {
                Language::Japanese => format!("解探索を開始できませんでした: {message}"),
                Language::English => format!("Could not start solution search: {message}"),
            },
        }
    }
}

fn localize_search_error(message: &str, language: Language) -> String {
    match message {
        "Conflicting values." => tr(
            language,
            "同じ行・列・ブロックに重複する数字があります。",
            "Conflicting values.",
        )
        .to_string(),
        "Expected 81 cell values and non-zero UI search limits." => tr(
            language,
            "盤面データまたは探索上限が不正です。",
            "Invalid board data or search limits.",
        )
        .to_string(),
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
    let mut is_searching = use_signal(|| false);
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
    let conflicts = conflicting_cells(&board);
    let no_candidates = no_candidate_cells(&board);
    let candidate_toggle_label = if show_candidates() {
        tr(lang, "候補を隠す", "Hide candidates")
    } else {
        tr(lang, "候補を表示", "Show candidates")
    };
    let language_switch_label = tr(lang, "English", "日本語");
    let input_help = tr(lang, "矢印キーで移動、数字キーで入力して次の空欄へ移動します。Ctrl/Cmd+Z: 元に戻す、Ctrl/Cmd+Y または Ctrl/Cmd+Shift+Z: やり直し。", "Use arrow keys to move. Enter a digit to jump to the next empty cell. Ctrl/Cmd+Z: Undo; Ctrl/Cmd+Y or Ctrl/Cmd+Shift+Z: Redo.");
    let undo_label = format!("↶ {}", tr(lang, "元に戻す", "Undo"));
    let redo_label = format!("↷ {}", tr(lang, "やり直す", "Redo"));
    let load_text_label = tr(lang, "↑ テキストを読み込む", "↑ Load text");
    let hint_button_label = tr(
        lang,
        "ヒント（一時停止中）",
        "Get hint (temporarily unavailable)",
    );
    let count_solutions_label = tr(lang, "解の数を調べる", "Count solutions");
    let cancel_search_label = tr(lang, "探索を中断", "Cancel search");
    let solve_label = tr(lang, "解く", "Solve");
    let clear_label = tr(lang, "クリア", "Clear");
    let save_text_label = tr(lang, "テキストに保存 ↓", "Save text ↓");
    let found_solutions_label = tr(lang, "見つかった解", "Found solutions");
    let previous_solution_label = tr(lang, "前の解", "Previous solution");
    let select_solution_label = tr(lang, "見つかった解を選択", "Select found solution");
    let next_solution_label = tr(lang, "次の解", "Next solution");
    let apply_solution_label = tr(lang, "選択した解を適用", "Use selected solution");
    let puzzle_text_label = tr(lang, "問題のテキスト", "Puzzle text");
    let puzzle_text_help = tr(
        lang,
        "9文字の行を9行入力してください。1〜9は初期数字、0・.・_は空欄です。",
        "Enter exactly 9 lines of 9 characters. Digits 1–9 are clues; 0, ., and _ are blank cells.",
    );
    let stale_solution_message = tr(
        lang,
        "これらの解は以前の盤面から得られたため、現在の盤面には適用できません。",
        "These solutions are from an earlier board and cannot be applied to the current board.",
    );

    rsx! {
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
            h1 { "Sudoku Solver" }
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
                                        aria_label: "{cell_aria_label(lang, y, x)}",
                                        aria_invalid: "{conflicts[y][x] || no_candidates[y][x]}",
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
                class: "buttons",
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
                button {
                    class: "left",
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
                                let conflict_count = count_conflict_cells(&board);
                                if conflict_count > 0 {
                                    msg.set(format!("{}", puzzle_loaded_message(lang, Some(conflict_count), None)));
                                    is_ok.set(false);
                                } else {
                                    let stuck_count = count_no_candidate_cells(&board);
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
                    aria_pressed: "{show_candidates()}",
                    onclick: move |_| show_candidates.set(!show_candidates()),
                    "{candidate_toggle_label}"
                }
                button {
                    disabled: true,
                    onclick: move |_| {
                        match sudoku_solver::find_hint(&board) {
                            Ok(Some(hint)) => {
                                msg.set(format_hint(hint, lang));
                                is_ok.set(true);
                                focus_cell(hint.y, hint.x);
                            }
                            Ok(None) if board.iter().flatten().all(|cell| (1..=9).contains(cell)) => {
                                msg.set(tr(lang, "問題はすでに完成しています。", "The puzzle is already complete.").to_string());
                                is_ok.set(true);
                            }
                            Ok(None) => {
                                msg.set(tr(lang, "基本的なヒントは見つかりませんでした。より高度な解法を試すか、「解く」を使ってください。", "No single-step hint found. Try a more advanced technique or Solve.").to_string());
                                is_ok.set(true);
                            }
                            Err(sudoku_solver::HintError::ConflictingValues) => {
                                msg.set(tr(lang, "ヒントを表示する前に、重複している数字を修正してください。", "Resolve conflicts before asking for a hint.").to_string());
                                is_ok.set(false);
                            }
                            Err(sudoku_solver::HintError::NoCandidates) => {
                                msg.set(tr(lang, "候補がない空欄があります。強調表示されたセルを先に修正してください。", "No candidates remain in an empty cell. Resolve the highlighted cells first.").to_string());
                                is_ok.set(false);
                            }
                        }
                    },
                    "{hint_button_label}"
                }
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

                        match create_search_worker() {
                            Ok(worker) => {
                                let mut solutions = search_solutions;
                                let mut status = search_status;
                                let mut searching = is_searching;
                                let current_job_id = search_job_id;
                                let mut error_status = search_status;
                                let mut error_searching = is_searching;
                                let started_at = search_started_at();
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
                                                message: message["message"].as_str().unwrap_or("Solution search failed.").to_string(),
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
                            search_job_id.set(search_job_id().wrapping_add(1));
                            active_search.set(None);
                            is_searching.set(false);
                            let count = search_solutions.read().len();
                            let elapsed_seconds = elapsed_seconds_since(search_started_at());
                            search_status.set(Some(SearchStatus::Cancelled { solutions: count, elapsed_seconds }));
                        },
                        "{cancel_search_label}"
                    }
                }
                button {
                    onclick: move |_| {
                        let mut solver_mtx = *mtx.read();
                        let conflict_count = count_conflict_cells(&solver_mtx);
                        let stuck_count = count_no_candidate_cells(&solver_mtx);

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
                                msg.set(match lang {
                                    Language::Japanese => format!("解けました（{ms:.0} ミリ秒）。"),
                                    Language::English => format!("Solved in {ms:.0} ms!"),
                                });
                                is_ok.set(true);
                            } else {
                                msg.set(match lang {
                                    Language::Japanese => format!("解が見つかりませんでした（{ms:.0} ミリ秒）。"),
                                    Language::English => format!("No solution found, {ms:.0} ms."),
                                });
                                is_ok.set(false);
                            }
                        }
                    },
                    "{solve_label}"
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
                button {
                    class: "right",
                    onclick: move |_| {
                        txt.set(to_txt(&mtx.read()));
                    },
                    "{save_text_label}"
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
                                        msg.set(tr(lang, "選択した解を適用しました。", "Selected solution applied.").to_string());
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
                label {
                    r#for: "puzzle-text",
                    "{puzzle_text_label}"
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

    let conflict_count = count_conflict_cells(&board);
    if conflict_count > 0 {
        msg.set(conflict_message(language, conflict_count));
        is_ok.set(false);
    } else {
        let stuck_count = count_no_candidate_cells(&board);
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
            tr(language, "変更を元に戻しました。", "Change undone."),
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
            tr(language, "変更をやり直しました。", "Change redone."),
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
    let conflict_count = count_conflict_cells(board);
    if conflict_count > 0 {
        msg.set(conflict_message(language, conflict_count));
        is_ok.set(false);
        return;
    }

    let stuck_count = count_no_candidate_cells(board);
    if stuck_count > 0 {
        msg.set(no_candidates_message(language, stuck_count));
        is_ok.set(false);
    } else {
        msg.set(success_message.to_string());
        is_ok.set(true);
    }
}

fn format_hint(hint: sudoku_solver::Hint, language: Language) -> String {
    let row = hint.y + 1;
    let column = hint.x + 1;
    match (language, hint.technique) {
        (Language::Japanese, sudoku_solver::HintTechnique::NakedSingle) => format!("ヒント: {row} 行 {column} 列には候補が1つだけです。{} を入力してください。", hint.digit),
        (Language::Japanese, sudoku_solver::HintTechnique::HiddenSingleRow) => format!("ヒント: {} 行では {} を置けるのは {} 列だけです。", row, hint.digit, column),
        (Language::Japanese, sudoku_solver::HintTechnique::HiddenSingleColumn) => format!("ヒント: {} 列では {} を置けるのは {} 行だけです。", column, hint.digit, row),
        (Language::Japanese, sudoku_solver::HintTechnique::HiddenSingleBox) => format!("ヒント: この3×3ブロックでは {} 行 {} 列だけに {} を置けます。", row, column, hint.digit),
        (Language::English, sudoku_solver::HintTechnique::NakedSingle) => format!("Hint: enter {} at row {row}, column {column}; this cell has only one candidate.", hint.digit),
        (Language::English, sudoku_solver::HintTechnique::HiddenSingleRow) => format!("Hint: enter {} at row {row}, column {column}; this digit fits only this cell in its row.", hint.digit),
        (Language::English, sudoku_solver::HintTechnique::HiddenSingleColumn) => format!("Hint: enter {} at row {row}, column {column}; this digit fits only this cell in its column.", hint.digit),
        (Language::English, sudoku_solver::HintTechnique::HiddenSingleBox) => format!("Hint: enter {} at row {row}, column {column}; this digit fits only this cell in its 3×3 box.", hint.digit),
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

fn parse_puzzle(txt: &str, language: Language) -> Result<sudoku_solver::Matrix, String> {
    let mut mtx = sudoku_solver::Matrix::default();
    let content = txt.trim_end_matches(|ch| ch == '\n' || ch == '\r');
    let lines: Vec<_> = content.lines().collect();
    if lines.len() != 9 {
        return Err(match language {
            Language::Japanese => format!("9行必要ですが、{}行あります。", lines.len()),
            Language::English => format!("Expected 9 lines, found {}.", lines.len()),
        });
    }

    for (y, line) in lines.iter().enumerate() {
        let chars: Vec<_> = line.chars().collect();
        if chars.len() != 9 {
            return Err(match language {
                Language::Japanese => format!(
                    "{} 行目は9文字必要ですが、{}文字あります。",
                    y + 1,
                    chars.len()
                ),
                Language::English => format!(
                    "Line {} must contain 9 characters, found {}.",
                    y + 1,
                    chars.len()
                ),
            });
        }
        for (x, ch) in chars.into_iter().enumerate() {
            mtx[y][x] = match ch {
                '1'..='9' => ch as u8 - b'0',
                '0' | '.' | '_' => 0,
                _ => {
                    return Err(match language {
                        Language::Japanese => format!(
                            "{} 行 {} 列に使えない文字があります。数字、0、.、_を使ってください。",
                            y + 1,
                            x + 1
                        ),
                        Language::English => format!(
                            "Unsupported character at row {}, column {}. Use digits, 0, . or _.",
                            y + 1,
                            x + 1
                        ),
                    });
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
