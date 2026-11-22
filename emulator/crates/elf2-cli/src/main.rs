//! elf2-cli: run the Netronics Elf II emulator from a shell.
//!
//! The VID-1 terminal is connected to stdin/stdout: characters the Elf II sends on
//! the serial port are written to stdout; text read from stdin is typed into the
//! terminal (with the configured paste pacing; LF becomes CR).
//!
//! Run `elf2-cli --help` for options.

use elf2::{Config, Event, Machine, Switch};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const HELP: &str = "\
elf2-cli - Netronics Elf II emulator

USAGE:
  elf2-cli [OPTIONS]

PROGRAMS AND ROMS:
  --config FILE        machine configuration (JSON); defaults otherwise
  --rom FILE@ADDR      load a ROM image (.bin at ADDR, or .hex with its own addresses)
  --load FILE[@ADDR]   load a program into RAM (.bin needs @ADDR; .hex does not)
  --boot-f000          put C0 F0 00 (LBR F000) at 0000, as you would key it in to
                       start the Giant Board monitor
  ADDR is hex, e.g. F000 or 0x0100.

SERIAL / VID-1 (override the configuration):
  --baud N             baud rate
  --rx-ef 3|4          EF line for serial receive
  --tx-inverted        invert Q as the transmit line
  --rx-inverted        invert the receive EF line
  --columns 32|64      VID-1 characters per line

RUNNING:
  --cycles N           stop after N machine cycles (default: run until Ctrl-C,
                       or until --exit-after-input)
  --exit-after-input MS  after stdin ends and all input has been sent, run MS more
                       milliseconds of emulated time, then stop
  --fast               do not pace to real time
  --show-display       print hex display changes to stderr
  --show-q             print Q changes to stderr
  --screen             print the VID-1 screen to stderr when stopping
  --dump START-END     print memory START..END (inclusive, hex) to stderr when stopping
  --disasm START-END   print a disassembly of START..END to stderr when stopping
  --no-stdin           do not read stdin
  -h, --help           this text
";

#[derive(Debug, Default, PartialEq)]
struct Options {
    config: Option<String>,
    roms: Vec<(String, Option<u16>)>,
    loads: Vec<(String, Option<u16>)>,
    boot_f000: bool,
    baud: Option<u32>,
    rx_ef: Option<u8>,
    tx_inverted: bool,
    rx_inverted: bool,
    columns: Option<u8>,
    cycles: Option<u64>,
    exit_after_input_ms: Option<u64>,
    fast: bool,
    show_display: bool,
    show_q: bool,
    screen: bool,
    dump: Option<(u16, u16)>,
    disasm: Option<(u16, u16)>,
    no_stdin: bool,
    help: bool,
}

fn parse_hex16(s: &str) -> Result<u16, String> {
    let t = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(t, 16).map_err(|_| format!("'{s}' is not a hex address"))
}

fn parse_file_addr(s: &str) -> Result<(String, Option<u16>), String> {
    match s.rsplit_once('@') {
        Some((f, a)) => Ok((f.to_string(), Some(parse_hex16(a)?))),
        None => Ok((s.to_string(), None)),
    }
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match a.as_str() {
            "-h" | "--help" => o.help = true,
            "--config" => o.config = Some(val(a)?),
            "--rom" => o.roms.push(parse_file_addr(&val(a)?)?),
            "--load" => o.loads.push(parse_file_addr(&val(a)?)?),
            "--boot-f000" => o.boot_f000 = true,
            "--baud" => o.baud = Some(val(a)?.parse().map_err(|_| "--baud: not a number")?),
            "--rx-ef" => o.rx_ef = Some(val(a)?.parse().map_err(|_| "--rx-ef: 3 or 4")?),
            "--tx-inverted" => o.tx_inverted = true,
            "--rx-inverted" => o.rx_inverted = true,
            "--columns" => o.columns = Some(val(a)?.parse().map_err(|_| "--columns: 32 or 64")?),
            "--cycles" => o.cycles = Some(val(a)?.parse().map_err(|_| "--cycles: not a number")?),
            "--exit-after-input" => {
                o.exit_after_input_ms = Some(
                    val(a)?
                        .parse()
                        .map_err(|_| "--exit-after-input: milliseconds")?,
                )
            }
            "--fast" => o.fast = true,
            "--show-display" => o.show_display = true,
            "--show-q" => o.show_q = true,
            "--screen" => o.screen = true,
            "--no-stdin" => o.no_stdin = true,
            "--dump" | "--disasm" => {
                let v = val(a)?;
                let (s, e) = v
                    .split_once('-')
                    .ok_or_else(|| format!("{a}: '{v}' is not START-END"))?;
                let r = Some((parse_hex16(s)?, parse_hex16(e)?));
                if a == "--dump" {
                    o.dump = r;
                } else {
                    o.disasm = r;
                }
            }
            other => return Err(format!("unknown option '{other}' (try --help)")),
        }
    }
    Ok(o)
}

