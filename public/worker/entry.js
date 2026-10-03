import init, { generate, search } from "./sudoku_search_worker.js";

let wasmReady = false;
const pendingMessages = [];

self.addEventListener("message", ({ data }) => {
    const message = typeof data === "string" ? JSON.parse(data) : data;
    if (!wasmReady) {
        pendingMessages.push(message);
        return;
    }

    dispatch(message);
});

function dispatch(message) {
    if (message?.type === "start") {
        search(
            message.job_id,
            Uint8Array.from(message.board),
            message.max_solutions,
            message.max_nodes,
        );
    } else if (message?.type === "generate") {
        generate(message.job_id, message.seed);
    }
}

await init();
wasmReady = true;
for (const message of pendingMessages) {
    dispatch(message);
}
