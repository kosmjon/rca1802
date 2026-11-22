//! VID-1 terminal tests (task 9).

use crate::vid1::{Vid1Terminal, ROWS};
use crate::*;
use proptest::prelude::*;

fn term() -> Vid1Terminal {
    Vid1Terminal::new(&Vid1Config::default())
}

fn put_str(t: &mut Vid1Terminal, s: &str) {
    for b in s.bytes() {
        t.put(b);
    }
}

#[test]
fn printable_characters_are_drawn_and_advance_the_cursor() {
    let mut t = term();
    put_str(&mut t, "HELLO world ~!");
    assert_eq!(t.text()[0], "HELLO world ~!");
    assert_eq!(t.cursor(), (0, 14));
    assert_eq!(t.columns(), 64);
    assert_eq!(t.screen().len(), 1024, "1 KB of screen memory");
}

#[test]
fn carriage_return_and_line_feed() {
    let mut t = term();
    put_str(&mut t, "abc");
    t.put(0x0D);
    assert_eq!(t.cursor(), (0, 0));
    put_str(&mut t, "X");
    assert_eq!(t.text()[0], "Xbc", "CR alone overwrites the line");
    t.put(0x0A);
    assert_eq!(
        t.cursor(),
        (1, 1),
        "LF alone moves down, keeping the column"
    );
    put_str(&mut t, "Y");
    assert_eq!(t.text()[1], " Y");
}

#[test]
fn scrolls_up_on_line_feed_at_the_bottom() {
    let mut t = term();
    for n in 0..ROWS {
        put_str(&mut t, &format!("line {n}"));
        t.put(0x0D);
        if n + 1 < ROWS {
            t.put(0x0A);
        }
    }
    assert_eq!(t.cursor().0, ROWS - 1);
    assert_eq!(t.text()[0], "line 0");
    assert_eq!(t.text()[15], "line 15");
    t.put(0x0A); // bottom line: scroll
    assert_eq!(t.cursor().0, ROWS - 1);
    let text = t.text();
    assert_eq!(text[0], "line 1");
    assert_eq!(text[14], "line 15");
    assert_eq!(text[15], "", "the new bottom line is blank");
}

