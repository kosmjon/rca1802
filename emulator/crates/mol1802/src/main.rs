//! Command-line front end for the MOL1802 transpiler (`design.md`,
//! section "8. CLI").

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: mol1802 [options] <source.m18>

options:
  -o <file>            transpiled asm1802 output path (default: <source>
                       with .asm)
  --assemble           also assemble the transpiled output with asm1802
  --assemble-out <f>   assembled output path (default: the transpiled
                       .asm path with .hex, or .bin with --bin)
  --bin                assemble to a flat binary instead of Intel HEX
  -l, --listing        with --assemble, also write a listing (default:
                       <assemble-out> with .lst)
  --max-errors <n>     errors to print before stopping (default 50, 0 = all)
  -h, --help           show this help
";

struct Args {
    source: PathBuf,
    output: Option<PathBuf>,
    assemble: bool,
    assemble_out: Option<PathBuf>,
    bin: bool,
    listing: bool,
    max_errors: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut source = None;
    let mut output = None;
    let mut assemble = false;
    let mut assemble_out = None;
    let mut bin = false;
    let mut listing = false;
    let mut max_errors = 50;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-o" => output = Some(PathBuf::from(it.next().ok_or("-o needs a file name")?)),
            "--assemble" => assemble = true,
            "--assemble-out" => {
                assemble_out = Some(PathBuf::from(
                    it.next().ok_or("--assemble-out needs a file name")?,
                ));
            }
            "--bin" => bin = true,
            "-l" | "--listing" => listing = true,
            "--max-errors" => {
                max_errors = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("--max-errors needs a number")?;
            }
            _ if a.starts_with('-') && a.len() > 1 => return Err(format!("unknown option {a}")),
            _ if source.is_none() => source = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}")),
        }
    }
    let source = source.ok_or("no source file given")?;
    Ok(Args {
        source,
        output,
        assemble,
        assemble_out,
        bin,
        listing,
        max_errors,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mol1802: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("mol1802: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<bool, String> {
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| args.source.with_extension("asm"));

    let generated =
        match transpile_tree(&args.source, &output_path, &mut Vec::new(), args.max_errors)? {
            Some(text) => text,
            None => return Ok(false),
        };

    if !args.assemble {
        return Ok(true);
    }

    // Reuse `asm1802::assemble_with` directly on the in-memory generated
    // text (design.md section 8), with a loader that resolves any
    // pass-through `INCLUDE` relative to the written `.asm` file's own
    // folder -- the same rule `asm1802::assemble_file` itself follows.
    let name = output_path.display().to_string();
    let asm = asm1802::assemble_with(&name, &generated, &[], &mut |req, from| {
        let req_path = Path::new(req);
        let candidate = if req_path.is_absolute() {
            req_path.to_path_buf()
        } else {
            Path::new(from)
                .parent()
                .unwrap_or(Path::new(""))
                .join(req_path)
        };
        std::fs::read(&candidate)
            .map(|b| (candidate.display().to_string(), asm1802::decode_source(b)))
            .map_err(|_| format!("can't find `{req}` (tried {})", candidate.display()))
    });

    if !asm.ok() {
        report_diagnostics(&asm.errors, &name, args.max_errors);
        return Ok(false);
    }

    let assemble_out = args
        .assemble_out
        .clone()
        .unwrap_or_else(|| output_path.with_extension(if args.bin { "bin" } else { "hex" }));

    if args.listing {
        write(
            &assemble_out.with_extension("lst"),
            asm.listing_text().as_bytes(),
        )?;
    }

    let Some((lo, hi)) = asm.range() else {
        eprintln!("mol1802: warning: the program contains no bytes");
        return Ok(true);
    };
    if args.bin {
        let (start, data) = asm.to_bin().expect("range is non-empty");
        write(&assemble_out, &data)?;
        println!(
            "{}: {} bytes, load at {start:04X} (ends {hi:04X})",
            assemble_out.display(),
            data.len()
        );
    } else {
        write(&assemble_out, asm.to_hex().as_bytes())?;
        let total: usize = asm.segments().iter().map(|(_, d)| d.len()).sum();
        println!(
            "{}: {total} bytes in {lo:04X}-{hi:04X}",
            assemble_out.display()
        );
    }
    Ok(true)
}

/// The deepest `.m18` include nesting allowed, matching `asm1802`'s limit.
const MAX_INCLUDE_DEPTH: usize = 16;

/// Transpiles `source` to `output`, first doing the same for every
/// `INCLUDE "x.m18"` it names (each is written to a sibling `x.asm`, and the
/// include line in `output` is rewritten to name it). Each file is
/// transpiled on its own: scratch-register (`R6` / `MOL1802_SCRATCH`) and
/// `SEX` state are not shared or checked across files. Nothing is written
/// for a file until all of its includes succeeded. Returns the generated
/// text, or `None` once diagnostics have been printed.
/// `stack` holds the canonical paths of the files currently being
/// transpiled, to catch include cycles.
fn transpile_tree(
    source: &Path,
    output: &Path,
    stack: &mut Vec<PathBuf>,
    max_errors: usize,
) -> Result<Option<String>, String> {
    let canonical =
        std::fs::canonicalize(source).map_err(|e| format!("{}: {e}", source.display()))?;
    if stack.contains(&canonical) {
        return Err(format!(
            "{}: INCLUDE cycle (already being transpiled)",
            source.display()
        ));
    }
    if stack.len() >= MAX_INCLUDE_DEPTH {
        return Err(format!(
            "{}: INCLUDE nesting deeper than {MAX_INCLUDE_DEPTH}",
            source.display()
        ));
    }
    let source_text = std::fs::read(source)
        .map(asm1802::decode_source)
        .map_err(|e| format!("{}: {e}", source.display()))?;

    let generated = match mol1802::transpile(&source_text) {
        Ok(text) => text,
        Err(diags) => {
            report_diagnostics(&diags, &source.to_string_lossy(), max_errors);
            return Ok(None);
        }
    };

    let (generated, targets) = mol1802::rewrite_m18_includes(&generated);
    stack.push(canonical);
    for target in &targets {
        let target_path = Path::new(target);
        let child_source = if target_path.is_absolute() {
            target_path.to_path_buf()
        } else {
            source.parent().unwrap_or(Path::new("")).join(target_path)
        };
        let child_output = child_source.with_extension("asm");
        if transpile_tree(&child_source, &child_output, stack, max_errors)?.is_none() {
            stack.pop();
            return Ok(None);
        }
    }
    stack.pop();

    write(output, generated.as_bytes())?;
    println!("{}: transpiled", output.display());
    Ok(Some(generated))
}

/// Prints each diagnostic as `file:line: error: message`, the same shape
/// `asm1802`'s own CLI uses, regardless of which stage (transpile or
/// assemble) produced it. `d.file` is used when set (real `asm1802`
/// diagnostics already carry the right name); `fallback_file` covers
/// `mol1802::transpile` diagnostics, which leave it empty.
fn report_diagnostics(diags: &[asm1802::Diagnostic], fallback_file: &str, max_errors: usize) {
    let shown = if max_errors == 0 {
        diags.len()
    } else {
        max_errors.min(diags.len())
    };
    for d in &diags[..shown] {
        let file = if d.file.is_empty() {
            fallback_file
        } else {
            d.file.as_str()
        };
        eprintln!("{file}:{}: error: {}", d.line, d.message);
    }
    if shown < diags.len() {
        eprintln!("... and {} more", diags.len() - shown);
    }
    eprintln!("{} error(s); no output written", diags.len());
}

fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}
