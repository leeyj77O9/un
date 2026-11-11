//! AST → bytecode compiler. The input is the **desugared, optimized** tree, so the
//! compiler never sees `+=` forms, `if/elif/else` chains, or folded constants
//! (DECISIONS.md D-004).
//!
//! Conventions the reference evaluator forces:
//! - every expression node leaves its value on the stack, unwrapped at that node's span,
//!   exactly where `Evaluator.cs` calls `Unwrap(..., node)`;
//! - a block unwraps every child (`EvalBlock`, `Evaluator.cs:584`) and pops every child
//!   result except the last, which is the block's value;
//! - `EvalList` / `EvalTuple` / `EvalDict` do not unwrap their elements, so literal
//!   elements carry no `UNWRAP`;
//! - `EvalCall` unwraps inside the frame it pushes, so `CALL` performs the unwrap.

use crate::ast::{Const, Node, NodeKind};
use crate::bytecode::instr::*;
use crate::bytecode::{Constant, ExcEntry, ExcKind, Program, Proto};
use crate::error::Panic;
use crate::source::Source;
use crate::token::TokenType;

/// jump targets recorded inside a control-flow group and patched when it closes.
#[derive(Debug, Default)]
pub struct Group {
    pub breaks: Vec<usize>,
    pub skips: Vec<usize>,
    pub is_loop: bool,
}

/// one compilation pass. `protos` is `program.protos`, and the last proto is the one
/// being built, so `emit` always appends to the top of that stack.
pub struct Compiler<'a> {
    pub source: &'a Source,
    pub program: Program,
    /// the loop group stack. phase 8 has no loops, so this stays empty; `open_group`
    /// and `close_group` are part of the compiler shape so phase 9 adds `break`/`skip`
    /// patching without restructuring the compiler.
    pub groups: Vec<Group>,
    /// set when the last compiled node ends in a control transfer (`UNSUPPORTED`), so
    /// the statement produces no value and `code_block` must not emit `UNWRAP`/`POP`.
    /// `code` clears it for every node, so it always describes the innermost node.
    pub raises: bool,
}

/// compiles the desugared + optimized root AST. the root is the module's `Block`.
pub fn compile(source: &Source, ast: &Node) -> Result<Program, Panic> {
    let mut c = Compiler::new(source);
    c.code_block(ast, false)?;
    c.finish()
}

impl<'a> Compiler<'a> {
    pub fn new(source: &'a Source) -> Self {
        let mut program = Program {
            source_path: source.name().to_string(),
            names: Vec::new(),
            consts: Vec::new(),
            protos: Vec::new(),
            msgs: Vec::new(),
            root: 0,
        };
        let name = push_name(&mut program, "<module>");
        program.protos.push(Proto::new(name, 0));
        Self {
            source,
            program,
            groups: Vec::new(),
            raises: false,
        }
    }

    /// the finished program; the root proto is the module body.
    pub fn finish(self) -> Result<Program, Panic> {
        if self.program.protos.len() != 1 {
            return Err(Panic::named("internal", "unbalanced proto stack"));
        }
        Ok(self.program)
    }

    pub fn open_group(&mut self, is_loop: bool) {
        self.groups.push(Group {
            breaks: Vec::new(),
            skips: Vec::new(),
            is_loop,
        });
    }

    pub fn record_break(&mut self, ip: usize) {
        if let Some(group) = self.groups.last_mut() {
            group.breaks.push(ip);
        }
    }

    pub fn record_skip(&mut self, ip: usize) {
        if let Some(group) = self.groups.last_mut() {
            group.skips.push(ip);
        }
    }

    pub fn close_group(&mut self, break_target: usize, skip_target: usize) -> Option<Group> {
        let group = self.groups.pop()?;
        for ip in &group.breaks {
            self.patch(*ip, break_target);
        }
        for ip in &group.skips {
            self.patch(*ip, skip_target);
        }
        Some(group)
    }

    fn current(&self) -> usize {
        self.program.protos.len() - 1
    }

    fn ip(&self) -> usize {
        self.program.protos[self.current()].code.len()
    }

    /// name table index for a proto-local name. names are shared through the module
    /// table, so a name is stored once. operands carry the **proto-local slot**, and
    /// `Program::name(proto, slot)` resolves `proto.names[slot]` against the module table.
    fn name_index(&mut self, name: &str) -> usize {
        let p = self.current();
        for (slot, &gi) in self.program.protos[p].names.iter().enumerate() {
            if self.program.names[gi] == name {
                return slot;
            }
        }
        let gi = push_name(&mut self.program, name);
        self.program.protos[p].names.push(gi);
        self.program.protos[p].names.len() - 1
    }

    /// appends a name with no dedup and returns its proto-local slot, used where indices
    /// must be contiguous.
    fn push_name_raw(&mut self, name: &str) -> usize {
        let p = self.current();
        let gi = push_name(&mut self.program, name);
        self.program.protos[p].names.push(gi);
        self.program.protos[p].names.len() - 1
    }

