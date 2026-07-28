use crate::ast::{BinaryOp, UnaryOp};
use crate::bytecode::{BytecodeProgram, Op};
use crate::diagnostic::Diagnostic;
use crate::ir::Value;
use crate::optimize::{binary, unary};
use crate::span::Span;

#[derive(Clone, Copy, Debug)]
pub struct VmLimits {
    pub max_steps: u64,
    pub max_call_depth: usize,
    pub max_output_bytes: usize,
    pub max_registers_per_frame: usize,
    pub max_functions: usize,
}

impl Default for VmLimits {
    fn default() -> Self {
        Self {
            max_steps: 1_000_000,
            max_call_depth: 256,
            max_output_bytes: 1_000_000,
            max_registers_per_frame: 65_536,
            max_functions: 10_000,
        }
    }
}

#[derive(Debug)]
pub struct Execution {
    pub exit_code: i64,
    pub output: String,
    pub steps: u64,
    pub max_depth: usize,
}

#[derive(Debug)]
pub struct VmError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub stack: Vec<String>,
}

impl VmError {
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::error(self.code, &self.message, self.span);
        if !self.stack.is_empty() {
            diagnostic = diagnostic.with_note(format!("call stack: {}", self.stack.join(" -> ")));
        }
        diagnostic
    }
}

/// Executes `main` using deterministic register frames and explicit limits.
///
/// # Errors
///
/// Returns a source-mapped trap for invalid bytecode, arithmetic faults, or a
/// resource-limit violation.
pub fn execute(program: &BytecodeProgram, limits: VmLimits) -> Result<Execution, VmError> {
    let mut vm = Vm {
        program,
        limits,
        steps: 0,
        max_depth: 0,
        output: String::new(),
        stack: Vec::new(),
    };
    if program.functions.len() > limits.max_functions {
        return Err(vm.error(
            "E7001",
            format!(
                "program has {} functions; VM limit is {}",
                program.functions.len(),
                limits.max_functions
            ),
            Span::default(),
        ));
    }
    let Some(&main) = program.function_names.get("main") else {
        return Err(vm.error("E7002", "bytecode has no `main` function", Span::default()));
    };
    let value = vm.call(main, &[], 1)?;
    let Value::Int(exit_code) = value else {
        return Err(vm.error(
            "E7003",
            "`main` returned a non-integer value",
            Span::default(),
        ));
    };
    Ok(Execution {
        exit_code,
        output: vm.output,
        steps: vm.steps,
        max_depth: vm.max_depth,
    })
}

struct Vm<'a> {
    program: &'a BytecodeProgram,
    limits: VmLimits,
    steps: u64,
    max_depth: usize,
    output: String,
    stack: Vec<String>,
}

