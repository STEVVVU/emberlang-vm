use emberlang_vm::diagnostic::render_all;
use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8, String> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().ok_or_else(usage)?;
    if command == "--help" || command == "-h" {
        println!("{}", usage());
        return Ok(0);
    }
    if command == "--version" || command == "-V" {
        println!("ember {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let filename = arguments.next().ok_or_else(usage)?;
    let mut options = CompileOptions::default();
    let mut limits = VmLimits::default();
    let remaining: Vec<String> = arguments.collect();
    parse_options(&remaining, &mut options, &mut limits)?;

    let source = fs::read_to_string(&filename)
        .map_err(|error| format!("error: failed to read `{filename}`: {error}"))?;
    let compilation = match compile(&source, options) {
        Ok(compilation) => compilation,
        Err(diagnostics) => {
            return Err(render_all(&filename, &source, &diagnostics));
        }
    };

    match command.as_str() {
        "check" => {
            let instruction_count: usize = compilation
                .bytecode
                .functions
                .iter()
                .map(|function| function.code.len())
                .sum();
            println!(
                "checked {}: {} functions, {} bytecode instructions",
                display_name(&filename),
                compilation.bytecode.functions.len(),
                instruction_count
            );
            if options.optimize {
                println!(
                    "optimized: {} constants folded, {} dead instructions removed",
                    compilation.ir.optimization_stats.constants_folded,
                    compilation.ir.optimization_stats.dead_instructions_removed
                );
            }
            Ok(0)
        }
        "run" => match execute(&compilation.bytecode, limits) {
            Ok(execution) => {
                print!("{}", execution.output);
                if execution.exit_code == 0 {
                    Ok(0)
                } else {
                    u8::try_from(execution.exit_code).map_err(|_| {
                        format!(
                            "error: exit code {} is outside 0..=255",
                            execution.exit_code
                        )
                    })
                }
            }
            Err(error) => Err(error.diagnostic().render(&filename, &source)),
        },
        "disassemble" => {
            print!("{}", compilation.bytecode.disassemble());
            Ok(0)
        }
        _ => Err(format!("unknown command `{command}`\n\n{}", usage())),
    }
}

fn parse_options(
    arguments: &[String],
    compile_options: &mut CompileOptions,
    limits: &mut VmLimits,
) -> Result<(), String> {
    let mut cursor = 0;
    while cursor < arguments.len() {
        match arguments[cursor].as_str() {
            "--no-opt" => compile_options.optimize = false,
            "--max-steps" => {
                cursor += 1;
                limits.max_steps = parse_limit(arguments.get(cursor), "--max-steps")?;
            }
            "--max-call-depth" => {
                cursor += 1;
                limits.max_call_depth = parse_limit(arguments.get(cursor), "--max-call-depth")?;
            }
            "--max-output-bytes" => {
                cursor += 1;
                limits.max_output_bytes = parse_limit(arguments.get(cursor), "--max-output-bytes")?;
            }
            argument => return Err(format!("unknown option `{argument}`\n\n{}", usage())),
        }
        cursor += 1;
    }
    Ok(())
}

fn parse_limit<T>(value: Option<&String>, option: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    value
        .ok_or_else(|| format!("missing value after `{option}`"))?
        .parse()
        .map_err(|_| format!("invalid numeric value for `{option}`"))
}

fn display_name(filename: &str) -> &str {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filename)
}

fn usage() -> String {
    "Ember compiler and bytecode VM

Usage:
  ember check <file> [--no-opt]
  ember run <file> [--no-opt] [--max-steps N] [--max-call-depth N] [--max-output-bytes N]
  ember disassemble <file> [--no-opt]

Commands:
  check         Parse, resolve, and type-check without execution
  run           Compile and execute main() in the sandboxed VM
  disassemble   Print deterministic register bytecode

Default VM limits: 1,000,000 steps, 256 calls, 1,000,000 output bytes"
        .to_owned()
}
