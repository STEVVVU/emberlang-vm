use crate::ast::{self, BinaryOp, Block, Expr, ExprKind, Program, Stmt, TypeName, UnaryOp};
use crate::collections::{HashMap, HashSet};
use crate::span::Span;
use std::fmt;

pub type Reg = usize;
pub type Label = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Unit => write!(f, "()"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OptimizationStats {
    pub constants_folded: usize,
    pub dead_instructions_removed: usize,
}

#[derive(Clone, Debug)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
    pub optimization_stats: OptimizationStats,
}

#[derive(Clone, Debug)]
pub struct IrFunction {
    pub name: String,
    pub parameter_count: usize,
    pub return_type: TypeName,
    pub register_count: usize,
    pub variable_registers: HashSet<Reg>,
    pub instructions: Vec<IrInstruction>,
}

#[derive(Clone, Debug)]
pub struct IrInstruction {
    pub kind: IrKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum IrKind {
    Label(Label),
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
        function: String,
        args: Vec<Reg>,
    },
    Print {
        src: Reg,
    },
    Jump(Label),
    Branch {
        condition: Reg,
        then_label: Label,
        else_label: Label,
    },
    Return(Option<Reg>),
}

impl IrInstruction {
    #[must_use]
    pub fn destination(&self) -> Option<Reg> {
        match self.kind {
            IrKind::Const { dst, .. }
            | IrKind::Copy { dst, .. }
            | IrKind::Unary { dst, .. }
            | IrKind::Binary { dst, .. }
            | IrKind::Call { dst, .. } => Some(dst),
            _ => None,
        }
    }

    #[must_use]
    pub fn used_registers(&self) -> Vec<Reg> {
        match &self.kind {
            IrKind::Copy { src, .. } | IrKind::Print { src } => vec![*src],
            IrKind::Unary { operand, .. } => vec![*operand],
            IrKind::Binary { left, right, .. } => vec![*left, *right],
            IrKind::Call { args, .. } => args.clone(),
            IrKind::Branch { condition, .. } => vec![*condition],
            IrKind::Return(Some(value)) => vec![*value],
            IrKind::Label(_) | IrKind::Const { .. } | IrKind::Jump(_) | IrKind::Return(None) => {
                Vec::new()
            }
        }
    }

    #[must_use]
    pub const fn is_pure(&self) -> bool {
        matches!(
            self.kind,
            IrKind::Const { .. }
                | IrKind::Copy { .. }
                | IrKind::Unary { .. }
                | IrKind::Binary { .. }
        )
    }
}

pub fn lower(program: &Program) -> IrProgram {
    IrProgram {
        functions: program.functions.iter().map(lower_function).collect(),
        optimization_stats: OptimizationStats::default(),
    }
}

fn lower_function(function: &ast::Function) -> IrFunction {
    let mut lowerer = FunctionLowerer {
        instructions: Vec::new(),
        scopes: vec![HashMap::default()],
        next_register: 0,
        next_label: 0,
        variable_registers: HashSet::default(),
    };
    for parameter in &function.params {
        let register = lowerer.new_variable();
        lowerer
            .scopes
            .last_mut()
            .expect("lowering scope exists")
            .insert(parameter.name.clone(), register);
    }
    lowerer.block(&function.body, false);
    lowerer.emit(
        IrKind::Return(None),
        Span::new(function.body.span.end, function.body.span.end),
    );
    IrFunction {
        name: function.name.clone(),
        parameter_count: function.params.len(),
        return_type: function.return_type,
        register_count: lowerer.next_register,
        variable_registers: lowerer.variable_registers,
        instructions: lowerer.instructions,
    }
}

struct FunctionLowerer {
    instructions: Vec<IrInstruction>,
    scopes: Vec<HashMap<String, Reg>>,
    next_register: Reg,
    next_label: Label,
    variable_registers: HashSet<Reg>,
}