impl Vm<'_> {
    fn call(
        &mut self,
        function_index: usize,
        args: &[Value],
        depth: usize,
    ) -> Result<Value, VmError> {
        if depth > self.limits.max_call_depth {
            return Err(self.error(
                "E7101",
                format!(
                    "call depth exceeded limit of {}",
                    self.limits.max_call_depth
                ),
                Span::default(),
            ));
        }
        self.max_depth = self.max_depth.max(depth);
        let function = &self.program.functions[function_index];
        if function.register_count > self.limits.max_registers_per_frame {
            return Err(self.error(
                "E7102",
                format!(
                    "function `{}` requires {} registers; frame limit is {}",
                    function.name, function.register_count, self.limits.max_registers_per_frame
                ),
                Span::default(),
            ));
        }
        if args.len() != function.parameter_count {
            return Err(self.error(
                "E7103",
                format!("invalid bytecode call to `{}`", function.name),
                Span::default(),
            ));
        }

        self.stack.push(function.name.clone());
        let result = self.run_frame(function_index, args);
        self.stack.pop();
        result
    }

    fn run_frame(&mut self, function_index: usize, args: &[Value]) -> Result<Value, VmError> {
        let function = &self.program.functions[function_index];
        let mut registers = vec![Value::Unit; function.register_count];
        registers[..args.len()].copy_from_slice(args);
        let mut ip = 0;

        while let Some(instruction) = function.code.get(ip) {
            self.steps = self.steps.saturating_add(1);
            if self.steps > self.limits.max_steps {
                return Err(self.error(
                    "E7104",
                    format!("execution exceeded step limit of {}", self.limits.max_steps),
                    instruction.span,
                ));
            }
            ip += 1;
            match &instruction.op {
                Op::Const { dst, value } => registers[*dst] = *value,
                Op::Copy { dst, src } => registers[*dst] = registers[*src],
                Op::Unary { dst, op, operand } => {
                    registers[*dst] =
                        self.eval_unary(*op, registers[*operand], instruction.span)?;
                }
                Op::Binary {
                    dst,
                    op,
                    left,
                    right,
                } => {
                    registers[*dst] = self.eval_binary(
                        *op,
                        registers[*left],
                        registers[*right],
                        instruction.span,
                    )?;
                }
                Op::Call {
                    dst,
                    function,
                    args,
                } => {
                    let values: Vec<Value> =
                        args.iter().map(|register| registers[*register]).collect();
                    let depth = self.stack.len() + 1;
                    registers[*dst] = self.call(*function, &values, depth)?;
                }
                Op::Print { src } => {
                    let line = format!("{}\n", registers[*src]);
                    if self.output.len() + line.len() > self.limits.max_output_bytes {
                        return Err(self.error(
                            "E7105",
                            format!(
                                "program output exceeded {} byte limit",
                                self.limits.max_output_bytes
                            ),
                            instruction.span,
                        ));
                    }
                    self.output.push_str(&line);
                }
                Op::Jump { target } => ip = *target,
                Op::JumpIfFalse { condition, target } => {
                    if registers[*condition] == Value::Bool(false) {
                        ip = *target;
                    }
                }
                Op::Return { value } => {
                    return Ok(value.map_or(Value::Unit, |register| registers[register]));
                }
            }
        }
        Ok(Value::Unit)
    }

    fn eval_unary(&self, op: UnaryOp, value: Value, span: Span) -> Result<Value, VmError> {
        unary(op, value).ok_or_else(|| {
            self.error(
                "E7201",
                match (op, value) {
                    (UnaryOp::Negate, Value::Int(_)) => "integer overflow in negation".to_owned(),
                    _ => "invalid unary operation in bytecode".to_owned(),
                },
                span,
            )
        })
    }

    fn eval_binary(
        &self,
        op: BinaryOp,
        left: Value,
        right: Value,
        span: Span,
    ) -> Result<Value, VmError> {
        binary(op, left, right).ok_or_else(|| {
            let message = match (op, right) {
                (BinaryOp::Divide | BinaryOp::Remainder, Value::Int(0)) => {
                    "division by zero".to_owned()
                }
                (
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Remainder,
                    Value::Int(_),
                ) => "integer overflow".to_owned(),
                _ => "invalid binary operation in bytecode".to_owned(),
            };
            self.error("E7202", message, span)
        })
    }

    fn error(&self, code: &'static str, message: impl Into<String>, span: Span) -> VmError {
        VmError {
            code,
            message: message.into(),
            span,
            stack: self.stack.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, CompileOptions};

    fn run(source: &str) -> Execution {
        let compilation = compile(source, CompileOptions::default()).unwrap();
        execute(&compilation.bytecode, VmLimits::default()).unwrap()
    }

    #[test]
    fn executes_calls_and_loops() {
        let execution = run(
            "fn sum(n: int) -> int { let total: int = 0; let i: int = 0; \
             while i < n { total = total + i; i = i + 1; } return total; } \
             fn main() -> int { print(sum(5)); return 0; }",
        );
        assert_eq!(execution.output, "10\n");
        assert_eq!(execution.exit_code, 0);
    }

    #[test]
    fn traps_infinite_loop_at_step_limit() {
        let compilation = compile(
            "fn main() -> int { while true { print(1); } return 0; }",
            CompileOptions::default(),
        )
        .unwrap();
        let limits = VmLimits {
            max_steps: 20,
            max_output_bytes: 1_000,
            ..VmLimits::default()
        };
        let error = execute(&compilation.bytecode, limits).unwrap_err();
        assert_eq!(error.code, "E7104");
    }

    #[test]
    fn traps_division_by_zero_with_span() {
        let compilation = compile(
            "fn main() -> int { let x: int = 0; return 10 / x; }",
            CompileOptions::default(),
        )
        .unwrap();
        let error = execute(&compilation.bytecode, VmLimits::default()).unwrap_err();
        assert_eq!(error.message, "division by zero");
        assert!(error.span.end > error.span.start);
    }
}
