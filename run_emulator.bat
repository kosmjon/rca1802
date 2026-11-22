@echo off
rem Builds (if wasm-pack is available) and serves the Elf II web front end,
rem then opens it in the default browser. See emulator\web\README.md for
rem how to use the page once it's open (load a .hex/.bin, front panel,
rem VID-1 terminal, debugger).
setlocal

cd /d "%~dp0emulator"

where wasm-pack >nul 2>nul
if %errorlevel%==0 (
    echo Building WebAssembly module...
    wasm-pack build crates\elf2-wasm --target web --release --out-dir ..\web\pkg
    if errorlevel 1 (
        echo wasm-pack build failed; serving the existing build in web\pkg instead.
    )
) else (
    echo wasm-pack not found on PATH; serving the existing build in web\pkg.
)

echo Starting local web server on http://127.0.0.1:1802/ ...
start "Elf II emulator server (close this window to stop)" cmd /k python -m http.server 1802 --bind 127.0.0.1 --directory web

timeout /t 2 /nobreak >nul
start "" http://127.0.0.1:1802/

endlocal