    /// appends `count` names in order and returns the index of the first. `BUILD_TUPLE_NAMED`
    /// maps entry `i` to `flags + i`, so contiguity is part of the contract.
    fn push_names_contiguous(&mut self, names: &[String]) -> usize {
        let first = self.push_name_raw(&names[0]);
        for name in &names[1..] {
            self.push_name_raw(name);
        }
        first
    }

    /// constant operands carry the **proto-local slot** too, resolved by
    /// `Program::constant(proto, slot)` against the shared module table.
    fn const_index(&mut self, value: Constant) -> usize {
        let p = self.current();
        for (slot, &gi) in self.program.protos[p].consts.iter().enumerate() {
            if self.program.consts[gi] == value {
                return slot;
            }
        }
        let gi = self.program.consts.len();
        self.program.consts.push(value);
        self.program.protos[p].consts.push(gi);
        self.program.protos[p].consts.len() - 1
    }

    fn msg_index(&mut self, message: &str) -> usize {
        if let Some(i) = self.program.msgs.iter().position(|m| m == message) {
            return i;
        }
        self.program.msgs.push(message.to_string());
        self.program.msgs.len() - 1
    }

    /// per call site argument names. `-1` is a positional argument, the rest are name
    /// table indices. this table is what `.unbc` stores; the running VM rebuilds the
    /// names from its accumulator, so it is filled for the format contract only.
    fn call_names_index(&mut self, entries: Vec<i32>) -> usize {
        let p = self.current();
        for (i, e) in self.program.protos[p].call_names.iter().enumerate() {
            if e == &entries {
                return i;
            }
        }
        self.program.protos[p].call_names.push(entries);
        self.program.protos[p].call_names.len() - 1
    }

    /// appends one instruction and its source span together. the `source_map` length
    /// invariant of BYTECODE.md §14 is the reason `emit` is the only way to grow `code`.
    fn emit(&mut self, op: Op, flags: u16, operand: i32, span: (usize, usize)) {
        let p = self.current();
        self.program.protos[p].code.push(Instr::with_flags(op, flags, operand));
        self.program.protos[p].source_map.push(span);
    }

    fn emit_jump(&mut self, op: Op, flags: u16, span: (usize, usize)) -> usize {
        let ip = self.ip();
        self.emit(op, flags, 0, span);
        ip
    }

    /// a jump operand is the delta from the **next** instruction (BYTECODE.md §1 rule 3).
    fn patch(&mut self, ip: usize, target: usize) {
        let p = self.current();
        self.program.protos[p].code[ip].operand = (target as i64 - ip as i64 - 1) as i32;
    }

    /// appends one exception-table entry and returns its index.
    fn add_exc(&mut self, entry: ExcEntry) -> usize {
        let p = self.current();
        self.program.protos[p].exc.push(entry);
        self.program.protos[p].exc.len() - 1
    }

    /// patches a `PUSH_TRY*` operand to an exception table index.
    fn patch_index(&mut self, ip: usize, index: usize) {
        let p = self.current();
        self.program.protos[p].code[ip].operand = index as i32;
    }

    fn unsupported(&mut self, node: &Node) {
        let message = format!("unsupported node kind {:?}", node.kind);
        let mi = self.msg_index(&message);
        self.emit(Op::Unsupported, 0, mi as i32, (node.start, node.length));
        self.raises = true;
    }

    // -----------------------------------------------------------------
    // expression
    // -----------------------------------------------------------------

