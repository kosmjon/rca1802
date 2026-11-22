//! Command-line front end for the asm1802 cross-assembler.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
usage: asm1802 [options] <source.asm>

options:
  -o <file>          output file (default: source name with .hex, or .bin with --bin)
  -I <dir>           also look for INCLUDE files in <dir> (after the including
                     file's own folder); may be repeated
  -D <name>          define <name> for IFDEF/IFNDEF from the start; may be
                     repeated
  --hex              write Intel HEX (default; also chosen by a .hex/.ihx -o name)
  --bin              write a flat binary from the lowest to the highest address,
                     gaps filled with 00 (also chosen by a .bin -o name)
  -l, --listing      write a listing with a symbol table to <source>.lst
  --listing=<file>   write the listing to <file>
  --max-errors <n>   errors to print before stopping (default 50, 0 = all)
  -h, --help         show this help
";

struct Args {
    source: PathBuf,
    output: Option<PathBuf>,
    bin: Option<bool>,
    listing: Option<PathBuf>,
    include_dirs: Vec<PathBuf>,
    defines: Vec<String>,
    max_errors: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut source = None;
    let mut output = None;
    let mut bin = None;
    let mut listing = None;
    let mut want_listing = false;
    let mut max_errors = 50;
    let mut include_dirs = Vec::new();
    let mut defines = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-o" => output = Some(PathBuf::from(it.next().ok_or("-o needs a file name")?)),
            "-I" => include_dirs.push(PathBuf::from(it.next().ok_or("-I needs a folder")?)),
            _ if a.starts_with("-I") && a.len() > 2 => include_dirs.push(PathBuf::from(&a[2..])),
            "-D" => defines.push(it.next().ok_or("-D needs a name")?),
            _ if a.starts_with("-D") && a.len() > 2 => defines.push(a[2..].to_string()),
            "--hex" => bin = Some(false),
            "--bin" => bin = Some(true),
            "-l" | "--listing" => want_listing = true,
            "--max-errors" => {
                max_errors = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or("--max-errors needs a number")?;
            }
            _ if a.starts_with("--listing=") => {
                listing = Some(PathBuf::from(&a["--listing=".len()..]));
            }
            _ if a.starts_with('-') && a.len() > 1 => return Err(format!("unknown option {a}")),
            _ if source.is_none() => source = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}")),
        }
    }
    let source = source.ok_or("no source file given")?;
    if want_listing && listing.is_none() {
        listing = Some(source.with_extension("lst"));
    }
    Ok(Args {
        source,
        output,
        bin,
        listing,
        include_dirs,
        defines,
        max_errors,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("asm1802: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("asm1802: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<bool, String> {
    let defines: Vec<&str> = args.defines.iter().map(String::as_str).collect();
    let asm = asm1802::assemble_file(&args.source, &args.include_dirs, &defines)?;

    if let Some(path) = &args.listing {
        write(path, asm.listing_text().as_bytes())?;
    }

    if !asm.ok() {
        let shown = if args.max_errors == 0 {
            asm.errors.len()
        } else {
            args.max_errors.min(asm.errors.len())
        };
        for d in &asm.errors[..shown] {
            eprintln!("{}:{}: error: {}", d.file, d.line, d.message);
            if let Some(l) = asm
                .listing
                .iter()
                .find(|l| l.line == d.line && l.file == d.file)
            {
                eprintln!("    {}", l.source);
            }
        }
        if shown < asm.errors.len() {
            eprintln!("... and {} more", asm.errors.len() - shown);
        }
        eprintln!("{} error(s); no output written", asm.errors.len());
        return Ok(false);
    }

    let bin = args.bin.unwrap_or_else(|| {
        args.output
            .as_deref()
            .and_then(Path::extension)
            .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    });
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| args.source.with_extension(if bin { "bin" } else { "hex" }));

    let Some((lo, hi)) = asm.range() else {
        eprintln!("asm1802: warning: the program contains no bytes");
        return Ok(true);
    };
    if bin {
        let (start, data) = asm.to_bin().expect("range is non-empty");
        write(&output, &data)?;
        println!(
            "{}: {} bytes, load at {start:04X} (ends {hi:04X})",
            output.display(),
            data.len()
        );
    } else {
        write(&output, asm.to_hex().as_bytes())?;
        let total: usize = asm.segments().iter().map(|(_, d)| d.len()).sum();
        println!("{}: {total} bytes in {lo:04X}-{hi:04X}", output.display());
    }
    Ok(true)
}

fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
}
