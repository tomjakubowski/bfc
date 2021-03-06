#![feature(plugin)]
#![plugin(quickcheck_macros)]

#![warn(trivial_numeric_casts)]

extern crate libc;
extern crate llvm_sys;
extern crate itertools;
extern crate quickcheck;
extern crate rand;
extern crate tempfile;
extern crate getopts;

use std::env;
use std::fs::File;
use std::io::Write;
use std::io::prelude::Read;
use std::num::Wrapping;
use std::path::Path;
use std::process::Command;
use getopts::{Options, Matches};
use tempfile::NamedTempFile;

mod bfir;
mod llvm;
mod peephole;
mod bounds;
mod execution;

#[cfg(test)]
mod peephole_tests;
#[cfg(test)]
mod llvm_tests;

/// Read the contents of the file at path, and return a string of its
/// contents.
fn slurp(path: &str) -> Result<String, std::io::Error> {
    let mut file = try!(File::open(path));
    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));
    Ok(contents)
}

/// Convert "foo.bf" to "foo".
#[allow(deprecated)] // .connect is in stable 1.2, but beta has deprecated it.
fn executable_name(bf_file_name: &str) -> String {
    let mut name_parts: Vec<_> = bf_file_name.split('.').collect();
    let parts_len = name_parts.len();
    if parts_len > 1 {
        name_parts.pop();
    }

    name_parts.connect(".")
}

fn print_usage(bin_name: &str, opts: Options) {
    let brief = format!("Usage: {} <BF source file> [options]", bin_name);
    print!("{}", opts.usage(&brief));
}

fn convert_io_error<T>(result: Result<T, std::io::Error>) -> Result<T, String> {
    match result {
        Ok(value) => {
            Ok(value)
        }
        Err(e) => {
            Err(format!("{}", e))
        }
    }
}

fn shell_command(command: &str, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new(command);
    for arg in args {
        c.arg(arg);
    }

    let result = try!(convert_io_error(c.output()));
    if result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        Ok((*stdout).to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err((*stderr).to_owned())
    }

}

fn compile_file(matches: &Matches) -> Result<(), String> {
    let ref path = matches.free[0];
    let src = try!(convert_io_error(slurp(path)));

    let mut instrs = try!(bfir::parse(&src));

    let opt_level = matches.opt_str("opt").unwrap_or(String::from("2"));
    if opt_level != "0" {
        instrs = peephole::optimize(instrs);
    }

    let state = if opt_level == "2" {
        execution::execute(&instrs, execution::MAX_STEPS)
    } else {
        execution::ExecutionState {
            instr_ptr: 0,
            cells: vec![Wrapping(0); bounds::highest_cell_index(&instrs) + 1],
            cell_ptr: 0,
            outputs: vec![],
        }
    };
    let initial_cells: Vec<i8> = state.cells.iter()
        .map(|x: &Wrapping<i8>| x.0).collect();

    let remaining_instrs = &instrs[state.instr_ptr..];

    if matches.opt_present("dump-ir") {
        if remaining_instrs.is_empty() {
            println!("(optimized out)");
        }

        for instr in remaining_instrs {
            println!("{}", instr);
        }
        return Ok(());
    }

    let llvm_ir_raw = llvm::compile_to_ir(
        path, &remaining_instrs.to_vec(), &initial_cells, state.cell_ptr as i32,
        &state.outputs);

    if matches.opt_present("dump-llvm") {
        let llvm_ir = String::from_utf8_lossy(llvm_ir_raw.as_bytes());
        println!("{}", llvm_ir);
        return Ok(());
    }                        

    // Write the LLVM IR to a temporary file.
    let mut llvm_ir_file = try!(convert_io_error(NamedTempFile::new()));
    let _ = llvm_ir_file.write(llvm_ir_raw.as_bytes());

    // Compile the LLVM IR to a temporary object file.
    let object_file = try!(convert_io_error(NamedTempFile::new()));

    let llvm_opt_arg = format!("-O{}", matches.opt_str("llvm-opt").unwrap_or(String::from("3")));

    let llc_args = [&llvm_opt_arg[..], "-filetype=obj",
                    llvm_ir_file.path().to_str().unwrap(),
                    "-o", object_file.path().to_str().unwrap()];
    try!(shell_command("llc", &llc_args[..]));

    // TODO: do path munging in executable_name().
    let bf_name = Path::new(path).file_name().unwrap();
    let output_name = executable_name(bf_name.to_str().unwrap());

    // Link the object file.
    let clang_args = [object_file.path().to_str().unwrap(),
                      "-o", &output_name[..]];
    try!(shell_command("clang", &clang_args[..]));

    // Strip the executable.
    let strip_args = ["-s", &output_name[..]];
    try!(shell_command("strip", &strip_args[..]));

    Ok(())
}

#[cfg_attr(test, allow(dead_code))]
fn main() {
    let args: Vec<_> = env::args().collect();

    let mut opts = Options::new();

    opts.optflag("h", "help", "show usage");
    opts.optflag("", "dump-llvm", "print LLVM IR generated");
    opts.optflag("", "dump-ir", "print BF IR generated");

    opts.optopt("O", "opt", "optimization level (0 to 2)", "LEVEL");
    opts.optopt("", "llvm-opt", "LLVM optimization level (0 to 3)", "LEVEL");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => {
            m
        }
        Err(_) => {
            print_usage(&args[0], opts);
            std::process::exit(1);
        }
    };

    if matches.opt_present("h") {
        print_usage(&args[0], opts);
        return;
    }

    if matches.free.len() != 1 {
        print_usage(&args[0], opts);
        std::process::exit(1);
    }

    match compile_file(&matches) {
        Ok(_) => {}
        Err(e) => {
            // TODO: this should go to stderr.
            println!("{}", e);
            std::process::exit(2);
        }
    }
}