    pub fn code(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        self.raises = false;

        match node.kind {
            NodeKind::Integer => {
                let v = match node.value.as_ref() {
                    Some(Const::Int(v)) => *v,
                    _ => return Err(Panic::named("internal", "invalid int object")),
                };
                let ci = self.const_index(Constant::Int(v));
                self.emit(Op::LoadConst, 0, ci as i32, span);
            }
            NodeKind::Float => {
                let v = match node.value.as_ref() {
                    Some(Const::Float(v)) => *v,
                    _ => return Err(Panic::named("internal", "invalid float object")),
                };
                let ci = self.const_index(Constant::Float(v));
                self.emit(Op::LoadConst, 0, ci as i32, span);
            }
            NodeKind::String => {
                let v = match node.value.as_ref() {
                    Some(Const::Str(v)) => v.clone(),
                    _ => return Err(Panic::named("internal", "invalid str object")),
                };
                let ci = self.const_index(Constant::Str(v));
                self.emit(Op::LoadConst, 0, ci as i32, span);
            }
            NodeKind::Boolean => {
                let v = match node.value.as_ref() {
                    Some(Const::Bool(v)) => *v,
                    _ => return Err(Panic::named("internal", "invalid bool object")),
                };
                self.emit(if v { Op::LoadTrue } else { Op::LoadFalse }, 0, 0, span);
            }
            NodeKind::None => self.emit(Op::LoadNone, 0, 0, span),
            // the parser's absent child marker (`a[..2]`). the reference reads it as a C#
            // `null`, so the flag bits carry the information, not the pushed value.
            NodeKind::Null => self.emit(Op::LoadNull, 0, 0, span),

            NodeKind::Identifier => {
                let name = self.text(node);
                let ni = self.name_index(&name);
                self.emit(Op::LoadName, 0, ni as i32, span);
            }

            NodeKind::FString => {
                for child in &node.children {
                    if child.kind == NodeKind::String {
                        let v = match child.value.as_ref() {
                            Some(Const::Str(v)) => v.clone(),
                            _ => return Err(Panic::named("internal", "invalid str object")),
                        };
                        let ci = self.const_index(Constant::Str(v));
                        self.emit(Op::LoadConst, 0, ci as i32, span_of(child));
                    } else {
                        self.code(child)?;
                        // `EvalFString` converts every expression part with `ToStr()` and
                        // the span of the check is the FString node
                        self.emit(Op::ToStr, 0, 0, span);
                    }
                }
                self.emit(Op::FString, 0, node.children.len() as i32, span);
            }

            NodeKind::Tuple => self.code_tuple(node)?,
            NodeKind::List => self.code_container(node, Op::BuildList)?,
            // `Eval` has no `Set` case, so `{1, 2}` is observable as
            // `unsupported node kind Set` at the node. `Spread` / `KwSpread` are only
            // evaluated by `EvalArguments`; a spread inside a container literal reaches
            // the same default arm, so it stays unsupported here too.
            NodeKind::Set | NodeKind::Spread | NodeKind::KwSpread => self.unsupported(node),
            NodeKind::Dict => self.code_dict(node)?,

            NodeKind::Assign => {
                self.code(&node.children[1])?;
                self.emit(Op::Dup, 0, 0, span);
                self.code_pattern(&node.children[0], false)?;
            }

            NodeKind::Binary => self.code_binary(node)?,
            NodeKind::Unary => self.code_unary(node)?,
            NodeKind::Ternary => self.code_ternary(node)?,

            NodeKind::Call => self.code_call(node)?,
            NodeKind::Property => self.code_property(node)?,
            NodeKind::Index => self.code_index(node)?,
            NodeKind::Slice => self.code_slice(node)?,

            NodeKind::Block => self.code_block(node, false)?,
            NodeKind::Use => self.code_use(node)?,

            NodeKind::If => self.code_if(node)?,
            NodeKind::While => self.code_while(node)?,
            NodeKind::For => self.code_for(node)?,
            NodeKind::Break => self.code_break(node),
            NodeKind::Skip => self.code_skip(node),

            _ => self.unsupported(node),
        }

        Ok(())
    }

    /// `EvalBlock` unwraps every child and keeps the last value as the block result.
    /// `pop_last` is the `for` / `while` body rule, where the last value is dropped too.
    pub fn code_block(&mut self, node: &Node, pop_last: bool) -> Result<(), Panic> {
        let n = node.children.len();
        if n == 0 {
            if !pop_last {
                self.emit(Op::LoadNone, 0, 0, (node.start, node.length));
            }
            self.raises = false;
            return Ok(());
        }

        for (i, child) in node.children.iter().enumerate() {
            self.code(child)?;

            // a control-transfer statement leaves nothing on the stack, so `UNWRAP`
            // and `POP` would underflow
            if self.raises {
                continue;
            }

            self.emit(Op::Unwrap, 0, 0, span_of(child));
            if i + 1 < n || pop_last {
                self.emit(Op::Pop, 0, 0, span_of(child));
            }
        }
        Ok(())
    }

