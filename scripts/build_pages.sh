#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build --release --target wasm32-unknown-unknown --features search-worker --bin sudoku_search_worker
rm -rf public/worker/generated
wasm-bindgen \
    --target web \
    --no-typescript \
    --out-dir public/worker \
    --out-name sudoku_search_worker \
    target/wasm32-unknown-unknown/release/sudoku_search_worker.wasm
dx bundle --platform web --release --out-dir docs

python3 - "$repo_root" <<'PY'
from pathlib import Path
import re
import shutil
import sys

root = Path(sys.argv[1])
docs = root / "docs"
generated = docs / "public"
index = (generated / "index.html").read_text()
js_name = re.search(r'/sudoku_solver/assets/([^" ]+\.js)', index).group(1)
js_text = (generated / "assets" / js_name).read_text()
wasm_name = re.search(r'/sudoku_solver/assets/([^" ]+\.wasm)', js_text).group(1)

favicon_link = '<link rel="icon" type="image/svg+xml" href="/sudoku_solver/favicon.svg">'
if 'rel="icon"' not in index:
    index = index.replace("</head>", f"{favicon_link}\n</head>")

assets = docs / "assets"
assets.mkdir(exist_ok=True)
for name in (js_name, wasm_name):
    shutil.copy2(generated / "assets" / name, assets / name)
(docs / "index.html").write_text(index)
shutil.copy2(generated / "app.css", docs / "app.css")
shutil.copy2(root / "public" / "favicon.svg", docs / "favicon.svg")

worker_source = generated / "worker"
worker_target = docs / "worker"
if worker_target.exists():
    shutil.rmtree(worker_target)
shutil.copytree(worker_source, worker_target)
stale_generated_worker = worker_target / "generated"
if stale_generated_worker.exists():
    shutil.rmtree(stale_generated_worker)

for path in assets.iterdir():
    if path.is_file() and path.name.startswith("sudoku_solver") and path.name not in (js_name, wasm_name):
        path.unlink()
shutil.rmtree(generated)

assert (assets / js_name).is_file()
assert (assets / wasm_name).is_file()
assert (worker_target / "entry.js").is_file()
assert (worker_target / "sudoku_search_worker.js").is_file()
assert (worker_target / "sudoku_search_worker_bg.wasm").is_file()
assert not (worker_target / "generated").exists()
assert "/sudoku_solver/" in index
print(f"Pages bundle updated: {js_name}, {wasm_name}, worker entry and WASM")
PY
