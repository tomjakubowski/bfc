
use std::fmt;
use std::num::Wrapping;
use std::collections::HashMap;

use self::Instruction::*;

pub type Cell = Wrapping<i8>;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Instruction {
    Increment(Cell),
    PointerIncrement(isize),
    Read,
    Write,
    Loop(Vec<Instruction>),
    // These instruction have no direct equivalent in BF, but we
    // generate them during optimisation.
    Set(Cell),
    MultiplyMove(HashMap<isize, Cell>),
}

fn fmt_with_indent(instr: &Instruction, indent: i32, f: &mut fmt::Formatter) {
    for _ in 0..indent {
        let _ = write!(f, "  ");
    }

    match instr {
        &Loop(ref loop_body) => {
            let _ = write!(f, "Loop");

            for loop_instr in loop_body.iter() {
                let _ = write!(f, "\n");
                fmt_with_indent(loop_instr, indent + 1, f);
            }
        }
        instr @ _ => {
            let _ = write!(f, "{:?}", instr);
        }
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_with_indent(self, 0, f);
        Ok(())
    }
}

/// Given a string of BF source code, parse and return our BF IR
/// representation.
pub fn parse(source: &str) -> Result<Vec<Instruction>, String> {
    parse_between(source, 0, source.chars().count())
}

/// Parse BF source code from index `start` up to (but excluding)
/// index `end`.
fn parse_between(source: &str, start: usize, end: usize) -> Result<Vec<Instruction>, String> {
    let chars: Vec<_> = source.chars().collect();
    assert!(start <= end);
    assert!(end <= chars.len());

    let mut instructions = Vec::new();
    let mut index = start;

    while index < end {
        match chars[index] {
            '+' => instructions.push(Increment(Wrapping(1))),
            '-' => instructions.push(Increment(Wrapping(-1))),
            '>' => instructions.push(PointerIncrement(1)),
            '<' => instructions.push(PointerIncrement(-1)),
            ',' => instructions.push(Read),
            '.' => instructions.push(Write),
            '[' => {
                let close_index = try!(find_close(source, index));
                let loop_body = try!(parse_between(source, index + 1, close_index));
                instructions.push(Loop(loop_body));

                index = close_index;
            }
            ']' => {
                return Err(format!("Unmatched ] at index {}.", index));
            }
            _ => (),
        }

        index += 1;
    }

    Ok(instructions)
}

/// Find the index of the `]` that matches the `[` at `open_index`.
fn find_close(source: &str, open_index: usize) -> Result<usize, String> {
    assert_eq!(source.chars().nth(open_index), Some('['));

    let mut nesting_depth = 0;
    for (index, c) in source.chars().enumerate() {
        if index < open_index {
            continue;
        }

        match c {
            '[' => nesting_depth += 1,
            ']' => nesting_depth -= 1,
            _ => (),
        }

        if nesting_depth == 0 {
            return Ok(index)
        }
    }
    // TODO: show line number
    Err(format!("Could not find matching ] for [ at index {}.", open_index))
}

#[test]
fn parse_increment() {
    assert_eq!(parse("+").unwrap(), [Increment(Wrapping(1))]);
    assert_eq!(parse("++").unwrap(), [Increment(Wrapping(1)),
                                      Increment(Wrapping(1))]);
}

#[test]
fn parse_decrement() {
    assert_eq!(parse("-").unwrap(), [Increment(Wrapping(-1))]);
}

#[test]
fn parse_pointer_increment() {
    assert_eq!(parse(">").unwrap(), [PointerIncrement(1)]);
}

#[test]
fn parse_pointer_decrement() {
    assert_eq!(parse("<").unwrap(), [PointerIncrement(-1)]);
}

#[test]
fn parse_read() {
    assert_eq!(parse(",").unwrap(), [Read]);
}

#[test]
fn parse_write() {
    assert_eq!(parse(".").unwrap(), [Write]);
}

#[test]
fn parse_empty_loop() {
    let expected = [Loop(vec![])];
    assert_eq!(parse("[]").unwrap(), expected);
}

#[test]
fn parse_simple_loop() {
    let loop_body = vec![Increment(Wrapping(1))];
    let expected = [Loop(loop_body)];
    assert_eq!(parse("[+]").unwrap(), expected);
}

#[test]
fn parse_complex_loop() {
    let loop_body = vec![Read, Increment(Wrapping(1))];
    let expected = [Write,
                    Loop(loop_body),
                    Increment(Wrapping(-1))];
    assert_eq!(parse(".[,+]-").unwrap(), expected);
}

#[test]
fn parse_unbalanced_loop() {
    assert!(parse("[").is_err());
    assert!(parse("]").is_err());
}

#[test]
fn parse_comment() {
    assert_eq!(parse("foo! ").unwrap(), []);
}