    /// pattern binding. every form consumes exactly one value from the stack.
    pub fn code_pattern(&mut self, pattern: &Node, local_only: bool) -> Result<(), Panic> {
        let span = (pattern.start, pattern.length);

        match pattern.kind {
            NodeKind::Identifier => {
                let name = self.text(pattern);
                let ni = self.name_index(&name);
                let flags = if local_only { FLAG_LOCAL_ONLY } else { 0 };
                self.emit(Op::StoreName, flags, ni as i32, span);
            }
            NodeKind::Typed => {
                // `BindName` is the only effect, the type node is not evaluated.
                self.code_pattern(&pattern.children[0], local_only)?;
            }
            NodeKind::Wildcard => {
                self.emit(Op::Pop, 0, 0, span);
            }
            NodeKind::Tuple => {
                let count = pattern.children.len() as i32;
                self.emit(Op::Unpack, 0, count, span);
                // `BindPattern` binds `items[i]` to `values[i]` forward, and `UNPACK`
                // pushes the items so that `values[0]` sits on top of the stack.
                for child in &pattern.children {
                    self.code_pattern(child, local_only)?;
                }
            }
            NodeKind::Property => {
                self.emit(Op::Dup, 0, 0, span);
                self.code(&pattern.children[0])?;
                let name = self.text(&pattern.children[1]);
                let ni = self.name_index(&name);
                self.emit(Op::SetAttr, 0, ni as i32, span);
                self.emit(Op::Unwrap, 0, 0, span);
                self.emit(Op::Pop, 0, 0, span);
                self.emit(Op::Pop, 0, 0, span);
            }
            NodeKind::Index => {
                self.emit(Op::Dup, 0, 0, span);
                self.code(&pattern.children[0])?;
                self.code(&pattern.children[1])?;
                self.emit(Op::SetItem, 0, 0, span);
                self.emit(Op::Unwrap, 0, 0, span);
                self.emit(Op::Pop, 0, 0, span);
                self.emit(Op::Pop, 0, 0, span);
            }
            NodeKind::Slice => {
                self.emit(Op::Dup, 0, 0, span);
                self.code(&pattern.children[0])?;
                let flags = self.slice_bounds(pattern)?;
                self.emit(Op::SetSlice, flags, flags.count_ones() as i32, span);
                self.emit(Op::Pop, 0, 0, span);
                self.emit(Op::Pop, 0, 0, span);
            }
            _ => {
                let message = format!("invalid pattern {:?}", pattern.kind);
                let mi = self.msg_index(&message);
                self.emit(Op::Unsupported, 0, mi as i32, span);
            }
        }

        Ok(())
    }

    /// emits the bound expressions of a slice-shaped node and returns the flag bits.
    /// bit0 = start, bit1 = end, bit2 = step; an absent bound emits nothing.
    fn slice_bounds(&mut self, node: &Node) -> Result<u16, Panic> {
        let mut flags = 0u16;
        let slots = [FLAG_SLICE_START, FLAG_SLICE_END, FLAG_SLICE_STEP];

        for (i, slot) in slots.iter().enumerate() {
            if node.children.len() <= i + 1 {
                break;
            }
            let child = &node.children[i + 1];
            if child.kind == NodeKind::Null {
                continue;
            }
            self.code(child)?;
            flags |= slot;
        }

        Ok(flags)
    }

    // -----------------------------------------------------------------
    // control flow
    // -----------------------------------------------------------------

    fn code_if(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let mut end_jumps = Vec::new();
        let mut next_jumps: Vec<usize> = Vec::new();
        let mut has_else = false;
        let mut all_raise = true;

        for child in &node.children {
            // patch jumps from previous failed conditions to here
            let here = self.ip();
            for j in next_jumps.drain(..) {
                self.patch(j, here);
            }

            match child.kind {
                NodeKind::IfCase => {
                    let cond = &child.children[0];
                    let body = &child.children[1];
                    self.code(cond)?;
                    self.emit(Op::ToBool, 0, 0, span_of(cond));
                    let jnext = self.emit_jump(Op::JmpIf, FLAG_JUMP_POP, span_of(cond));
                    self.code(body)?;
                    if self.raises {
                        self.raises = false;
                    } else {
                        all_raise = false;
                        end_jumps.push(self.emit_jump(Op::Jmp, 0, span_of(body)));
                    }
                    next_jumps.push(jnext);
                }
                NodeKind::Else => {
                    has_else = true;
                    self.code(&child.children[0])?;
                    if self.raises {
                        self.raises = false;
                    } else {
                        all_raise = false;
                        end_jumps.push(self.emit_jump(Op::Jmp, 0, span_of(child)));
                    }
                }
                _ => {}
            }
        }

        let here = self.ip();
        for j in next_jumps.drain(..) {
            self.patch(j, here);
        }

        if !has_else {
            // fall-through when no condition matched and no else
            self.emit(Op::LoadNone, 0, 0, span);
            all_raise = false;
        }

        let end = self.ip();
        for j in end_jumps {
            self.patch(j, end);
        }

        if all_raise {
            self.raises = true;
        }
        Ok(())
    }

    fn code_while(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let cond = &node.children[0];
        let body = &node.children[1];

        let loop_start = self.ip();
        self.code(cond)?;
        // while condition is strict Bool (As<Bool>), non-Bool or false exits
        let jend = self.emit_jump(Op::JmpIf, FLAG_JUMP_POP, span_of(cond));

        self.open_group(true);
        self.code_block(body, true)?;
        self.emit(Op::Jmp, 0, (loop_start as i32) - (self.ip() as i32 + 1), span_of(body));

        let end = self.ip();
        self.patch(jend, end);
        self.close_group(end, loop_start);
        self.emit(Op::LoadNone, 0, 0, span);
        Ok(())
    }