fn build_config(o: &Options) -> Result<Config, String> {
    let mut cfg = match &o.config {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            Config::from_json(&text).map_err(|e| format!("{path}: {e}"))?
        }
        None => Config::default(),
    };
    if let Some(s) = cfg.serial.as_mut() {
        if let Some(b) = o.baud {
            s.baud = b;
        }
        if let Some(e) = o.rx_ef {
            s.rx_ef = e;
        }
        s.tx_inverted |= o.tx_inverted;
        s.rx_inverted |= o.rx_inverted;
    }
    if let Some(c) = o.columns {
        cfg.vid1.columns = c;
    }
    cfg.validate().map_err(|e| e.to_string())?;
    Ok(cfg)
}

fn is_hex_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".hex")
}

/// Read a file as (base address, bytes). Intel HEX must be one contiguous block.
fn read_image(path: &str, addr: Option<u16>) -> Result<(u16, Vec<u8>), String> {
    if is_hex_file(path) {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let chunks = elf2::hexfile::parse(&text).map_err(|e| format!("{path}: {e}"))?;
        let mut chunks = chunks;
        chunks.sort_by_key(|c| c.addr);
        let base = chunks
            .first()
            .map(|c| c.addr)
            .ok_or(format!("{path}: no data"))?;
        let mut bytes = Vec::new();
        for c in chunks {
            if usize::from(c.addr) != usize::from(base) + bytes.len() {
                return Err(format!("{path}: data is not one contiguous block"));
            }
            bytes.extend_from_slice(&c.data);
        }
        Ok((addr.unwrap_or(base), bytes))
    } else {
        let base = addr.ok_or(format!("{path}: binary files need @ADDR"))?;
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        Ok((base, bytes))
    }
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

fn setup(o: &Options) -> Result<Machine, String> {
    let mut m = Machine::new(build_config(o)?).map_err(|e| e.to_string())?;
    for (path, addr) in &o.roms {
        let (base, bytes) = read_image(path, *addr)?;
        m.load_rom(&file_stem(path), base, &bytes)
            .map_err(|e| format!("{path}: {e}"))?;
        eprintln!(
            "ROM {} at {base:04X} ({} bytes)",
            file_stem(path),
            bytes.len()
        );
    }
    for missing in m.missing_roms() {
        eprintln!(
            "warning: ROM '{}' (SHA-256 {}) listed in the configuration was not loaded",
            missing.name, missing.sha256
        );
    }
    for (path, addr) in &o.loads {
        if is_hex_file(path) && addr.is_none() {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let (low, n) = m.load_hex(&text).map_err(|e| format!("{path}: {e}"))?;
            eprintln!("loaded {path}: {n} bytes from {low:04X}");
        } else {
            let (base, bytes) = read_image(path, *addr)?;
            m.load_bin(base, &bytes)
                .map_err(|e| format!("{path}: {e}"))?;
            eprintln!("loaded {path}: {} bytes at {base:04X}", bytes.len());
        }
    }
    if o.boot_f000 {
        m.load_bin(0x0000, &[0xC0, 0xF0, 0x00])
            .map_err(|e| format!("--boot-f000: {e}"))?;
    }
    Ok(m)
}

/// Start a thread reading stdin; chunks of text arrive on the channel, `None` at EOF.
fn stdin_reader() -> mpsc::Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = std::io::stdin();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx.send(Some(s)).is_err() {
                        return;
                    }
                }
            }
        }
    });
    rx
}

