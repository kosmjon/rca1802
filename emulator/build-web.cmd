@echo off
rem Build the WebAssembly front end into web\pkg (where the page loads it from).
rem Pass extra args through, e.g. build-web.cmd --dev
setlocal
cd /d "%~dp0"
wasm-pack build crates\elf2-wasm --target web --release --out-dir ..\..\web\pkg %*
rem Stamp a fresh cache-busting version so a rebuild is never hidden by a cache.
python web\stamp-version.py
endlocal