    fn code_for(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let target = &node.children[0];
        let iterable = &node.children[1];
        let body = &node.children[2];

        self.code(iterable)?;
        let iter_text = self.source.span(iterable.start, iterable.length);
        let msg = format!("{iter_text} is not iterable");
        let mi = self.msg_index(&msg);
        self.emit(Op::PrepIter, 0, mi as i32, span_of(iterable));

        self.emit(Op::NewScope, FLAG_SCOPE_LOOP, 0, span);
        self.open_group(true);

        let loop_start = self.ip();
        let jend = self.emit_jump(Op::ForNext, 0, span_of(iterable));
        self.code_pattern(target, true)?;
        self.code_block(body, true)?;
        self.emit(Op::Jmp, 0, (loop_start as i32) - (self.ip() as i32 + 1), span_of(body));

        let exhausted = self.ip();
        self.patch(jend, exhausted);
        // iterator already popped by ForNext on exhausted path
        self.emit(Op::PopScope, 0, 0, span);
        let jafter = self.emit_jump(Op::Jmp, 0, span);

        let break_cleanup = self.ip();
        // iterator still on stack when breaking from body
        self.emit(Op::Pop, 0, 0, span);
        self.emit(Op::PopScope, 0, 0, span);

        let after = self.ip();
        self.patch(jafter, after);
        self.close_group(break_cleanup, loop_start);

        self.emit(Op::LoadNone, 0, 0, span);
        Ok(())
    }

    fn code_break(&mut self, node: &Node) {
        let span = (node.start, node.length);
        if let Some(pos) = self.groups.iter().rposition(|g| g.is_loop) {
            let ip = self.emit_jump(Op::Break, 0, span);
            self.groups[pos].breaks.push(ip);
            self.raises = true;
        } else {
            self.error_node(node, "'break' outside loop");
        }
    }

    fn code_skip(&mut self, node: &Node) {
        let span = (node.start, node.length);
        if let Some(pos) = self.groups.iter().rposition(|g| g.is_loop) {
            let ip = self.emit_jump(Op::Skip, 0, span);
            self.groups[pos].skips.push(ip);
            self.raises = true;
        } else {
            self.error_node(node, "'skip' outside loop");
        }
    }

    fn error_node(&mut self, node: &Node, message: &str) {
        let span = (node.start, node.length);
        let mi = self.msg_index(message);
        self.emit(Op::Unsupported, 0, mi as i32, span);
        self.raises = true;
    }

    // -----------------------------------------------------------------
    // literals
    // -----------------------------------------------------------------

    fn code_tuple(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let count = node.children.len();

        self.emit(Op::ArgBegin, 0, count as i32, span);

        let mut names: Vec<String> = Vec::new();
        for child in &node.children {
            match child.kind {
                NodeKind::Pair => {
                    let name = self.text(&child.children[0]);
                    self.code(&child.children[1])?;
                    let ni = self.name_index(&name);
                    self.emit(Op::ArgKw, 0, ni as i32, span_of(child));
                    names.push(name);
                }
                NodeKind::KwSpread => {
                    self.code(&child.children[0])?;
                    self.emit(Op::ArgKwSpread, 0, 0, span_of(child));
                    names.push(String::new());
                }
                _ => {
                    self.code(child)?;
                    self.emit(Op::ArgPos, 0, 0, span_of(child));
                    names.push(String::new());
                }
            }
        }

        let named = node.children.iter().any(|c| {
            c.kind == NodeKind::Pair || c.kind == NodeKind::KwSpread
        });

        if named {
            let first = self.push_names_contiguous(&names);
            self.emit(Op::BuildTupleNamed, first as u16, count as i32, span);
        } else {
            self.emit(Op::BuildTuple, 0, count as i32, span);
        }

        Ok(())
    }

    /// list / set literal: every element is pushed with `ARG_POS` and the build op
    /// consumes them in order.
    fn code_container(&mut self, node: &Node, build: Op) -> Result<(), Panic> {
        let span = (node.start, node.length);
        self.emit(Op::ArgBegin, 0, node.children.len() as i32, span);

        for child in &node.children {
            self.code(child)?;
            self.emit(Op::ArgPos, 0, 0, span_of(child));
        }

        self.emit(build, 0, node.children.len() as i32, span);
        Ok(())
    }

    fn code_dict(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        self.emit(Op::ArgBegin, 0, node.children.len() as i32, span);

        for pair in &node.children {
            self.code(&pair.children[0])?;
            self.emit(Op::ArgPos, 0, 0, span_of(&pair.children[0]));
            self.code(&pair.children[1])?;
            self.emit(Op::ArgPos, 0, 0, span_of(&pair.children[1]));
        }

        self.emit(Op::BuildDict, 0, node.children.len() as i32, span);
        Ok(())
    }

    // -----------------------------------------------------------------
    // operators
    // -----------------------------------------------------------------

