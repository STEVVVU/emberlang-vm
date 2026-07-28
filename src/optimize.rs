use crate::ast::{BinaryOp, UnaryOp};
use crate::ir::{IrInstruction, IrKind, IrProgram, Reg, Value};
use std::collections::HashMap;

pub fn optimize(program: &mut IrProgram) {
    for function in &mut program.functions {
        program.optimization_stats.constants_folded += fold_constants(&mut function.instructions);
        program.optimization_stats.dead_instructions_removed +=
            eliminate_dead_results(&mut function.instructions);
    }
}

fn fold_constants(instructions: &mut [IrInstruction]) -> usize {
    let mut constants: HashMap<Reg, Value> = HashMap::new();
    let mut folded = 0;
    for instruction in instructions {
        match instruction.kind.clone() {
            IrKind::Label(_) | IrKind::Jump(_) | IrKind::Return(_) => constants.clear(),
            IrKind::Const { dst, value } => {
                constants.insert(dst, value);
            }
            IrKind::Copy { dst, src } => {
                if let Some(value) = constants.get(&src).copied() {
                    instruction.kind = IrKind::Const { dst, value };
                    constants.insert(dst, value);
                    folded += 1;
                } else {
                    constants.remove(&dst);
                }
            }
            IrKind::Unary { dst, op, operand } => {
                if let Some(value) = constants
                    .get(&operand)
                    .copied()
                    .and_then(|value| unary(op, value))
                {
                    instruction.kind = IrKind::Const { dst, value };
                    constants.insert(dst, value);
                    folded += 1;
                } else {
                    constants.remove(&dst);
                }
            }
            IrKind::Binary {
                dst,
                op,
                left,
                right,
            } => {
                let value = constants
                    .get(&left)
                    .copied()
                    .zip(constants.get(&right).copied())
                    .and_then(|(left, right)| binary(op, left, right));
                if let Some(value) = value {
                    instruction.kind = IrKind::Const { dst, value };
                    constants.insert(dst, value);
                    folded += 1;
                } else {
                    constants.remove(&dst);
                }
            }
            IrKind::Branch {
                condition,
                then_label,
                else_label,
            } => {
                if let Some(Value::Bool(value)) = constants.get(&condition) {
                    instruction.kind = IrKind::Jump(if *value { then_label } else { else_label });
                    folded += 1;
                }
                constants.clear();
            }
            IrKind::Call { dst, .. } => {
                constants.remove(&dst);
            }
            IrKind::Print { .. } => {}
        }
    }
    folded
}

fn eliminate_dead_results(instructions: &mut Vec<IrInstruction>) -> usize {
    let original_len = instructions.len();
    loop {
        let mut uses: HashMap<Reg, usize> = HashMap::new();
        for instruction in &*instructions {
            for register in instruction.used_registers() {
                *uses.entry(register).or_default() += 1;
            }
        }
        let previous_len = instructions.len();
        instructions.retain(|instruction| {
            let Some(destination) = instruction.destination() else {
                return true;
            };
            !instruction.is_pure() || uses.get(&destination).copied().unwrap_or(0) > 0
        });
        if instructions.len() == previous_len {
            break;
        }
    }
    original_len - instructions.len()
}

pub(crate) fn unary(op: UnaryOp, value: Value) -> Option<Value> {
    match (op, value) {
        (UnaryOp::Negate, Value::Int(value)) => value.checked_neg().map(Value::Int),
        (UnaryOp::Not, Value::Bool(value)) => Some(Value::Bool(!value)),
        _ => None,
    }
}

pub(crate) fn binary(op: BinaryOp, left: Value, right: Value) -> Option<Value> {
    match (op, left, right) {
        (BinaryOp::Add, Value::Int(left), Value::Int(right)) => {
            left.checked_add(right).map(Value::Int)
        }
        (BinaryOp::Subtract, Value::Int(left), Value::Int(right)) => {
            left.checked_sub(right).map(Value::Int)
        }
        (BinaryOp::Multiply, Value::Int(left), Value::Int(right)) => {
            left.checked_mul(right).map(Value::Int)
        }
        (BinaryOp::Divide, Value::Int(left), Value::Int(right)) => {
            left.checked_div(right).map(Value::Int)
        }
        (BinaryOp::Remainder, Value::Int(left), Value::Int(right)) => {
            left.checked_rem(right).map(Value::Int)
        }
        (BinaryOp::Equal, left, right) => Some(Value::Bool(left == right)),
        (BinaryOp::NotEqual, left, right) => Some(Value::Bool(left != right)),
        (BinaryOp::Less, Value::Int(left), Value::Int(right)) => Some(Value::Bool(left < right)),
        (BinaryOp::LessEqual, Value::Int(left), Value::Int(right)) => {
            Some(Value::Bool(left <= right))
        }
        (BinaryOp::Greater, Value::Int(left), Value::Int(right)) => Some(Value::Bool(left > right)),
        (BinaryOp::GreaterEqual, Value::Int(left), Value::Int(right)) => {
            Some(Value::Bool(left >= right))
        }
        (BinaryOp::And, Value::Bool(left), Value::Bool(right)) => Some(Value::Bool(left && right)),
        (BinaryOp::Or, Value::Bool(left), Value::Bool(right)) => Some(Value::Bool(left || right)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, CompileOptions};

    #[test]
    fn folds_constant_expression_and_removes_dead_temporary() {
        let source = "fn main() -> int { let x: int = 2 + 3 * 4; print(x); return 0; }";
        let compilation = compile(source, CompileOptions::default()).unwrap();
        assert!(compilation.ir.optimization_stats.constants_folded >= 3);
        assert!(compilation.ir.optimization_stats.dead_instructions_removed >= 2);
    }

    #[test]
    fn does_not_fold_division_by_zero() {
        assert_eq!(binary(BinaryOp::Divide, Value::Int(1), Value::Int(0)), None);
    }
}
