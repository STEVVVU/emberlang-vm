use crate::ast::{BinaryOp, TypeName, UnaryOp};
use crate::ir::{IrKind, IrProgram, Label, Reg, Value};
use crate::span::Span;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct BytecodeProgram {
    pub functions: Vec<BytecodeFunction>,
    pub function_names: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct BytecodeFunction {
    pub name: String,
    pub parameter_count: usize,
    pub return_type: TypeName,
    pub register_count: usize,
    pub code: Vec<Instruction>,
}

#[derive(Clone, Debug)]
pub struct Instruction {
    pub op: Op,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Op {
    Const {
        dst: Reg,
        value: Value,
    },
    Copy {
        dst: Reg,
        src: Reg,
    },
    Unary {
        dst: Reg,
        op: UnaryOp,
        operand: Reg,
    },
    Binary {
        dst: Reg,
        op: BinaryOp,
        left: Reg,
        right: Reg,
    },
    Call {
        dst: Reg,
        function: usize,
        args: Vec<Reg>,
    },
    Print {
        src: Reg,
    },
    Jump {
        target: usize,
    },
    JumpIfFalse {
        condition: Reg,
        target: usize,
    },
    Return {
        value: Option<Reg>,
    },
}

#[must_use]
pub fn generate(ir: &IrProgram) -> BytecodeProgram {
    let function_names: HashMap<String, usize> = ir
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.clone(), index))
        .collect();
    let functions = ir
        .functions
        .iter()
        .map(|function| generate_function(function, &function_names))
        .collect();
    BytecodeProgram {
        functions,
        function_names,
    }
}

fn generate_function(
    function: &crate::ir::IrFunction,
    function_names: &HashMap<String, usize>,
) -> BytecodeFunction {
    let mut labels: HashMap<Label, usize> = HashMap::new();
    let mut offset = 0;
    for instruction in &function.instructions {
        if let IrKind::Label(label) = instruction.kind {
            labels.insert(label, offset);
        } else {
            offset += instruction_width(&instruction.kind);
        }
    }
    let mut code = Vec::new();
    for instruction in &function.instructions {
        emit_instruction(&mut code, instruction, &labels, function_names);
    }
    BytecodeFunction {
        name: function.name.clone(),
        parameter_count: function.parameter_count,
        return_type: function.return_type,
        register_count: function.register_count,
        code,
    }
}

fn emit_instruction(
    code: &mut Vec<Instruction>,
    instruction: &crate::ir::IrInstruction,
    labels: &HashMap<Label, usize>,
    function_names: &HashMap<String, usize>,
) {
    let span = instruction.span;
    let op = match &instruction.kind {
        IrKind::Label(_) => return,
        IrKind::Const { dst, value } => Op::Const {
            dst: *dst,
            value: *value,
        },
        IrKind::Copy { dst, src } => Op::Copy {
            dst: *dst,
            src: *src,
        },
        IrKind::Unary { dst, op, operand } => Op::Unary {
            dst: *dst,
            op: *op,
            operand: *operand,
        },
        IrKind::Binary {
            dst,
            op,
            left,
            right,
        } => Op::Binary {
            dst: *dst,
            op: *op,
            left: *left,
            right: *right,
        },
        IrKind::Call {
            dst,
            function,
            args,
        } => Op::Call {
            dst: *dst,
            function: function_names[function],
            args: args.clone(),
        },
        IrKind::Print { src } => Op::Print { src: *src },
        IrKind::Jump(label) => Op::Jump {
            target: labels[label],
        },
        IrKind::Branch {
            condition,
            then_label,
            else_label,
        } => {
            code.push(Instruction {
                op: Op::JumpIfFalse {
                    condition: *condition,
                    target: labels[else_label],
                },
                span,
            });
            Op::Jump {
                target: labels[then_label],
            }
        }
        IrKind::Return(value) => Op::Return { value: *value },
    };
    code.push(Instruction { op, span });
}

const fn instruction_width(kind: &IrKind) -> usize {
    if matches!(kind, IrKind::Branch { .. }) {
        2
    } else if matches!(kind, IrKind::Label(_)) {
        0
    } else {
        1
    }
}

impl BytecodeProgram {
    #[must_use]
    pub fn disassemble(&self) -> String {
        let mut output = String::new();
        for (function_index, function) in self.functions.iter().enumerate() {
            let _ = writeln!(
                output,
                "function {function_index} {} (params={}, registers={}) -> {}",
                function.name,
                function.parameter_count,
                function.register_count,
                function.return_type
            );
            for (offset, instruction) in function.code.iter().enumerate() {
                let _ = writeln!(
                    output,
                    "  {offset:04}  {}",
                    DisplayOp(&instruction.op, self)
                );
            }
            output.push('\n');
        }
        output
    }
}

struct DisplayOp<'a>(&'a Op, &'a BytecodeProgram);

impl std::fmt::Display for DisplayOp<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Op::Const { dst, value } => write!(f, "CONST         r{dst}, {value}"),
            Op::Copy { dst, src } => write!(f, "COPY          r{dst}, r{src}"),
            Op::Unary { dst, op, operand } => {
                write!(f, "{op:?}       r{dst}, r{operand}")
            }
            Op::Binary {
                dst,
                op,
                left,
                right,
            } => write!(f, "{op:?}       r{dst}, r{left}, r{right}"),
            Op::Call {
                dst,
                function,
                args,
            } => {
                write!(
                    f,
                    "CALL          r{dst}, {}(",
                    self.1.functions[*function].name
                )?;
                for (index, argument) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "r{argument}")?;
                }
                write!(f, ")")
            }
            Op::Print { src } => write!(f, "PRINT         r{src}"),
            Op::Jump { target } => write!(f, "JUMP          {target:04}"),
            Op::JumpIfFalse { condition, target } => {
                write!(f, "JUMP_IF_FALSE r{condition}, {target:04}")
            }
            Op::Return { value: Some(value) } => write!(f, "RETURN        r{value}"),
            Op::Return { value: None } => write!(f, "RETURN"),
        }
    }
}