    /// the short-circuit forms (`and` / `or` / `??`) keep the left value as the result
    /// when they short-circuit, which is why both sides sit in the code and the value
    /// stack carries the surviving one. `Unwrap` of `ToBool` is the `Unwrap(..., node)`
    /// the reference performs at the binary node; the ternary does not unwrap.
    fn code_binary(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let op = node.op.unwrap_or(TokenType::Error);

        match op {
            TokenType::And => {
                self.code(&node.children[0])?;
                self.emit(Op::Dup, 0, 0, span);
                self.emit(Op::ToBool, FLAG_STRICT, 0, span);
                let truthy = self.emit_jump(Op::JmpIf, FLAG_JUMP_TRUE | FLAG_JUMP_POP, span);
                // falsy: the popped `Bool` is gone, the surviving `left` is the result
                let end = self.emit_jump(Op::Jmp, 0, span);
                // truthy: drop the stale `left`, evaluate the right side
                let target = self.ip();
                self.patch(truthy, target);
                self.emit(Op::Pop, 0, 0, span);
                self.code(&node.children[1])?;
                self.patch(end, self.ip());
            }
            TokenType::Or => {
                self.code(&node.children[0])?;
                self.emit(Op::Dup, 0, 0, span);
                self.emit(Op::ToBool, FLAG_STRICT, 0, span);
                let falsy = self.emit_jump(Op::JmpIf, FLAG_JUMP_POP, span);
                // truthy: the surviving `left` is the result
                let end = self.emit_jump(Op::Jmp, 0, span);
                // falsy: drop the stale `left`, evaluate the right side
                let target = self.ip();
                self.patch(falsy, target);
                self.emit(Op::Pop, 0, 0, span);
                self.code(&node.children[1])?;
                self.patch(end, self.ip());
            }
            TokenType::DoubleQuestion => {
                self.code(&node.children[0])?;
                // `left.IsNone()` is the jump condition, so `FLAG_JUMP_TRUE` is required
                let right = self.emit_jump(
                    Op::JmpIf,
                    FLAG_JUMP_NONE | FLAG_JUMP_TRUE,
                    span,
                );
                let end = self.emit_jump(Op::Jmp, 0, span);
                let target = self.ip();
                self.patch(right, target);
                // `left` is `none`, drop it and evaluate the right side
                self.emit(Op::Pop, 0, 0, span);
                self.code(&node.children[1])?;
                self.patch(end, self.ip());
            }
            _ => {
                self.code(&node.children[0])?;
                self.code(&node.children[1])?;
                let op = match binary_op(op) {
                    Some(op) => op,
                    None => {
                        let message = format!("invalid binary operator {op:?}");
                        let mi = self.msg_index(&message);
                        self.emit(Op::Unsupported, 0, mi as i32, span);
                        return Ok(());
                    }
                };
                self.emit(op, 0, 0, span);
                self.emit(Op::Unwrap, 0, 0, span);
            }
        }

        Ok(())
    }

    fn code_unary(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let op = node.op.unwrap_or(TokenType::Error);

        // `EvalUnary`'s `?` form: the region is `[PUSH, PROPAGATE)`, the handler is
        // `PROPAGATE`, and the spans the propagation carries are the `?` node's spans.
        // `catch (Error e)` is the VM `handle`, and the original `LangError` is kept so
        // the top-level `throw;` shows the original span.
        if op == TokenType::Question {
            let push_ip = self.ip();
            self.emit(Op::PushTryProp, 0, 0, span);
            self.code(&node.children[0])?;
            let handler = self.ip();
            self.emit(Op::Propagate, 0, 0, span);

            let entry = ExcEntry {
                kind: ExcKind::Propagate,
                start: push_ip,
                end: self.ip(),
                handler,
                var_slot: None,
            };
            let index = self.add_exc(entry);
            self.patch_index(push_ip, index);
            return Ok(());
        }

        let op = match op {
            TokenType::Plus => Op::Pos,
            TokenType::Minus => Op::Neg,
            TokenType::Not => Op::Not,
            TokenType::BNot => Op::Bnot,
            TokenType::Asterisk | TokenType::Spread => Op::Spread,
            _ => {
                let message = format!("invalid unary operator {op:?}");
                let mi = self.msg_index(&message);
                self.emit(Op::Unsupported, 0, mi as i32, span);
                return Ok(());
            }
        };

        self.code(&node.children[0])?;
        self.emit(op, 0, 0, span);
        self.emit(Op::Unwrap, 0, 0, span);

        Ok(())
    }

    /// `EvalTernary` does **not** unwrap the condition, so an `Err` condition is falsy
    /// and the `else` branch runs.
    fn code_ternary(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);

        self.code(&node.children[0])?;
        self.emit(Op::ToBool, 0, 0, span);
        let truthy = self.emit_jump(Op::JmpIf, FLAG_JUMP_TRUE | FLAG_JUMP_POP, span);
        // falsy: `ToBool` consumed the condition, so the else branch runs on an empty stack
        self.code(&node.children[2])?;
        let end = self.emit_jump(Op::Jmp, 0, span);
        let target = self.ip();
        self.patch(truthy, target);
        self.code(&node.children[1])?;
        self.patch(end, self.ip());