fn run(o: &Options) -> Result<(), String> {
    let mut m = setup(o)?;
    let cps = u64::from(m.config().cycles_per_second());
    let slice = cps / 100; // 10 ms of emulated time per pass
    let input = (!o.no_stdin).then(stdin_reader);
    let mut input_done = o.no_stdin;
    let mut drain_until: Option<u64> = None;
    let mut out = std::io::stdout();
    let start = Instant::now();
    let mut total: u64 = 0;
    let mut framing_errors = 0u64;

    m.set_switch(Switch::Run, true);
    loop {
        if let Some(rx) = &input {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Some(text) => m.term_paste(&text),
                    None => input_done = true,
                }
            }
        }
        if let Some(limit) = o.cycles {
            if total >= limit {
                break;
            }
        }
        if let Some(ms) = o.exit_after_input_ms {
            if input_done && m.term_pending() == 0 && drain_until.is_none() {
                drain_until = Some(total + ms * cps / 1000);
            }
            if drain_until.is_some_and(|d| total >= d) {
                break;
            }
        }

        let budget = o.cycles.map_or(slice, |l| slice.min(l - total));
        let r = m.run(budget.max(1));
        total += r.cycles;
        let mut wrote = false;
        for e in m.events() {
            match *e {
                Event::TermChar(c) => {
                    let _ = out.write_all(&[c]);
                    wrote = true;
                }
                Event::TermFramingError => framing_errors += 1,
                Event::HexDisplay(v) if o.show_display => eprintln!("[display {v:02X}]"),
                Event::Q(q) if o.show_q => eprintln!("[Q {}]", u8::from(q)),
                Event::Halted => eprintln!("[IDL at {:04X}]", m.cpu().r[m.cpu().p as usize]),
                _ => {}
            }
        }
        if wrote {
            let _ = out.flush();
        }
        if !o.fast {
            let target = Duration::from_secs_f64(total as f64 / cps as f64);
            let elapsed = start.elapsed();
            if target > elapsed {
                std::thread::sleep(target - elapsed);
            }
        }
    }
    let _ = out.flush();

    if framing_errors > 0 {
        eprintln!("[{framing_errors} serial framing errors: check --baud and polarity options]");
    }
    if o.screen {
        eprintln!("--- VID-1 screen ---");
        for line in m.vid1_text() {
            eprintln!("|{line}");
        }
    }
    if let Some((s, e)) = o.disasm {
        let bytes = m.save_bin(s, e);
        let mut k = 0usize;
        while k < bytes.len() {
            let ins = elf2::cdp1802::disasm(&bytes[k..], s.wrapping_add(k as u16));
            let raw: Vec<String> = bytes[k..(k + ins.len as usize).min(bytes.len())]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect();
            eprintln!("{:04X}  {:<9} {ins}", ins.addr, raw.join(" "));
            k += ins.len as usize;
        }
    }
    if let Some((s, e)) = o.dump {
        let bytes = m.save_bin(s, e);
        for (k, row) in bytes.chunks(16).enumerate() {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
            eprintln!("{:04X}: {}", usize::from(s) + k * 16, hex.join(" "));
        }
    }
    let c = m.cpu();
    eprintln!(
        "[stopped after {total} machine cycles ({:.2} s emulated); P={:X} R(P)={:04X} D={:02X} Q={}]",
        total as f64 / cps as f64,
        c.p,
        c.r[c.p as usize],
        c.d,
        u8::from(m.q())
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("elf2-cli: {e}");
            std::process::exit(2);
        }
    };
    if o.help {
        print!("{HELP}");
        return;
    }
    if let Err(e) = run(&o) {
        eprintln!("elf2-cli: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_options() {
        let o = parse_args(&args(
            "--rom mon.hex --rom a.bin@F000 --load p.bin@0x0100 --boot-f000 --baud 110 \
             --rx-ef 4 --tx-inverted --cycles 1000 --fast --dump 0000-00FF --screen",
        ))
        .unwrap();
        assert_eq!(
            o.roms,
            [("mon.hex".into(), None), ("a.bin".into(), Some(0xF000))]
        );
        assert_eq!(o.loads, [("p.bin".into(), Some(0x0100))]);
        assert!(o.boot_f000 && o.tx_inverted && o.fast && o.screen);
        assert_eq!(
            (o.baud, o.rx_ef, o.cycles),
            (Some(110), Some(4), Some(1000))
        );
        assert_eq!(o.dump, Some((0x0000, 0x00FF)));
    }

    #[test]
    fn rejects_bad_options() {
        assert!(parse_args(&args("--bogus")).is_err());
        assert!(parse_args(&args("--rom")).is_err());
        assert!(parse_args(&args("--load x.bin@ZZZZ")).is_err());
        assert!(parse_args(&args("--dump 0000")).is_err());
    }

    #[test]
    fn config_overrides_are_validated() {
        let o = parse_args(&args("--rx-ef 2")).unwrap();
        assert!(build_config(&o).is_err());
        let o = parse_args(&args("--baud 110 --columns 32")).unwrap();
        let c = build_config(&o).unwrap();
        assert_eq!(c.serial.unwrap().baud, 110);
        assert_eq!(c.vid1.columns, 32);
    }
}
