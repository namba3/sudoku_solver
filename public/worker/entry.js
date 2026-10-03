import init, { search } from "./sudoku_search_worker.js";

let wasmReady = false;
const pendingMessages = [];

self.addEventListener("message", ({ data }) => {
    const message = typeof data === "string" ? JSON.parse(data) : data;
    if (message?.type !== "start") {
        return;
    }

    if (!wasmReady) {
        pendingMessages.push(message);
        return;
    }

    search(
        message.job_id,
        Uint8Array.from(message.board),
        message.max_solutions,
        message.max_nodes,
    );
});

await init();
wasmReady = true;
for (const message of pendingMessages) {
    search(
        message.job_id,
        Uint8Array.from(message.board),
        message.max_solutions,
        message.max_nodes,
    );
}