        Ok(())
    }

    // -----------------------------------------------------------------
    // call / property / index / slice
    // -----------------------------------------------------------------

    fn code_call(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let args = node.children.get(1);

        self.code(&node.children[0])?;

        // `EvalCall` tests the optional chain **before** `EvalArguments`, so the guard
        // sits between the callee and the argument list: a `none` callee pops itself and
        // pushes `none`, and the argument expressions are never evaluated.
        if is_optional_chain(&node.children[0]) {
            let skip = self.emit_jump(Op::JmpIf, FLAG_JUMP_NONE | FLAG_JUMP_TRUE, span);
            self.emit_call(args, span)?;
            let end = self.emit_jump(Op::Jmp, 0, span);
            let target = self.ip();
            self.patch(skip, target);
            self.emit(Op::Pop, 0, 0, span);
            self.emit(Op::LoadNone, 0, 0, span);
            self.patch(end, self.ip());
            return Ok(());
        }

        self.emit_call(args, span)
    }

    /// `ArgBegin` + the argument ops + `CALL`. `CALL`'s operand is the static argument
    /// count, the VM uses the accumulator frame's length at run time so a spread can
    /// grow it.
    fn emit_call(&mut self, args: Option<&Node>, span: (usize, usize)) -> Result<(), Panic> {
        let count = args.map(|a| a.children.len()).unwrap_or(0);

        // the name table entries the call site records, positional is `-1`
        let mut entries: Vec<i32> = Vec::new();
        self.emit(Op::ArgBegin, 0, count as i32, span);

        for child in args.map(|a| a.children.as_slice()).unwrap_or(&[]) {
            match child.kind {
                NodeKind::Pair => {
                    let name = self.text(&child.children[0]);
                    self.code(&child.children[1])?;
                    let ni = self.name_index(&name);
                    self.emit(Op::ArgKw, 0, ni as i32, span_of(child));
                    entries.push(ni as i32);
                }
                NodeKind::KwSpread => {
                    self.code(&child.children[0])?;
                    self.emit(Op::ArgKwSpread, 0, 0, span_of(child));
                    entries.push(-1);
                }
                NodeKind::Spread => {
                    self.code(&child.children[0])?;
                    self.emit(Op::ArgSpread, 0, 0, span_of(child));
                    entries.push(-1);
                }
                _ => {
                    self.code(child)?;
                    self.emit(Op::ArgPos, 0, 0, span_of(child));
                    entries.push(-1);
                }
            }
        }

        let names_idx = self.call_names_index(entries);
        self.emit(Op::Call, names_idx as u16, count as i32, span);

        Ok(())
    }

    fn code_property(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let chain = node.op == Some(TokenType::QuestionDot) || is_optional_chain(&node.children[0]);

        self.code(&node.children[0])?;

        let name = self.text(&node.children[1]);
        let ni = self.name_index(&name);

        if chain {
            let get = self.emit_jump(Op::JmpIf, FLAG_JUMP_NONE | FLAG_JUMP_TRUE, span);
            self.emit(Op::GetAttr, 0, ni as i32, span);
            let end = self.emit_jump(Op::Jmp, 0, span);
            let target = self.ip();
            self.patch(get, target);
            self.emit(Op::Pop, 0, 0, span);
            self.emit(Op::LoadNone, 0, 0, span);
            self.patch(end, self.ip());
            self.emit(Op::Unwrap, 0, 0, span);
            return Ok(());
        }

        self.emit(Op::GetAttr, 0, ni as i32, span);
        self.emit(Op::Unwrap, 0, 0, span);

        Ok(())
    }

    fn code_index(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let chain = is_optional_chain(&node.children[0]);

        self.code(&node.children[0])?;

        if chain {
            let get = self.emit_jump(Op::JmpIf, FLAG_JUMP_NONE | FLAG_JUMP_TRUE, span);
            self.code(&node.children[1])?;
            self.emit(Op::GetItem, 0, 0, span);
            let end = self.emit_jump(Op::Jmp, 0, span);
            let target = self.ip();
            self.patch(get, target);
            self.emit(Op::Pop, 0, 0, span);
            self.emit(Op::LoadNone, 0, 0, span);
            self.patch(end, self.ip());
            self.emit(Op::Unwrap, 0, 0, span);
            return Ok(());
        }

        self.code(&node.children[1])?;
        self.emit(Op::GetItem, 0, 0, span);
        self.emit(Op::Unwrap, 0, 0, span);

        Ok(())
    }

    fn code_slice(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let chain = is_optional_chain(&node.children[0]);

        self.code(&node.children[0])?;

        if chain {
            // `EvalSlice` returns `none` before the bound expressions run, so they sit in
            // the non-skip branch
            let get = self.emit_jump(Op::JmpIf, FLAG_JUMP_NONE | FLAG_JUMP_TRUE, span);
            let flags = self.slice_bounds(node)?;
            self.emit(Op::Slice, flags, flags.count_ones() as i32, span);
            let end = self.emit_jump(Op::Jmp, 0, span);
            let target = self.ip();
            self.patch(get, target);
            self.emit(Op::Pop, 0, 0, span);
            self.emit(Op::LoadNone, 0, 0, span);
            self.patch(end, self.ip());
            self.emit(Op::Unwrap, 0, 0, span);
            return Ok(());
        }

        let flags = self.slice_bounds(node)?;
        self.emit(Op::Slice, flags, flags.count_ones() as i32, span);
        self.emit(Op::Unwrap, 0, 0, span);

        Ok(())
    }

    /// `use` is resolved at run time, exactly as `EvalUse` does. the path and the alias
    /// and the import pairs are laid out contiguously in the name table:
    /// `operand` is the path index, flags bit0 = alias, bit1 = import list,
    /// and the pair count is in flags' high byte.
    fn code_use(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);

        let path = self.text(&node.children[0]);
        // native and file modules are chosen at run time by `Global.IsNative`, so the
        // path is always compiled and `Op::Use` resolves it

        let mut alias: Option<String> = None;
        let mut imports: Vec<(String, String)> = Vec::new();

        for child in node.children.iter().skip(1) {
            match child.kind {
                NodeKind::Tuple => {
                    for item in &child.children {
                        if item.kind == NodeKind::Pair {
                            imports.push((
                                self.text(&item.children[0]),
                                self.text(&item.children[1]),
                            ));
                        } else {
                            let name = self.text(item);
                            imports.push((name.clone(), name));
                        }
                    }
                }
                NodeKind::Identifier => {
                    alias = Some(self.text(child));
                }
                _ => {}
            }
        }

        // the layout is fixed by the VM: path, alias when present, then the pairs
        let path_idx = self.push_name_raw(&path);
        let mut flags = 0u16;
        if let Some(alias) = &alias {
            self.push_name_raw(alias);
            flags |= 1;
        }
        if !imports.is_empty() {
            flags |= 2;
            flags |= (imports.len() as u16) << 8;
            for (name, alias) in &imports {
                self.push_name_raw(name);
                self.push_name_raw(alias);
            }
        }

        self.emit(Op::Use, flags, path_idx as i32, span);
        // `EvalUse` returns `Obj.None`, so the statement produces a value
        self.emit(Op::LoadNone, 0, 0, span);

        Ok(())
    }

    /// `Node.GetText` — the literal when the node carries one, otherwise the source slice.
    fn text(&self, node: &Node) -> String {
        match node.value.as_ref() {
            Some(Const::Str(v)) => v.clone(),
            _ => self.source.span(node.start, node.length),
        }
    }
}

