use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::DedicatedWorkerGlobalScope;

use sudoku_solver::{Matrix, SearchEvent, SolutionSearch};

fn main() {}

#[wasm_bindgen]
pub fn search(job_id: u32, flat_board: Vec<u8>, max_solutions: u32, max_nodes: u32) {
    if flat_board.len() != 81 || max_solutions == 0 || max_nodes == 0 {
        post_message(serde_json::json!({
            "type": "error",
            "job_id": job_id,
            "message": "Expected 81 cell values and non-zero UI search limits.",
        }));
        return;
    }

    let board: Matrix = std::array::from_fn(|y| std::array::from_fn(|x| flat_board[y * 9 + x]));
    let mut search = match SolutionSearch::new(&board) {
        Ok(search) => search,
        Err(error) => {
            post_message(serde_json::json!({
            "type": "error",
            "job_id": job_id,
            "message": match error {
                sudoku_solver::SearchError::ConflictingValues => "Conflicting values.",
            },
            }));
            return;
        }
    };

    let mut termination = "exhausted";
    loop {
        if search.explored_nodes() >= u64::from(max_nodes) {
            termination = "node_limit";
            break;
        }

        let Some(event) = search.next() else {
            break;
        };

        match event {
            SearchEvent::SolutionFound { index, board } => {
                post_message(serde_json::json!({
                    "type": "solution",
                    "job_id": job_id,
                    "index": index,
                    "board": board,
                }));
                if index >= max_solutions as usize {
                    termination = "solution_limit";
                    break;
                }
            }
            SearchEvent::Progress {
                nodes,
                solutions_found,
            } => post_message(serde_json::json!({
                "type": "progress",
                "job_id": job_id,
                "nodes": nodes,
                "solutions_found": solutions_found,
            })),
        }
    }

    post_message(serde_json::json!({
        "type": "finished",
        "job_id": job_id,
        "solutions_found": search.solutions_found(),
        "explored_nodes": search.explored_nodes(),
        "termination": termination,
    }));
}

fn post_message(message: serde_json::Value) {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let _ = scope.post_message(&JsValue::from_str(&message.to_string()));
}