#[test]
fn backspace_moves_left_without_erasing_and_stops_at_column_one() {
    let mut t = term();
    put_str(&mut t, "ab");
    t.put(0x08);
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(t.text()[0], "ab", "BS only moves the cursor");
    t.put(0x08);
    t.put(0x08);
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn form_feed_clears_and_homes() {
    let mut t = term();
    put_str(&mut t, "junk");
    t.put(0x0A);
    put_str(&mut t, "more");
    t.put(0x0C);
    assert!(t.screen().iter().all(|&b| b == b' '));
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn delete_erases_at_cursor_by_default() {
    let mut t = term();
    put_str(&mut t, "abc");
    t.put(0x08);
    t.put(0x7F);
    assert_eq!(t.text()[0], "ab");
    assert_eq!(t.cursor(), (0, 2), "the cursor does not move");
}

#[test]
fn long_lines_wrap_and_scroll() {
    let mut t = term();
    let long: String = (0..64).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    put_str(&mut t, &long);
    assert_eq!(
        t.cursor(),
        (1, 0),
        "wraps as soon as the last column is written"
    );
    assert_eq!(t.text()[0], long);
    // Fill the last line: the screen scrolls.
    for _ in 0..ROWS {
        put_str(&mut t, &long);
    }
    assert_eq!(t.cursor().0, ROWS - 1);
    assert!(t.text().iter().take(ROWS - 1).all(|l| l == &long));
}

#[test]
fn unknown_control_codes_and_high_bit() {
    let mut t = term();
    for c in [0x00u8, 0x01, 0x07, 0x1B, 0x1F, 0x09] {
        t.put(c);
    }
    assert!(t.screen().iter().all(|&b| b == b' '));
    assert_eq!(t.cursor(), (0, 0));
    t.put(b'A' | 0x80); // 7-bit ASCII: bit 7 ignored
    assert_eq!(t.text()[0], "A");
    t.put(0x8D); // CR with bit 7 set
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn thirty_two_column_mode() {
    let mut t = Vid1Terminal::new(&Vid1Config {
        columns: 32,
        ..Vid1Config::default()
    });
    assert_eq!(t.columns(), 32);
    assert_eq!(t.screen().len(), 16 * 32);
    put_str(&mut t, &"x".repeat(33));
    assert_eq!(t.cursor(), (1, 1));
    assert_eq!(t.text()[0].len(), 32);
}

fn bind(code: u8, action: Vid1Action) -> ControlCode {
    ControlCode { code, action }
}

#[test]
fn configurable_codes_for_the_unverified_functions() {
    let mut cfg = Vid1Config::default();
    cfg.codes.extend([
        bind(0x1C, Vid1Action::CursorRight),
        bind(0x1D, Vid1Action::CursorUp),
        bind(0x1E, Vid1Action::CursorDown),
        bind(0x1F, Vid1Action::Home),
        bind(0x0B, Vid1Action::EraseToEol),
        bind(0x0E, Vid1Action::EraseToEos),
    ]);
    let mut t = Vid1Terminal::new(&cfg);
    put_str(&mut t, "0123456789");
    t.put(0x0D);
    t.put(0x0A);
    put_str(&mut t, "abcdef");
    assert_eq!(t.cursor(), (1, 6));

    t.put(0x1D); // up
    assert_eq!(t.cursor(), (0, 6));
    t.put(0x1C); // right
    assert_eq!(t.cursor(), (0, 7));
    t.put(0x1E); // down
    assert_eq!(t.cursor(), (1, 7));
    t.put(0x1F); // home
    assert_eq!(t.cursor(), (0, 0));

    for _ in 0..4 {
        t.put(0x1C);
    }
    t.put(0x0B); // erase to end of line
    assert_eq!(t.text()[0], "0123");
    assert_eq!(t.text()[1], "abcdef", "other lines untouched");

    t.put(0x1F);
    t.put(0x1C);
    t.put(0x1C);
    t.put(0x0E); // erase to end of screen
    assert_eq!(t.text()[0], "01");
    assert_eq!(t.text()[1], "");

    // Cursor movement limits: no scrolling, no wrapping.
    t.put(0x1F);
    t.put(0x1D);
    assert_eq!(t.cursor(), (0, 0));
    for _ in 0..40 {
        t.put(0x1E);
    }
    assert_eq!(t.cursor().0, ROWS - 1, "CursorDown stops at the bottom");
    for _ in 0..100 {
        t.put(0x1C);
    }
    assert_eq!(t.cursor().1, 63);
}

#[test]
fn rubout_steps_back_and_erases() {
    let mut cfg = Vid1Config::default();
    cfg.codes.push(bind(0x7F, Vid1Action::Rubout)); // later entry overrides the default
    let mut t = Vid1Terminal::new(&cfg);
    put_str(&mut t, "abc");
    t.put(0x7F);
    assert_eq!(t.text()[0], "ab");
    assert_eq!(t.cursor(), (0, 2));
}

#[test]
fn control_code_table_in_json_and_validation() {
    let c = Config::from_json(
        r#"{ "vid1": { "columns": 32, "codes": [ {"code": 12, "action": "ClearScreen"} ] } }"#,
    )
    .unwrap();
    assert_eq!(c.vid1.columns, 32);
    assert_eq!(c.vid1.codes.len(), 1);
    assert_eq!(Config::from_json(&c.to_json()).unwrap(), c);
    // Only the columns given: the default code table stays.
    let c = Config::from_json(r#"{ "vid1": { "columns": 32 } }"#).unwrap();
    assert_eq!(c.vid1.codes, Vid1Config::default().codes);
    // Printable characters cannot be bound.
    assert!(
        Config::from_json(r#"{ "vid1": { "codes": [ {"code": 65, "action": "Home"} ] } }"#)
            .is_err()
    );
}

proptest! {
    /// Any byte stream leaves the cursor on the screen and only printable characters
    /// (or blanks) in the buffer, for both screen widths.
    #[test]
    fn prop_screen_stays_valid(bytes in proptest::collection::vec(any::<u8>(), 0..600), wide in any::<bool>()) {
        let mut cfg = Vid1Config { columns: if wide { 64 } else { 32 }, ..Vid1Config::default() };
        cfg.codes.extend([
            bind(0x1C, Vid1Action::CursorRight),
            bind(0x1D, Vid1Action::CursorUp),
            bind(0x1E, Vid1Action::CursorDown),
            bind(0x1F, Vid1Action::Home),
            bind(0x0B, Vid1Action::EraseToEol),
            bind(0x0E, Vid1Action::EraseToEos),
            bind(0x01, Vid1Action::Rubout),
        ]);
        let mut t = Vid1Terminal::new(&cfg);
        for b in bytes {
            t.put(b);
            let (r, c) = t.cursor();
            prop_assert!(r < ROWS && c < t.columns());
        }
        prop_assert_eq!(t.screen().len(), ROWS * t.columns());
        prop_assert!(t.screen().iter().all(|&b| (0x20..=0x7E).contains(&b)));
    }
}

// ------------------------------------------------------------------ with the machine

const ECHO: &[u8] = include_bytes!("../../../tests/programs/serial_echo.bin");

fn echo_machine() -> Machine {
    let mut m = Machine::new(Config::default()).unwrap();
    m.poke(0, ECHO);
    m.set_switch(Switch::Run, true);
    m
}

#[test]
fn echoed_characters_appear_on_the_screen() {
    let mut m = echo_machine();
    for &b in b"Hi\r" {
        m.term_key(b);
        m.run(25_000);
    }
    m.run(30_000);
    let text = m.vid1_text();
    assert_eq!(text[0], "Hi");
    assert_eq!(
        m.vid1_cursor(),
        Some((0, 0)),
        "CR returned the cursor to column 1"
    );
    assert_eq!(m.vid1_columns(), Some(64));
}

#[test]
fn typed_keys_do_not_appear_without_an_echo() {
    // No program running: full duplex, no local echo.
    let mut m = Machine::new(Config::default()).unwrap();
    m.poke(0, &[0x30, 0x00]); // BR 0000: ignores the serial line
    m.set_switch(Switch::Run, true);
    for &b in b"typed" {
        m.term_key(b);
        m.run(25_000);
    }
    assert!(m.vid1_text().iter().all(|l| l.is_empty()));
    assert_eq!(m.vid1_cursor(), Some((0, 0)));
}

#[test]
fn no_vid1_without_the_serial_port() {
    let m = Machine::new(Config {
        serial: None,
        ..Config::default()
    })
    .unwrap();
    assert!(m.vid1_screen().is_none());
    assert!(m.vid1_text().is_empty());
}