fn push_name(program: &mut Program, name: &str) -> usize {
    program.names.push(name.to_string());
    program.names.len() - 1
}

/// the source span of a node. every `emit` call carries it, because `source_map` is the
/// only position a `.unbc` program can report (`Program.span`).
fn span_of(node: &Node) -> (usize, usize) {
    (node.start, node.length)
}

/// `IsOptionalChain` (`Evaluator.cs:297`).
fn is_optional_chain(node: &Node) -> bool {
    match node.kind {
        NodeKind::Property => node.op == Some(TokenType::QuestionDot) || is_optional_chain(&node.children[0]),
        NodeKind::Call => is_optional_chain(&node.children[0]),
        NodeKind::Index => is_optional_chain(&node.children[0]),
        NodeKind::Slice => is_optional_chain(&node.children[0]),
        _ => false,
    }
}

/// `TokenType` → the arithmetic opcode. the short-circuit operators are handled by
/// `code_binary` and never reach this table.
fn binary_op(op: TokenType) -> Option<Op> {
    Some(match op {
        TokenType::Plus => Op::Add,
        TokenType::Minus => Op::Sub,
        TokenType::Asterisk => Op::Mul,
        TokenType::Slash => Op::Div,
        TokenType::DoubleSlash => Op::Idiv,
        TokenType::Percent => Op::Mod,
        TokenType::DoubleAsterisk => Op::Pow,
        TokenType::BAnd => Op::Band,
        TokenType::BOr => Op::Bor,
        TokenType::BXor => Op::Bxor,
        TokenType::LeftShift => Op::Lsh,
        TokenType::RightShift => Op::Rsh,
        TokenType::Equal => Op::Eq,
        TokenType::Unequal => Op::Ne,
        TokenType::LessThan => Op::Lt,
        TokenType::GreaterThan => Op::Gt,
        TokenType::LessOrEqual => Op::Le,
        TokenType::GreaterOrEqual => Op::Ge,
        TokenType::Is => Op::Is,
        TokenType::IsNot => Op::IsNot,
        TokenType::In => Op::In,
        TokenType::Xor => Op::Xor,
        TokenType::DotDot => Op::Range,
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests;
