import init, { search } from "./sudoku_search_worker.js";

await init();

self.addEventListener("message", ({ data }) => {
    const message = typeof data === "string" ? JSON.parse(data) : data;
    if (message?.type !== "start") {
        return;
    }

    search(
        message.job_id,
        Uint8Array.from(message.board),
        message.max_solutions,
        message.max_nodes,
    );
});