impl FunctionLowerer {
    fn block(&mut self, block: &Block, nested: bool) {
        if nested {
            self.scopes.push(HashMap::default());
        }
        for statement in &block.statements {
            self.statement(statement);
        }
        if nested {
            self.scopes.pop();
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name, value, span, ..
            } => {
                let source = self.expression(value);
                let destination = self.new_variable();
                self.emit(
                    IrKind::Copy {
                        dst: destination,
                        src: source,
                    },
                    *span,
                );
                self.scopes
                    .last_mut()
                    .expect("lowering scope exists")
                    .insert(name.clone(), destination);
            }
            Stmt::Assign {
                name, value, span, ..
            } => {
                let source = self.expression(value);
                let destination = self.lookup(name);
                self.emit(
                    IrKind::Copy {
                        dst: destination,
                        src: source,
                    },
                    *span,
                );
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let condition = self.expression(condition);
                let then_label = self.new_label();
                let else_label = self.new_label();
                let end_label = self.new_label();
                self.emit(
                    IrKind::Branch {
                        condition,
                        then_label,
                        else_label,
                    },
                    *span,
                );
                self.emit(IrKind::Label(then_label), then_block.span);
                self.block(then_block, true);
                self.emit(IrKind::Jump(end_label), then_block.span);
                self.emit(IrKind::Label(else_label), *span);
                if let Some(block) = else_block {
                    self.block(block, true);
                }
                self.emit(IrKind::Jump(end_label), *span);
                self.emit(IrKind::Label(end_label), *span);
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                let condition_label = self.new_label();
                let body_label = self.new_label();
                let end_label = self.new_label();
                self.emit(IrKind::Label(condition_label), *span);
                let condition = self.expression(condition);
                self.emit(
                    IrKind::Branch {
                        condition,
                        then_label: body_label,
                        else_label: end_label,
                    },
                    *span,
                );
                self.emit(IrKind::Label(body_label), body.span);
                self.block(body, true);
                self.emit(IrKind::Jump(condition_label), body.span);
                self.emit(IrKind::Label(end_label), *span);
            }
            Stmt::Return { value, span } => {
                let value = value.as_ref().map(|value| self.expression(value));
                self.emit(IrKind::Return(value), *span);
            }
            Stmt::Print { value, span } => {
                let source = self.expression(value);
                self.emit(IrKind::Print { src: source }, *span);
            }
            Stmt::Expr { value, .. } => {
                self.expression(value);
            }
        }
    }

    fn expression(&mut self, expression: &Expr) -> Reg {
        match &expression.kind {
            ExprKind::Integer(value) => {
                let destination = self.new_temporary();
                self.emit(
                    IrKind::Const {
                        dst: destination,
                        value: Value::Int(*value),
                    },
                    expression.span,
                );
                destination
            }
            ExprKind::Boolean(value) => {
                let destination = self.new_temporary();
                self.emit(
                    IrKind::Const {
                        dst: destination,
                        value: Value::Bool(*value),
                    },
                    expression.span,
                );
                destination
            }
            ExprKind::Variable(name) => self.lookup(name),
            ExprKind::Unary { op, operand } => {
                let operand = self.expression(operand);
                let destination = self.new_temporary();
                self.emit(
                    IrKind::Unary {
                        dst: destination,
                        op: *op,
                        operand,
                    },
                    expression.span,
                );
                destination
            }
            ExprKind::Binary { left, op, right } => {
                let left = self.expression(left);
                let right = self.expression(right);
                let destination = self.new_temporary();
                self.emit(
                    IrKind::Binary {
                        dst: destination,
                        op: *op,
                        left,
                        right,
                    },
                    expression.span,
                );
                destination
            }
            ExprKind::Call { callee, args, .. } => {
                let args = args
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect();
                let destination = self.new_temporary();
                self.emit(
                    IrKind::Call {
                        dst: destination,
                        function: callee.clone(),
                        args,
                    },
                    expression.span,
                );
                destination
            }
        }
    }

    fn emit(&mut self, kind: IrKind, span: Span) {
        self.instructions.push(IrInstruction { kind, span });
    }

    fn new_variable(&mut self) -> Reg {
        let register = self.new_temporary();
        self.variable_registers.insert(register);
        register
    }

    fn new_temporary(&mut self) -> Reg {
        let register = self.next_register;
        self.next_register += 1;
        register
    }

    fn new_label(&mut self) -> Label {
        let label = self.next_label;
        self.next_label += 1;
        label
    }

    fn lookup(&self, name: &str) -> Reg {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .expect("resolved variable must lower")
    }
}

impl fmt::Display for IrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for function in &self.functions {
            writeln!(
                f,
                "fn {} (params={}, regs={}) -> {}:",
                function.name,
                function.parameter_count,
                function.register_count,
                function.return_type
            )?;
            for instruction in &function.instructions {
                writeln!(f, "  {}", DisplayInstruction(&instruction.kind))?;
            }
        }
        Ok(())
    }
}

struct DisplayInstruction<'a>(&'a IrKind);

impl fmt::Display for DisplayInstruction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            IrKind::Label(label) => write!(f, "bb{label}:"),
            IrKind::Const { dst, value } => write!(f, "%{dst} = const {value}"),
            IrKind::Copy { dst, src } => write!(f, "%{dst} = copy %{src}"),
            IrKind::Unary { dst, op, operand } => {
                write!(f, "%{dst} = {op:?} %{operand}")
            }
            IrKind::Binary {
                dst,
                op,
                left,
                right,
            } => write!(f, "%{dst} = {op:?} %{left}, %{right}"),
            IrKind::Call {
                dst,
                function,
                args,
            } => {
                write!(f, "%{dst} = call {function}(")?;
                for (index, argument) in args.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "%{argument}")?;
                }
                write!(f, ")")
            }
            IrKind::Print { src } => write!(f, "print %{src}"),
            IrKind::Jump(label) => write!(f, "jump bb{label}"),
            IrKind::Branch {
                condition,
                then_label,
                else_label,
            } => write!(f, "branch %{condition}, bb{then_label}, bb{else_label}"),
            IrKind::Return(Some(value)) => write!(f, "return %{value}"),
            IrKind::Return(None) => write!(f, "return"),
        }
    }
}
