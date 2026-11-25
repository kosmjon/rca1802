//! Command-line front end for the CDP1802 disassembler.

use std::path::PathBuf;
use std::process::ExitCode;

use dis1802::{disassemble, Options, Program};

const USAGE: &str = "\
usage: dis1802 [options] <input>

<input> is an Intel HEX file (lines starting with ':') or a raw binary.

options:
  --org ADDR        load address for a raw binary (hex; default 0000).
                    Ignored for Intel HEX, which carries its own addresses.
  --entry ADDR      extra entry point to trace from (hex). May repeat.
  --no-scrt         do not treat `SEP R4` + inline `DW` as a call; disassemble
                    those bytes as ordinary code instead.
  --call-reg N      program-counter register for SCRT calls (default 4).
  --return-reg N    return register for SCRT (default 5).
  -o FILE           write the listing to FILE (default: standard output).
  -h, --help        show this help.
";

struct Args {
    input: PathBuf,
    org: u16,
    output: Option<PathBuf>,
    opts: Options,
}

fn parse_hex16(s: &str, what: &str) -> Result<u16, String> {
    let t = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    let t = t.strip_suffix(['h', 'H']).unwrap_or(t);
    u16::from_str_radix(t, 16).map_err(|_| format!("{what} `{s}` is not a hex address"))
}

fn parse_args() -> Result<Args, String> {
    let mut input = None;
    let mut org = 0u16;
    let mut output = None;
    let mut opts = Options::default();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--org" => org = parse_hex16(&it.next().ok_or("--org needs an address")?, "--org")?,
            "--entry" => {
                let v = parse_hex16(&it.next().ok_or("--entry needs an address")?, "--entry")?;
                opts.entries.push(v);
            }
            "--no-scrt" => opts.scrt = false,
            "--call-reg" => {
                opts.call_reg = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .filter(|&r| r < 16)
                    .ok_or("--call-reg needs a register number 0-15")?;
            }
            "--return-reg" => {
                opts.return_reg = it
                    .next()
                    .and_then(|n| n.parse().ok())
                    .filter(|&r| r < 16)
                    .ok_or("--return-reg needs a register number 0-15")?;
            }
            "-o" => output = Some(PathBuf::from(it.next().ok_or("-o needs a file name")?)),
            _ if a.starts_with('-') && a.len() > 1 => return Err(format!("unknown option {a}")),
            _ if input.is_none() => input = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}")),
        }
    }
    Ok(Args {
        input: input.ok_or("no input file given")?,
        org,
        output,
        opts,
    })
}

/// Load the input as a single image. Intel HEX (detected by a leading ':') is
/// parsed and its lowest address used as the org; a raw binary uses `--org`.
/// A HEX file with gaps is flattened (gaps filled with 00) between its lowest
/// and highest addresses, so the listing is one contiguous block.
fn load_image(args: &Args) -> Result<Program, String> {
    let raw = std::fs::read(&args.input).map_err(|e| format!("{}: {e}", args.input.display()))?;
    let looks_hex = raw
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|&b| b == b':');

    if looks_hex {
        let text = String::from_utf8_lossy(&raw);
        let chunks = elf2::hexfile::parse(&text).map_err(|e| e.to_string())?;
        if chunks.is_empty() {
            return Err("the HEX file contains no data".into());
        }
        let lo = chunks.iter().map(|c| c.addr).min().unwrap();
        let hi = chunks
            .iter()
            .map(|c| u32::from(c.addr) + c.data.len() as u32)
            .max()
            .unwrap();
        let mut bytes = vec![0u8; (hi - u32::from(lo)) as usize];
        for c in &chunks {
            let off = usize::from(c.addr) - usize::from(lo);
            bytes[off..off + c.data.len()].copy_from_slice(&c.data);
        }
        Ok(Program::new(lo, bytes))
    } else {
        Ok(Program::new(args.org, raw))
    }
}

fn run(args: &Args) -> Result<(), String> {
    let prog = load_image(args)?;
    let listing = disassemble(&prog, &args.opts);
    let text = listing.to_text();
    match &args.output {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("dis1802: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dis1802: {e}");
            ExitCode::FAILURE
        }
    }
}
