use clap::{ArgAction, Args, ColorChoice, Parser, Subcommand, ValueEnum};
use emberlang_vm::diagnostic::{render_all_with, RenderStyle};
use emberlang_vm::vm::{execute, VmLimits};
use emberlang_vm::{compile, CompileOptions};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULTS: &str = "Default VM limits: 1,000,000 steps, 256 calls, 1,000,000 output bytes";

#[derive(Debug, Parser)]
#[command(
    name = "ember",
    version,
    about = "Ember compiler and bytecode VM",
    after_help = DEFAULTS,
    // `Auto` resolves against the real stream at parse time, so `--help` and
    // clap's own errors match the diagnostic renderer's colour decision.
    color = ColorChoice::Auto
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse, resolve, and type-check without execution
    Check {
        #[command(flatten)]
        input: Input,
    },
    // Doc comments on clap items become `--help` text verbatim, so they are
    // written as prose: markdown backticks would be shown to the user as-is.
    /// Compile and execute the entry function in the sandboxed VM
    Run {
        #[command(flatten)]
        input: Input,
        #[command(flatten)]
        limits: LimitArgs,
    },
    /// Print deterministic register bytecode
    Disassemble {
        #[command(flatten)]
        input: Input,
    },
    /// Print the register IR, before or after optimization
    Ir {
        #[command(flatten)]
        input: Input,
    },
}

#[derive(Debug, Args)]
struct Input {
    /// Ember source file
    file: PathBuf,
    /// Skip constant folding and dead-result elimination
    #[arg(long = "no-opt", action = ArgAction::SetTrue)]
    no_opt: bool,
    /// When to colorize diagnostics
    #[arg(long, value_enum, default_value_t = ColorMode::Auto, value_name = "WHEN")]
    color: ColorMode,
}

impl Input {
    fn compile_options(&self) -> CompileOptions {
        CompileOptions {
            optimize: !self.no_opt,
        }
    }

    fn render_style(&self) -> RenderStyle {
        self.color.render_style()
    }
}

/// VM resource limits. Each flag lowers or raises one bound; the sandbox itself
/// is always enforced, so there is no way to switch a limit off.
///
/// Every `long` name is spelled out rather than derived, because the fields drop
/// the `max_` prefix that the flags keep: the struct name already supplies it,
/// and repeating it in each field trips `clippy::struct_field_names`.
#[derive(Debug, Args)]
struct LimitArgs {
    /// Maximum instructions executed before trapping
    #[arg(long = "max-steps", value_name = "N")]
    steps: Option<u64>,
    /// Maximum simultaneous call frames
    #[arg(long = "max-call-depth", value_name = "N")]
    call_depth: Option<usize>,
    /// Maximum bytes of program output
    #[arg(long = "max-output-bytes", value_name = "N")]
    output_bytes: Option<usize>,
    /// Maximum registers in a single call frame
    #[arg(long = "max-registers-per-frame", value_name = "N")]
    registers_per_frame: Option<usize>,
    /// Maximum functions in a loaded program
    #[arg(long = "max-functions", value_name = "N")]
    functions: Option<usize>,
}

impl LimitArgs {
    /// Applies the overrides on top of the defaults, leaving every unspecified
    /// bound at its default value.
    fn resolve(&self) -> VmLimits {
        let defaults = VmLimits::default();
        VmLimits {
            max_steps: self.steps.unwrap_or(defaults.max_steps),
            max_call_depth: self.call_depth.unwrap_or(defaults.max_call_depth),
            max_output_bytes: self.output_bytes.unwrap_or(defaults.max_output_bytes),
            max_registers_per_frame: self
                .registers_per_frame
                .unwrap_or(defaults.max_registers_per_frame),
            max_functions: self.functions.unwrap_or(defaults.max_functions),
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorMode {
    /// Colorize only when stderr is a terminal
    Auto,
    /// Always emit ANSI colors and Unicode gutters
    Always,
    /// Never emit ANSI colors; ASCII gutters only
    Never,
}

impl ColorMode {
    fn render_style(self) -> RenderStyle {
        match self {
            // Piped or redirected stderr gets the plain renderer, so captured
            // output stays byte-identical across platforms.
            Self::Auto if std::io::stderr().is_terminal() => RenderStyle::Colored,
            Self::Auto | Self::Never => RenderStyle::Plain,
            Self::Always => RenderStyle::Colored,
        }
    }
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: &Cli) -> Result<u8, String> {
    let input = cli.command.input();
    let filename = input.file.display().to_string();
    let options = input.compile_options();

    let source = fs::read_to_string(&input.file)
        .map_err(|error| format!("error: failed to read `{filename}`: {error}"))?;
    let compilation = match compile(&source, options) {
        Ok(compilation) => compilation,
        Err(diagnostics) => {
            return Err(render_all_with(
                &filename,
                &source,
                &diagnostics,
                input.render_style(),
            ));
        }
    };

    match &cli.command {
        Command::Check { .. } => {
            let instruction_count: usize = compilation
                .bytecode
                .functions
                .iter()
                .map(|function| function.code.len())
                .sum();
            println!(
                "checked {}: {} functions, {} bytecode instructions",
                display_name(&input.file),
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
        Command::Run { limits, .. } => match execute(&compilation.bytecode, limits.resolve()) {
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
            Err(error) => {
                Err(error
                    .diagnostic()
                    .render_with(&filename, &source, input.render_style()))
            }
        },
        Command::Disassemble { .. } => {
            print!("{}", compilation.bytecode.disassemble());
            Ok(0)
        }
        Command::Ir { .. } => {
            print!("{}", compilation.ir);
            Ok(0)
        }
    }
}

impl Command {
    /// Every subcommand takes the same input arguments; this collapses the match
    /// so callers that only need the file/flags do not repeat it.
    fn input(&self) -> &Input {
        match self {
            Self::Check { input }
            | Self::Disassemble { input }
            | Self::Ir { input }
            | Self::Run { input, .. } => input,
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn unspecified_limits_keep_their_defaults() {
        let cli = Cli::parse_from(["ember", "run", "prog.em", "--max-steps", "10"]);
        let Command::Run { limits, .. } = &cli.command else {
            panic!("expected the run subcommand");
        };
        let resolved = limits.resolve();
        let defaults = VmLimits::default();
        assert_eq!(resolved.max_steps, 10);
        assert_eq!(resolved.max_call_depth, defaults.max_call_depth);
        assert_eq!(resolved.max_output_bytes, defaults.max_output_bytes);
        assert_eq!(
            resolved.max_registers_per_frame,
            defaults.max_registers_per_frame
        );
        assert_eq!(resolved.max_functions, defaults.max_functions);
    }

    #[test]
    fn no_opt_disables_optimization() {
        let disabled = Cli::parse_from(["ember", "check", "prog.em", "--no-opt"]);
        assert!(!disabled.command.input().compile_options().optimize);
        let enabled = Cli::parse_from(["ember", "check", "prog.em"]);
        assert!(enabled.command.input().compile_options().optimize);
    }

    #[test]
    fn color_flag_selects_the_renderer() {
        assert_eq!(ColorMode::Never.render_style(), RenderStyle::Plain);
        assert_eq!(ColorMode::Always.render_style(), RenderStyle::Colored);
    }
}
