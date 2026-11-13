use crate::ast::{Const, Node, NodeKind};
use crate::bytecode::instr::*;
use crate::bytecode::{Constant, ExcEntry, ExcKind, Param, Pattern, Program, Proto};
use crate::error::Panic;
use crate::source::Source;
use crate::token::TokenType;
#[derive(Debug, Default)]
pub struct Group {
    pub breaks: Vec<usize>,
    pub skips: Vec<usize>,
    pub is_loop: bool,
}
pub struct Compiler<'a> {
    pub source: &'a Source,
    pub program: Program,

    pub current: usize,

    pub proto_stack: Vec<usize>,

    pub groups: Vec<Group>,

    pub raises: bool,
}
pub fn compile(source: &Source, ast: &Node) -> Result<Program, Panic> {
    let mut c = Compiler::new(source);
    c.code_block(ast, false, true)?;
    c.finish()
}
impl<'a> Compiler<'a> {
    pub fn new(source: &'a Source) -> Self {
        let mut program = Program {
            source_path: source.name().to_string(),
            source: None,
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
            current: 0,
            proto_stack: Vec::new(),
            groups: Vec::new(),
            raises: false,
        }
    }

    pub fn finish(self) -> Result<Program, Panic> {
        if !self.proto_stack.is_empty() {
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
        self.current
    }
    fn ip(&self) -> usize {
        self.program.protos[self.current].code.len()
    }

    fn begin_proto(&mut self, name: &str, ext_type: Option<&str>) -> usize {
        let name_index = push_name(&mut self.program, name);
        let mut proto = Proto::new(name_index, PROTO_FUNCTION);
        if let Some(ext) = ext_type {
            proto.ext_type = Some(push_name(&mut self.program, ext));
        }
        self.program.protos.push(proto);
        let index = self.program.protos.len() - 1;
        self.proto_stack.push(self.current);
        self.current = index;
        index
    }

    fn end_proto(&mut self) -> usize {
        let index = self.current;
        self.current = self.proto_stack.pop().expect("proto stack");
        let parent = self.current;
        self.program.protos[parent].protos.push(index);
        self.program.protos[parent].protos.len() - 1
    }

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

    fn push_name_raw(&mut self, name: &str) -> usize {
        let p = self.current();
        let gi = push_name(&mut self.program, name);
        self.program.protos[p].names.push(gi);
        self.program.protos[p].names.len() - 1
    }

    fn push_names_contiguous(&mut self, names: &[String]) -> usize {
        let first = self.push_name_raw(&names[0]);
        for name in &names[1..] {
            self.push_name_raw(name);
        }
        first
    }

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

    fn patch(&mut self, ip: usize, target: usize) {
        let p = self.current();
        self.program.protos[p].code[ip].operand = (target as i64 - ip as i64 - 1) as i32;
    }

    fn add_exc(&mut self, entry: ExcEntry) -> usize {
        let p = self.current();
        self.program.protos[p].exc.push(entry);
        self.program.protos[p].exc.len() - 1
    }

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

                        self.emit(Op::ToStr, 0, 0, span);
                    }
                }
                self.emit(Op::FString, 0, node.children.len() as i32, span);
            }
            NodeKind::Tagged => self.code_tagged(node)?,
            NodeKind::Tuple => self.code_tuple(node)?,
            NodeKind::List => self.code_container(node, Op::BuildList)?,

            NodeKind::Set => self.code_container(node, Op::BuildSet)?,
            NodeKind::Spread | NodeKind::KwSpread => self.unsupported(node),
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
            NodeKind::Block => self.code_block(node, false, true)?,
            NodeKind::Use => self.code_use(node)?,
            NodeKind::Function => self.code_function(node)?,
            NodeKind::Extension => self.code_extension(node)?,
            NodeKind::Lambda => self.code_lambda(node)?,
            NodeKind::Return => self.code_return(node)?,
            NodeKind::If => self.code_if(node)?,
            NodeKind::While => self.code_while(node)?,
            NodeKind::For => self.code_for(node)?,
            NodeKind::Try => self.code_try(node)?,
            NodeKind::Match => self.code_match(node)?,
            NodeKind::Defer => self.code_defer(node)?,
            NodeKind::Using => self.code_using(node)?,
            NodeKind::Class => self.code_class(node)?,
            NodeKind::Enum => self.code_enum(node)?,
            NodeKind::Break => self.code_break(node),
            NodeKind::Skip => self.code_skip(node),
            _ => self.unsupported(node),
        }
        Ok(())
    }

    pub fn code_block(&mut self, node: &Node, pop_last: bool, unwrap: bool) -> Result<(), Panic> {
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

            if self.raises {
                continue;
            }
            if unwrap {
                self.emit(Op::Unwrap, 0, 0, span_of(child));
            }
            if i + 1 < n || pop_last {
                self.emit(Op::Pop, 0, 0, span_of(child));
            }
        }
        Ok(())
    }

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

                self.code_pattern(&pattern.children[0], local_only)?;
            }
            NodeKind::Wildcard => {
                self.emit(Op::Pop, 0, 0, span);
            }
            NodeKind::Tuple => {
                let count = pattern.children.len() as i32;
                self.emit(Op::Unpack, 0, count, span);

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

    fn code_if(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let mut end_jumps = Vec::new();
        let mut next_jumps: Vec<usize> = Vec::new();
        let mut has_else = false;
        let mut all_raise = true;
        for child in &node.children {

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

        let jend = self.emit_jump(Op::JmpIf, FLAG_JUMP_POP, span_of(cond));
        self.open_group(true);
        self.code_block(body, true, true)?;
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
        self.code_block(body, true, true)?;
        self.emit(Op::Jmp, 0, (loop_start as i32) - (self.ip() as i32 + 1), span_of(body));
        let exhausted = self.ip();
        self.patch(jend, exhausted);

        self.emit(Op::PopScope, 0, 0, span);
        let jafter = self.emit_jump(Op::Jmp, 0, span);
        let break_cleanup = self.ip();

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

    fn code_try(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let try_block = &node.children[0];
        let mut catch: Option<&Node> = None;
        let mut finally_block: Option<&Node> = None;
        for child in node.children.iter().skip(1) {
            match child.kind {
                NodeKind::Catch => {
                    if catch.is_none() {
                        catch = Some(child);
                    }
                }
                NodeKind::Finally => finally_block = Some(&child.children[0]),
                _ => {}
            }
        }

        let push_finally = self.ip();
        if finally_block.is_some() {
            self.emit(Op::PushTry, 0, 0, span);
        }
        let push_catch = self.ip();
        if catch.is_some() {
            self.emit(Op::PushTry, 0, 0, span);
        }
        let region_start = self.ip();
        self.code_block(try_block, false, true)?;
        let region_end = self.ip();
        if catch.is_some() {
            self.emit(Op::PopTry, 0, 0, span);
        }
        if finally_block.is_some() {
            self.emit(Op::PopTry, 0, 0, span);
        }
        if let Some(finally) = finally_block {
            self.code_block(finally, true, true)?;
        }

        let mut jumps = vec![self.emit_jump(Op::Jmp, 0, span)];
        if let Some(catch) = catch {
            let handler = self.ip();

            let (var_slot, block) = if catch.children.len() == 2 {
                let name = self.name_index(&self.text(&catch.children[0]));
                (Some(name), &catch.children[1])
            } else {
                (None, &catch.children[0])
            };
            self.code_block(block, false, true)?;
            self.emit(Op::PopScope, 0, 0, span);
            if finally_block.is_some() {
                self.emit(Op::PopTry, 0, 0, span);
                let finally = finally_block.expect("finally block");
                self.code_block(finally, true, true)?;
            }
            jumps.push(self.emit_jump(Op::Jmp, 0, span));
            let entry = ExcEntry {
                kind: ExcKind::Catch,
                start: region_start,
                end: region_end,
                handler,
                var_slot,
            };
            let index = self.add_exc(entry);
            self.patch_index(push_catch, index);
        }
        if let Some(finally) = finally_block {
            let handler = self.ip();
            self.code_block(finally, true, true)?;
            self.emit(Op::Raise, 0, 0, span);
            let entry = ExcEntry {
                kind: ExcKind::Finally,
                start: region_start,
                end: region_end,
                handler,
                var_slot: None,
            };
            let index = self.add_exc(entry);
            self.patch_index(push_finally, index);
        }

        let end = self.ip();
        for jump in jumps {
            self.patch(jump, end);
        }
        Ok(())
    }

    fn code_defer(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let block = &node.children[0];

        self.begin_proto("defer", None);
        self.code_function_body(block)?;
        let proto_slot = self.end_proto();
        self.emit(Op::PushDefer, 0, proto_slot as i32, span);

        self.emit(Op::LoadNone, 0, 0, span);
        Ok(())
    }

    fn code_using(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        self.code(&node.children[0])?;
        self.emit(Op::Dup, 0, 0, span);
        self.emit(Op::PushUsing, 0, 0, span);
        Ok(())
    }

    fn code_tagged(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let tag = &node.children[0];
        let template = &node.children[1];

        self.code(tag)?;
        let mut values: Vec<&Node> = Vec::new();
        let mut strings: Vec<String> = Vec::new();
        for child in &template.children {
            if child.kind == NodeKind::String {
                strings.push(self.text(child));
            } else {
                values.push(child);
            }
        }

        self.emit(Op::ArgBegin, 0, values.len() as i32, span);
        for child in &values {
            self.code(child)?;
            self.emit(Op::ArgPos, 0, 0, span_of(child));
        }
        self.emit(Op::BuildList, 0, values.len() as i32, span);

        self.emit(Op::ArgBegin, 0, strings.len() as i32, span);
        for text in &strings {
            let ci = self.const_index(Constant::Str(text.clone()));
            self.emit(Op::LoadConst, 0, ci as i32, span);
            self.emit(Op::ArgPos, 0, 0, span);
        }
        self.emit(Op::BuildList, 0, strings.len() as i32, span);

        self.emit(Op::ArgBegin, 0, 2, span);
        self.emit(Op::ArgPos, 0, 0, span);
        self.emit(Op::ArgPos, 0, 0, span);
        self.emit(Op::Call, 0, 2, span);
        Ok(())
    }

    fn code_match(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let value = &node.children[0];

        self.code(value)?;
        let mut ends: Vec<usize> = Vec::new();
        for case in node.children.iter().skip(1) {
            if case.kind != NodeKind::Case {
                continue;
            }
            let pattern = &case.children[0];
            let case_body = &case.children[1];

            self.emit(Op::NewScope, 0, 0, span);
            let compiled = self.compile_pattern(pattern)?;
            let pattern_index = self.add_pattern(compiled);
            self.emit(Op::MatchPattern, 0, pattern_index as i32, span);

            let fail = self.emit_jump(Op::JmpIf, FLAG_JUMP_POP, span);

            self.emit(Op::Pop, 0, 0, span);
            self.code(case_body)?;
            self.emit(Op::PopScope, 0, 0, span);
            ends.push(self.emit_jump(Op::Jmp, 0, span));

            self.patch(fail, self.ip());
            self.emit(Op::PopScope, 0, 0, span);
        }

        let message = "no match case";
        let mi = self.msg_index(message);
        self.emit(Op::Unsupported, 0, mi as i32, span);
        let end = self.ip();
        for jump in ends {
            self.patch(jump, end);
        }
        Ok(())
    }

    fn compile_pattern(&mut self, pattern: &Node) -> Result<Pattern, Panic> {
        let compiled = match pattern.kind {
            NodeKind::Wildcard => Pattern::Wildcard,
            NodeKind::Identifier => {
                let name = self.text(pattern);
                Pattern::Bind(push_name(&mut self.program, &name))
            }

            NodeKind::Typed => {
                let name = self.text(&pattern.children[0]);
                Pattern::Bind(push_name(&mut self.program, &name))
            }
            NodeKind::Tuple => {
                let mut items = Vec::new();
                for item in &pattern.children {
                    items.push(self.compile_pattern(item)?);
                }
                Pattern::Tuple(items)
            }
            NodeKind::Integer
            | NodeKind::Float
            | NodeKind::String
            | NodeKind::Boolean
            | NodeKind::None => {
                let value = match pattern.value.as_ref() {
                    Some(Const::Int(v)) => Constant::Int(*v),
                    Some(Const::Float(v)) => Constant::Float(*v),
                    Some(Const::Str(v)) => Constant::Str(v.clone()),
                    Some(Const::Bool(v)) => Constant::Bool(*v),
                    _ => Constant::None,
                };
                Pattern::Literal(push_const(&mut self.program, value))
            }
            _ => return Err(Panic::named("internal", "invalid pattern")),
        };
        Ok(compiled)
    }

    fn add_pattern(&mut self, pattern: Pattern) -> usize {
        let current = self.current;
        self.program.protos[current].patterns.push(pattern);
        self.program.protos[current].patterns.len() - 1
    }

    fn code_function(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let name = self.text(&node.children[0]);
        let params = &node.children[1];
        let body = &node.children[2];

        let decorated = node
            .annotations
            .as_ref()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let flags = if decorated { 0 } else { FLAG_FUNC_BIND };
        self.compile_function(&name, None, params, body, span, flags)?;
        self.apply_annotations(node, span)?;
        if decorated {
            let slot = self.name_index(&name);
            self.emit(Op::StoreName, FLAG_SCOPE_SET, slot as i32, span);
        }
        Ok(())
    }

    fn apply_annotations(&mut self, node: &Node, span: (usize, usize)) -> Result<(), Panic> {
        let Some(annotations) = node.annotations.as_ref() else {
            return Ok(());
        };
        for annotation in annotations.iter().rev() {

            self.code(&annotation.children[0])?;

            self.emit(Op::Swap, 0, 0, span);
            self.emit(Op::ArgBegin, 0, 1, span);
            self.emit(Op::ArgPos, 0, 0, span);
            self.emit(Op::Call, 0, 1, span);
        }
        Ok(())
    }

    fn code_extension(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let ext_type = self.text(&node.children[0]);
        let name = self.text(&node.children[1]);
        let params = &node.children[2];
        let body = &node.children[3];
        self.compile_function(&name, Some(&ext_type), params, body, span, FLAG_FUNC_EXT)?;
        self.apply_annotations(node, span)
    }

    fn code_lambda(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let params = &node.children[0];
        let body = &node.children[1];
        self.compile_function("lambda", None, params, body, span, 0)
    }

    fn compile_function(
        &mut self,
        name: &str,
        ext_type: Option<&str>,
        params: &Node,
        body: &Node,
        span: (usize, usize),
        flags: u16,
    ) -> Result<(), Panic> {
        let defaults = self.compile_defaults(params)?;

        let saved_raises = self.raises;
        let saved_groups = std::mem::take(&mut self.groups);
        self.begin_proto(name, ext_type);
        self.compile_params(params)?;
        self.code_function_body(body)?;
        let slot = self.end_proto();
        self.groups = saved_groups;
        self.raises = saved_raises;
        let flags = flags | ((defaults as u16) << 8);
        self.emit(Op::MakeFunc, flags, slot as i32, span);
        Ok(())
    }

    fn compile_defaults(&mut self, params: &Node) -> Result<usize, Panic> {
        let mut count = 0;
        for param in &params.children {
            let node = &param.children[0];
            if node.kind != NodeKind::Assign {
                continue;
            }
            let saved = self.raises;
            self.code(&node.children[1])?;
            self.raises = saved;
            count += 1;
        }
        Ok(count)
    }

    fn compile_params(&mut self, params: &Node) -> Result<(), Panic> {
        for param in &params.children {
            let node = &param.children[0];
            let (name, type_name, flags) = match node.kind {
                NodeKind::Identifier => (self.text(node), None, PARAM_ESSENTIAL),
                NodeKind::Typed => (
                    self.text(&node.children[0]),
                    Some(self.text(&node.children[1])),
                    PARAM_ESSENTIAL,
                ),
                NodeKind::Assign => {
                    let target = &node.children[0];
                    if target.kind == NodeKind::Typed {
                        (
                            self.text(&target.children[0]),
                            Some(self.text(&target.children[1])),
                            PARAM_OPTIONAL,
                        )
                    } else {
                        (self.text(target), None, PARAM_OPTIONAL)
                    }
                }
                NodeKind::Unary if node.op == Some(TokenType::Asterisk) => {
                    (self.text(&node.children[0]), None, PARAM_POSITIONAL)
                }
                NodeKind::Unary if node.op == Some(TokenType::DoubleAsterisk) => {
                    (self.text(&node.children[0]), None, PARAM_KEYWORD)
                }
                _ => return Err(Panic::named("internal", "invalid function parameter")),
            };
            let name_index = push_name(&mut self.program, &name);
            let type_index = type_name.map(|t| push_name(&mut self.program, &t));
            let current = self.current;
            self.program.protos[current].params.push(Param {
                name: name_index,
                type_name: type_index,
                flags,
                default: None,
            });
        }
        Ok(())
    }

    fn code_function_body(&mut self, body: &Node) -> Result<(), Panic> {
        let span = (body.start, body.length);

        self.code_block(body, true, false)?;
        self.emit(Op::LoadNone, 0, 0, span);
        self.emit(Op::Return, 0, 0, span);
        self.raises = true;
        Ok(())
    }

    fn code_class(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let class_name = self.text(&node.children[0]);
        let mut index = 1;
        let mut fields: Vec<String> = Vec::new();
        if index < node.children.len() && node.children[index].kind == NodeKind::Tuple {
            let parameters = &node.children[index];
            for param in &parameters.children {
                let value = &param.children[0];
                let name = match value.kind {
                    NodeKind::Identifier => self.text(value),
                    NodeKind::Typed => self.text(&value.children[0]),
                    _ => return Err(Panic::named("internal", "invalid initializer")),
                };
                fields.push(name);
            }
            index += 1;
        }
        let mut bases: Vec<String> = Vec::new();
        if index < node.children.len() && node.children[index].kind == NodeKind::ClassBases {
            for base in &node.children[index].children {
                bases.push(self.text(base));
            }
            index += 1;
        }
        let body = &node.children[index];

        let name_slot = self.push_name_raw(&class_name);
        for field in &fields {
            self.push_name_raw(field);
        }
        let mut flags = 0u16;
        if !fields.is_empty() {
            flags |= FLAG_CLASS_PARAMS;
        }
        flags |= (fields.len() as u16) << 8;
        self.emit(Op::MakeClass, flags, name_slot as i32, span);

        self.apply_annotations(node, span)?;

        let register_slot = self.push_name_raw(&class_name);
        self.emit(Op::RegisterClass, 0, register_slot as i32, span);

        if !bases.is_empty() {
            for base in &bases {
                let slot = self.name_index(base);
                self.emit(Op::LoadBaseClass, 0, slot as i32, span);
            }
            self.emit(Op::SetBases, 0, bases.len() as i32, span);
        }

        let mut accessors: Vec<(&Node, Option<&Node>, Option<&Node>)> = Vec::new();
        let mut accessor_names: Vec<String> = Vec::new();
        for member in &body.children {
            if member.kind != NodeKind::Accessor {
                continue;
            }
            let name = self.text(&member.children[0]);
            let kind = match member.value.as_ref() {
                Some(Const::Str(v)) => v.clone(),
                _ => String::new(),
            };
            let entry = match accessor_names.iter().position(|n| *n == name) {
                Some(pos) => &mut accessors[pos],
                None => {
                    accessor_names.push(name.clone());
                    accessors.push((member, None, None));
                    accessors.last_mut().expect("accessor")
                }
            };
            if kind == "get" {
                if entry.1.is_some() {
                    self.error_node(member, &format!("duplicate getter for property '{name}'"));
                    return Ok(());
                }
                entry.1 = Some(member);
            } else {
                if entry.2.is_some() {
                    self.error_node(member, &format!("duplicate setter for property '{name}'"));
                    return Ok(());
                }
                entry.2 = Some(member);
            }
        }

        for member in &body.children {
            if member.kind != NodeKind::Function {
                continue;
            }
            let name = self.text(&member.children[0]);
            if accessor_names.iter().any(|n| *n == name) {
                self.error_node(member, &format!("property '{name}' conflicts with a method"));
                return Ok(());
            }
        }

        for member in &body.children {
            match member.kind {
                NodeKind::Function => {
                    let member_name = self.text(&member.children[0]);
                    let slot = self.name_index(&member_name);

                    let saved_raises = self.raises;
                    let saved_groups = std::mem::take(&mut self.groups);
                    self.begin_proto(&member_name, None);
                    self.compile_params(&member.children[1])?;
                    self.code_function_body(&member.children[2])?;
                    let proto_slot = self.end_proto();
                    self.groups = saved_groups;
                    self.raises = saved_raises;
                    self.emit(Op::MakeFunc, 0, proto_slot as i32, span);
                    self.emit(Op::AddMethod, 0, slot as i32, span);
                }
                NodeKind::Assign => {
                    let member_name = self.text(&member.children[0]);

                    if accessor_names.iter().any(|n| *n == member_name) {
                        continue;
                    }
                    let slot = self.name_index(&member_name);
                    self.code(&member.children[1])?;
                    self.emit(Op::AddMember, 0, slot as i32, span);
                }
                _ => {}
            }
        }

        for (index, (_accessor, getter, setter)) in accessors.iter().enumerate() {
            let name = &accessor_names[index];
            let slot = self.name_index(name);

            let mut default = None;
            for member in &body.children {
                if member.kind == NodeKind::Assign && self.text(&member.children[0]) == *name {
                    default = Some(&member.children[1]);
                }
            }
            match default {
                Some(expr) => self.code(expr)?,
                None => self.emit(Op::LoadNone, 0, 0, span),
            }

            for accessor_node in [getter, setter] {
                match accessor_node {
                    Some(accessor_node) => {
                        let body = &accessor_node.children[1];
                        let saved_raises = self.raises;
                        let saved_groups = std::mem::take(&mut self.groups);
                        self.begin_proto(name, None);

                        if accessor_node.value.as_ref()
                            == Some(&Const::Str("set".to_owned()))
                        {
                            let value_slot = push_name(&mut self.program, "value");
                            self.program.protos[self.current].params.push(Param {
                                name: value_slot,
                                type_name: None,
                                flags: PARAM_ESSENTIAL,
                                default: None,
                            });
                        }
                        self.code_function_body(body)?;
                        let proto_slot = self.end_proto();
                        self.groups = saved_groups;
                        self.raises = saved_raises;
                        self.emit(Op::MakeFunc, 0, proto_slot as i32, span);
                    }
                    None => self.emit(Op::LoadNone, 0, 0, span),
                }
            }
            self.emit(Op::MakeProperty, 0, slot as i32, span);
            self.emit(Op::AddMember, 0, slot as i32, span);
        }
        Ok(())
    }

    fn code_enum(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let enum_name = self.text(&node.children[0]);
        let name_slot = self.push_name_raw(&enum_name);
        self.emit(Op::MakeEnum, 0, name_slot as i32, span);
        let register_slot = self.push_name_raw(&enum_name);
        self.emit(Op::RegisterClass, 0, register_slot as i32, span);
        let mut index = 1i64;
        for member in node.children.iter().skip(1) {
            let (member_name, value) = match member.kind {
                NodeKind::Assign => {
                    let name = self.text(&member.children[0]);
                    let value = match member.children[1].value.as_ref() {
                        Some(Const::Int(v)) => *v,
                        _ => return Err(Panic::named("internal", "invalid enum value")),
                    };
                    index = value + 1;
                    (name, value)
                }
                _ => {
                    let name = self.text(member);
                    let value = index;
                    index += 1;
                    (name, value)
                }
            };
            let slot = self.name_index(&member_name);
            let value_slot = self.const_index(Constant::Int(value));
            self.emit(Op::LoadConst, 0, value_slot as i32, span);
            self.emit(Op::AddMember, 0, slot as i32, span);
        }
        Ok(())
    }

    fn code_return(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        match node.children.first() {
            Some(child) => self.code(child)?,
            None => {

                let message = "System.ArgumentOutOfRangeException: Index was out of range. Must be non-negative and less than the size of the collection. (Parameter 'index')";
                let mi = self.msg_index(message);
                self.emit(Op::Unsupported, FLAG_HOST_ERROR, mi as i32, span);
                self.raises = true;
                return Ok(());
            }
        }
        self.emit(Op::Return, 0, 0, span);
        self.raises = true;
        Ok(())
    }

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

    fn code_binary(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let op = node.op.unwrap_or(TokenType::Error);
        match op {
            TokenType::And => {
                self.code(&node.children[0])?;
                self.emit(Op::Dup, 0, 0, span);
                self.emit(Op::ToBool, FLAG_STRICT, 0, span);
                let truthy = self.emit_jump(Op::JmpIf, FLAG_JUMP_TRUE | FLAG_JUMP_POP, span);

                let end = self.emit_jump(Op::Jmp, 0, span);

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

                let end = self.emit_jump(Op::Jmp, 0, span);

                let target = self.ip();
                self.patch(falsy, target);
                self.emit(Op::Pop, 0, 0, span);
                self.code(&node.children[1])?;
                self.patch(end, self.ip());
            }
            TokenType::DoubleQuestion => {
                self.code(&node.children[0])?;

                let right = self.emit_jump(
                    Op::JmpIf,
                    FLAG_JUMP_NONE | FLAG_JUMP_TRUE,
                    span,
                );
                let end = self.emit_jump(Op::Jmp, 0, span);
                let target = self.ip();
                self.patch(right, target);

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

        if op == TokenType::Go {
            let saved_raises = self.raises;
            let saved_groups = std::mem::take(&mut self.groups);
            self.begin_proto("go", None);
            self.code(&node.children[0])?;
            self.emit(Op::Return, 0, 0, span);
            let slot = self.end_proto();
            self.groups = saved_groups;
            self.raises = saved_raises;
            self.emit(Op::MakeFuture, 0, slot as i32, span);
            return Ok(());
        }

        if op == TokenType::Wait {
            self.code(&node.children[0])?;
            self.emit(Op::Await, 0, 0, span);
            return Ok(());
        }

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

    fn code_ternary(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        self.code(&node.children[0])?;
        self.emit(Op::ToBool, 0, 0, span);
        let truthy = self.emit_jump(Op::JmpIf, FLAG_JUMP_TRUE | FLAG_JUMP_POP, span);

        self.code(&node.children[2])?;
        let end = self.emit_jump(Op::Jmp, 0, span);
        let target = self.ip();
        self.patch(truthy, target);
        self.code(&node.children[1])?;
        self.patch(end, self.ip());
        Ok(())
    }

    fn code_call(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let args = node.children.get(1);
        self.code(&node.children[0])?;

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

    fn emit_call(&mut self, args: Option<&Node>, span: (usize, usize)) -> Result<(), Panic> {
        let count = args.map(|a| a.children.len()).unwrap_or(0);

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

                    self.emit(Op::ArgPos, FLAG_ARG_CHECK, 0, span_of(child));
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

    fn code_use(&mut self, node: &Node) -> Result<(), Panic> {
        let span = (node.start, node.length);
        let path = self.text(&node.children[0]);

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

        self.emit(Op::LoadNone, 0, 0, span);
        Ok(())
    }

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
fn push_const(program: &mut Program, value: Constant) -> usize {
    program.consts.push(value);
    program.consts.len() - 1
}
fn span_of(node: &Node) -> (usize, usize) {
    (node.start, node.length)
}
fn is_optional_chain(node: &Node) -> bool {
    match node.kind {
        NodeKind::Property => node.op == Some(TokenType::QuestionDot) || is_optional_chain(&node.children[0]),
        NodeKind::Call => is_optional_chain(&node.children[0]),
        NodeKind::Index => is_optional_chain(&node.children[0]),
        NodeKind::Slice => is_optional_chain(&node.children[0]),
        _ => false,
    }
}
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
