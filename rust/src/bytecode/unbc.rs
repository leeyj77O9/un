use crate::bytecode::instr::Instr;
use crate::bytecode::{
    Constant, ExcEntry, ExcKind, Param, Pattern, Program, Proto, MAGIC, MAX_PATTERN_DEPTH, VERSION,
};
const NONE: u32 = u32::MAX;
// Header flags. Bit 0 = embedded source (historical). Bit 1 = every proto
// record carries a trailing declared-return-type name index. Old images only
// ever set bit 0, so readers can tell the two layouts apart without bumping
// VERSION or changing any Op value.
const FLAG_SOURCE: u16 = 1;
const FLAG_DECLARED_RET: u16 = 2;
const CONST_INT: u8 = 0;
const CONST_FLOAT: u8 = 1;
const CONST_STR: u8 = 2;
const CONST_BOOL: u8 = 3;
const CONST_NONE: u8 = 4;
const CONST_NULL: u8 = 5;
const PATTERN_WILDCARD: u8 = 0;
const PATTERN_BIND: u8 = 1;
const PATTERN_LITERAL: u8 = 2;
const PATTERN_TUPLE: u8 = 3;
const PATTERN_VARIANT: u8 = 4;
// Additive extension: 0..=4 keep their historical values so existing .unbc files
// remain readable. New tags are appended after the reserved opcode-independent range.
const PATTERN_OR: u8 = 5;
const PATTERN_CLASS: u8 = 6;
const PATTERN_FIELDS: u8 = 7;
// Type-name patterns are a purely additive extension: tag 8 is new, so older
// .unbc images (which never contain it) still round-trip at VERSION 1.
const PATTERN_TYPE: u8 = 8;
const EXC_CATCH: u8 = 0;
const EXC_FINALLY: u8 = 1;
const EXC_PROPAGATE: u8 = 2;
pub fn write(program: &Program) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());

    let flags: u16 = if program.source.is_some() { FLAG_SOURCE } else { 0 }
        | FLAG_DECLARED_RET;
    out.extend_from_slice(&flags.to_le_bytes());
    write_string(&mut out, &program.source_path);
    if let Some(source) = &program.source {
        write_string(&mut out, source);
    }

    out.extend_from_slice(&(program.names.len() as u32).to_le_bytes());
    for name in &program.names {
        write_string(&mut out, name);
    }

    out.extend_from_slice(&(program.consts.len() as u32).to_le_bytes());
    for value in &program.consts {
        match value {
            Constant::Int(v) => {
                out.push(CONST_INT);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Constant::Float(v) => {
                out.push(CONST_FLOAT);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Constant::Str(v) => {
                out.push(CONST_STR);
                write_string(&mut out, v);
            }
            Constant::Bool(v) => {
                out.push(CONST_BOOL);
                out.push(if *v { 1 } else { 0 });
            }
            Constant::None => out.push(CONST_NONE),
            Constant::Null => out.push(CONST_NULL),
        }
    }

    out.extend_from_slice(&(program.msgs.len() as u32).to_le_bytes());
    for message in &program.msgs {
        write_string(&mut out, message);
    }

    out.extend_from_slice(&(program.protos.len() as u32).to_le_bytes());
    for proto in &program.protos {
        write_proto(&mut out, proto);
    }
    out
}
fn write_string(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}
fn write_proto(out: &mut Vec<u8>, proto: &Proto) {
    out.extend_from_slice(&(proto.name as u32).to_le_bytes());
    out.extend_from_slice(&(proto.arity as u32).to_le_bytes());
    out.extend_from_slice(&proto.flags.to_le_bytes());

    out.extend_from_slice(&(proto.locals.len() as u32).to_le_bytes());
    for slot in &proto.locals {
        out.extend_from_slice(&(*slot as u32).to_le_bytes());
    }

    out.extend_from_slice(&(proto.params.len() as u32).to_le_bytes());
    for param in &proto.params {
        out.extend_from_slice(&(param.name as u32).to_le_bytes());
        match param.type_name {
            Some(index) => out.extend_from_slice(&(index as u32).to_le_bytes()),
            None => out.extend_from_slice(&NONE.to_le_bytes()),
        }
        out.extend_from_slice(&param.flags.to_le_bytes());
        match param.default {
            Some(index) => out.extend_from_slice(&(index as u32).to_le_bytes()),
            None => out.extend_from_slice(&NONE.to_le_bytes()),
        }
    }

    out.extend_from_slice(&(proto.code.len() as u32).to_le_bytes());
    for instr in &proto.code {
        out.extend_from_slice(&instr.encode());
    }

    for table in [&proto.consts, &proto.names, &proto.protos] {
        out.extend_from_slice(&(table.len() as u32).to_le_bytes());
        for index in table {
            out.extend_from_slice(&(*index as u32).to_le_bytes());
        }
    }

    out.extend_from_slice(&(proto.call_names.len() as u32).to_le_bytes());
    for names in &proto.call_names {
        out.extend_from_slice(&(names.len() as u32).to_le_bytes());
        for name in names {
            out.extend_from_slice(&(*name as u32).to_le_bytes());
        }
    }

    out.extend_from_slice(&(proto.source_map.len() as u32).to_le_bytes());
    for (start, length) in &proto.source_map {
        out.extend_from_slice(&(*start as u32).to_le_bytes());
        out.extend_from_slice(&(*length as u32).to_le_bytes());
    }

    out.extend_from_slice(&(proto.exc.len() as u32).to_le_bytes());
    for entry in &proto.exc {
        let kind = match entry.kind {
            ExcKind::Catch => EXC_CATCH,
            ExcKind::Finally => EXC_FINALLY,
            ExcKind::Propagate => EXC_PROPAGATE,
        };
        out.push(kind);
        out.extend_from_slice(&(entry.start as u32).to_le_bytes());
        out.extend_from_slice(&(entry.end as u32).to_le_bytes());
        out.extend_from_slice(&(entry.handler as u32).to_le_bytes());
        match entry.var_slot {
            Some(slot) => out.extend_from_slice(&(slot as u32).to_le_bytes()),
            None => out.extend_from_slice(&NONE.to_le_bytes()),
        }
    }

    out.extend_from_slice(&(proto.patterns.len() as u32).to_le_bytes());
    for pattern in &proto.patterns {
        write_pattern(out, pattern);
    }

    match proto.ext_type {
        Some(index) => out.extend_from_slice(&(index as u32).to_le_bytes()),
        None => out.extend_from_slice(&NONE.to_le_bytes()),
    }
    // Additive tail field: present only in images with FLAG_DECLARED_RET.
    match proto.declared_ret {
        Some(index) => out.extend_from_slice(&(index as u32).to_le_bytes()),
        None => out.extend_from_slice(&NONE.to_le_bytes()),
    }
}
fn write_pattern(out: &mut Vec<u8>, pattern: &Pattern) {
    match pattern {
        Pattern::Wildcard => out.push(PATTERN_WILDCARD),
        Pattern::Bind(name) => {
            out.push(PATTERN_BIND);
            out.extend_from_slice(&(*name as u32).to_le_bytes());
        }
        Pattern::Literal(index) => {
            out.push(PATTERN_LITERAL);
            out.extend_from_slice(&(*index as u32).to_le_bytes());
        }
        Pattern::Tuple(items) => {
            out.push(PATTERN_TUPLE);
            out.extend_from_slice(&(items.len() as u32).to_le_bytes());
            for item in items {
                write_pattern(out, item);
            }
        }
        Pattern::Variant(enum_name, variant_name, inner) => {
            out.push(PATTERN_VARIANT);
            out.extend_from_slice(&(*enum_name as u32).to_le_bytes());
            out.extend_from_slice(&(*variant_name as u32).to_le_bytes());
            write_pattern(out, inner);
        }
        Pattern::Or(alternatives) => {
            out.push(PATTERN_OR);
            out.extend_from_slice(&(alternatives.len() as u32).to_le_bytes());
            for alt in alternatives {
                write_pattern(out, alt);
            }
        }
        Pattern::Class(name, inner) => {
            out.push(PATTERN_CLASS);
            out.extend_from_slice(&(*name as u32).to_le_bytes());
            write_pattern(out, inner);
        }
        Pattern::Fields(fields) => {
            out.push(PATTERN_FIELDS);
            out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
            for (name, pat) in fields {
                match name {
                    Some(name) => {
                        out.push(1);
                        out.extend_from_slice(&(*name as u32).to_le_bytes());
                    }
                    None => out.push(0),
                }
                write_pattern(out, pat);
            }
        }
        Pattern::Type(name) => {
            out.push(PATTERN_TYPE);
            out.extend_from_slice(&(*name as u32).to_le_bytes());
        }
    }
}
pub fn read(bytes: &[u8]) -> Result<Program, String> {
    let mut reader = Reader { bytes, pos: 0 };
    let magic = reader.take(4)?;
    if magic != MAGIC {
        return Err("not a .unbc file".to_string());
    }
    let version = reader.u16()?;
    if version != VERSION {
        return Err(format!("unsupported .unbc version {version}"));
    }
    let flags = reader.u16()?;
    if flags & !(FLAG_SOURCE | FLAG_DECLARED_RET) != 0 {
        return Err(format!("unsupported .unbc flags {flags:#06x}"));
    }
    let source_path = reader.string()?;
    let source = if flags & FLAG_SOURCE != 0 {
        Some(reader.string()?)
    } else {
        None
    };
    let name_count = reader.u32()? as usize; reader.guard(name_count)?;
    let mut names = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        names.push(reader.string()?);
    }
    let const_count = reader.u32()? as usize; reader.guard(const_count)?;
    let mut consts = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        consts.push(read_constant(&mut reader)?);
    }
    let msg_count = reader.u32()? as usize; reader.guard(msg_count)?;
    let mut msgs = Vec::with_capacity(msg_count);
    for _ in 0..msg_count {
        msgs.push(reader.string()?);
    }
    let proto_count = reader.u32()? as usize; reader.guard(proto_count)?;
    let mut protos = Vec::with_capacity(proto_count);
    // Old images (flags bit 1 clear) have no trailing declared-ret field;
    // readers default it to None without changing VERSION.
    let has_declared_ret = flags & FLAG_DECLARED_RET != 0;
    for _ in 0..proto_count {
        protos.push(read_proto(&mut reader, has_declared_ret)?);
    }
    let program = Program {
        source_path,
        source,
        names,
        consts,
        protos,
        msgs,
        root: 0,
        id: crate::bytecode::next_program_id(),
    };
    if reader.remaining() != 0 {
        return Err(format!(
            "trailing garbage in .unbc: {} byte(s) after the last proto",
            reader.remaining()
        ));
    }
    program.validate()?;
    Ok(program)
}
fn read_constant(reader: &mut Reader) -> Result<Constant, String> {
    let tag = reader.u8()?;
    Ok(match tag {
        CONST_INT => Constant::Int(reader.i64()?),
        CONST_FLOAT => Constant::Float(reader.f64()?),
        CONST_STR => Constant::Str(reader.string()?),
        CONST_BOOL => Constant::Bool(reader.u8()? != 0),
        CONST_NONE => Constant::None,
        CONST_NULL => Constant::Null,
        other => return Err(format!("unknown constant tag {other}")),
    })
}
fn read_proto(reader: &mut Reader, has_declared_ret: bool) -> Result<Proto, String> {
    let name = reader.u32()? as usize;
    let arity = reader.u32()? as usize;
    let flags = reader.u16()?;
    let local_count = reader.u32()? as usize; reader.guard(local_count)?;
    let mut locals = Vec::with_capacity(local_count);
    for _ in 0..local_count {
        locals.push(reader.u32()? as usize);
    }
    let param_count = reader.u32()? as usize; reader.guard(param_count)?;
    let mut params = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        let param_name = reader.u32()? as usize;
        let type_name = reader.optional_u32()?;
        let param_flags = reader.u16()?;
        let default = reader.optional_u32()?;
        params.push(Param {
            name: param_name,
            type_name,
            flags: param_flags,
            default,
        });
    }
    let code_count = reader.u32()? as usize; reader.guard(code_count)?;
    let mut code = Vec::with_capacity(code_count);
    for _ in 0..code_count {
        let mut bytes = [0u8; 7];
        bytes.copy_from_slice(reader.take(7)?);
        let instr = Instr::decode(&bytes);
        if instr.op_code().is_none() {
            return Err(format!("unknown opcode {:#04x}", instr.op));
        }
        code.push(instr);
    }
    let consts = read_table(reader)?;
    let names = read_table(reader)?;
    let protos = read_table(reader)?;
    let call_count = reader.u32()? as usize; reader.guard(call_count)?;
    let mut call_names = Vec::with_capacity(call_count);
    for _ in 0..call_count {
        let count = reader.u32()? as usize;
        reader.guard(count)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(reader.u32()? as i32);
        }
        call_names.push(entries);
    }
    let map_count = reader.u32()? as usize; reader.guard(map_count)?;
    let mut source_map = Vec::with_capacity(map_count);
    for _ in 0..map_count {
        let start = reader.u32()? as usize;
        let length = reader.u32()? as usize;
        source_map.push((start, length));
    }
    let exc_count = reader.u32()? as usize; reader.guard(exc_count)?;
    let mut exc = Vec::with_capacity(exc_count);
    for _ in 0..exc_count {
        let kind = match reader.u8()? {
            EXC_CATCH => ExcKind::Catch,
            EXC_FINALLY => ExcKind::Finally,
            EXC_PROPAGATE => ExcKind::Propagate,
            other => return Err(format!("unknown exception kind {other}")),
        };
        let start = reader.u32()? as usize;
        let end = reader.u32()? as usize;
        let handler = reader.u32()? as usize;
        let var_slot = reader.optional_u32()?;
        exc.push(ExcEntry {
            kind,
            start,
            end,
            handler,
            var_slot,
        });
    }
    let pattern_count = reader.u32()? as usize; reader.guard(pattern_count)?;
    let mut patterns = Vec::with_capacity(pattern_count);
    for _ in 0..pattern_count {
        patterns.push(read_pattern(reader)?);
    }
    let ext_type = reader.optional_u32()?;
    let declared_ret = if has_declared_ret {
        reader.optional_u32()?
    } else {
        None
    };
    Ok(Proto {
        name,
        arity,
        locals,
        params,
        code,
        consts,
        names,
        protos,
        call_names,
        source_map,
        exc,
        patterns,
        flags,
        ext_type,
        declared_ret,
        fn_span: None,
    })
}
fn read_pattern(reader: &mut Reader) -> Result<Pattern, String> {
    read_pattern_depth(reader, 0)
}
fn read_pattern_depth(reader: &mut Reader, depth: usize) -> Result<Pattern, String> {
    if depth > MAX_PATTERN_DEPTH {
        return Err(format!("pattern nesting exceeds {MAX_PATTERN_DEPTH}"));
    }
    let tag = reader.u8()?;
    Ok(match tag {
        PATTERN_WILDCARD => Pattern::Wildcard,
        PATTERN_BIND => Pattern::Bind(reader.u32()? as usize),
        PATTERN_LITERAL => Pattern::Literal(reader.u32()? as usize),
        PATTERN_TUPLE => {
            let count = reader.u32()? as usize;
            reader.guard(count)?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(read_pattern_depth(reader, depth + 1)?);
            }
            Pattern::Tuple(items)
        }
        PATTERN_VARIANT => {
            let enum_name = reader.u32()? as usize;
            let variant_name = reader.u32()? as usize;
            let inner = read_pattern_depth(reader, depth + 1)?;
            Pattern::Variant(enum_name, variant_name, Box::new(inner))
        }
        PATTERN_OR => {
            let count = reader.u32()? as usize;
            reader.guard(count)?;
            let mut alternatives = Vec::with_capacity(count);
            for _ in 0..count {
                alternatives.push(read_pattern_depth(reader, depth + 1)?);
            }
            Pattern::Or(alternatives)
        }
        PATTERN_CLASS => {
            let name = reader.u32()? as usize;
            let inner = read_pattern_depth(reader, depth + 1)?;
            Pattern::Class(name, Box::new(inner))
        }
        PATTERN_FIELDS => {
            let count = reader.u32()? as usize;
            reader.guard(count)?;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                let named = reader.u8()?;
                let name = match named {
                    0 => None,
                    1 => Some(reader.u32()? as usize),
                    other => return Err(format!("unknown pattern field tag {other}")),
                };
                fields.push((name, read_pattern_depth(reader, depth + 1)?));
            }
            Pattern::Fields(fields)
        }
        PATTERN_TYPE => Pattern::Type(reader.u32()? as usize),
        other => return Err(format!("unknown pattern tag {other}")),
    })
}
fn read_table(reader: &mut Reader) -> Result<Vec<usize>, String> {
    let count = reader.u32()? as usize;
    reader.guard(count)?;
    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        table.push(reader.u32()? as usize);
    }
    Ok(table)
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    fn guard(&self, count: usize) -> Result<(), String> {
        if count > self.remaining() {
            return Err(format!(
                "declared element count {count} exceeds the {} remaining bytes",
                self.remaining()
            ));
        }
        Ok(())
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        if self.pos + count > self.bytes.len() {
            return Err("unexpected end of the .unbc file".to_string());
        }
        let slice = &self.bytes[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    fn i64(&mut self) -> Result<i64, String> {
        let bytes = self.take(8)?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(i64::from_le_bytes(raw))
    }
    fn f64(&mut self) -> Result<f64, String> {
        let bytes = self.take(8)?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(f64::from_le_bytes(raw))
    }
    fn string(&mut self) -> Result<String, String> {
        let length = self.u32()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "the .unbc file has an invalid string".to_string())
    }
    fn optional_u32(&mut self) -> Result<Option<usize>, String> {
        let value = self.u32()?;
        if value == NONE {
            Ok(None)
        } else {
            Ok(Some(value as usize))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::instr::Op;

    fn sample() -> Program {
        let mut program = Program {
            source_path: "test.un".to_string(),
            source: Some("x = 1 + 2\nmore source text here".to_string()),
            names: vec![
                "<module>".to_string(),
                "io".to_string(),
                "x".to_string(),
                "int".to_string(),
            ],
            consts: vec![
                Constant::Int(1),
                Constant::Float(1.5),
                Constant::Str("text".to_string()),
                Constant::Bool(true),
                Constant::None,
                Constant::Null,
            ],
            id: crate::bytecode::next_program_id(),
            protos: Vec::new(),
            msgs: vec!["unsupported node kind Match".to_string()],
            root: 0,
        };
        let mut proto = Proto::new(0, 0);
        proto.arity = 1;
        proto.locals = vec![1];
        proto.params = vec![Param {
            name: 2,
            type_name: Some(1),
            flags: 1,
            default: Some(0),
        }];
        proto.code = vec![
            Instr::new(Op::LoadConst, 0),
            Instr::with_flags(Op::JmpIf, 2, 0),
            Instr::new(Op::LoadNone, 0),
            Instr::new(Op::Return, 0),
        ];
        proto.consts = vec![0, 1, 2];
        proto.names = vec![1, 2];
        proto.protos = Vec::new();
        proto.call_names = vec![vec![-1, 1]];
        proto.source_map = vec![(0, 4), (5, 2), (7, 3), (9, 4)];
        proto.exc = vec![ExcEntry {
            kind: ExcKind::Catch,
            start: 0,
            end: 2,
            handler: 1,
            var_slot: Some(1),
        }];
        proto.patterns = vec![
            Pattern::Wildcard,
            Pattern::Bind(2),
            Pattern::Literal(0),
            Pattern::Tuple(vec![Pattern::Wildcard, Pattern::Bind(1)]),
            Pattern::Or(vec![
                Pattern::Literal(1),
                Pattern::Variant(0, 1, Box::new(Pattern::Bind(2))),
            ]),
            Pattern::Class(
                0,
                Box::new(Pattern::Fields(vec![
                    (Some(1), Pattern::Wildcard),
                    (None, Pattern::Bind(2)),
                ])),
            ),
        ];
        proto.ext_type = Some(1);
        proto.declared_ret = Some(3);
        program.protos.push(proto);
        program
    }
    #[test]
    fn the_container_round_trips() {
        let program = sample();
        let bytes = write(&program);
        let read_back = read(&bytes).expect("read");
        assert_eq!(read_back.source_path, program.source_path);
        assert_eq!(read_back.source, program.source);
        assert_eq!(read_back.names, program.names);
        assert_eq!(read_back.consts, program.consts);
        assert_eq!(read_back.msgs, program.msgs);
        assert_eq!(read_back.protos.len(), program.protos.len());
        let proto = &read_back.protos[0];
        let original = &program.protos[0];
        assert_eq!(proto.code, original.code);
        assert_eq!(proto.params, original.params);
        assert_eq!(proto.consts, original.consts);
        assert_eq!(proto.names, original.names);
        assert_eq!(proto.call_names, original.call_names);
        assert_eq!(proto.source_map, original.source_map);
        assert_eq!(proto.exc, original.exc);
        assert_eq!(proto.patterns, original.patterns);
        assert_eq!(proto.ext_type, original.ext_type);
        assert_eq!(proto.declared_ret, original.declared_ret);
    }
    #[test]
    fn old_images_without_declared_ret_read_as_none() {
        let program = sample();
        let mut bytes = write(&program);
        // Strip one additive u32 per proto and clear the header flag that
        // marks its presence — exactly the layout produced before this change.
        bytes.truncate(bytes.len() - 4 * program.protos.len());
        // Header layout: MAGIC[0..4], VERSION[4..6], flags[6..8].
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        bytes[6..8].copy_from_slice(&(flags & !FLAG_DECLARED_RET).to_le_bytes());
        let read_back = read(&bytes).expect("read legacy image");
        assert!(read_back.protos[0].declared_ret.is_none());
        assert_eq!(read_back.protos[0].code, program.protos[0].code);
        assert_eq!(read_back.protos[0].ext_type, program.protos[0].ext_type);
        assert_eq!(read_back.names, program.names);
    }
    #[test]
    fn compiler_records_declared_return_types() {
        let text = "fn add(a: int, b: int) -> int\n    -> a + b\n\nfn scaled(x: int) -> float\n    -> x * 1.5\n";
        let mut source = crate::source::Source::new("test.un", text);
        let tokens = crate::lexer::lex(&mut source).expect("lex");
        let source = std::rc::Rc::new(source);
        let node = crate::parser::parse(tokens, source.clone()).expect("parse");
        let root = crate::optimize::optimize(&node).expect("optimize").root;
        let program = crate::compiler::compile(&source, &root).expect("compile");
        let declared: Vec<&str> = program
            .protos
            .iter()
            .filter_map(|p| p.declared_ret.map(|i| program.names[i].as_str()))
            .collect();
        assert!(declared.contains(&"int"), "{declared:?}");
        assert!(declared.contains(&"float"), "{declared:?}");

        let read_back = read(&write(&program)).expect("round trip");
        let back: Vec<&str> = read_back
            .protos
            .iter()
            .filter_map(|p| p.declared_ret.map(|i| read_back.names[i].as_str()))
            .collect();
        assert_eq!(back, declared);
    }
    #[test]
    fn an_out_of_range_declared_ret_is_rejected() {
        let mut program = sample();
        program.protos[0].declared_ret = Some(program.names.len());
        let bytes = write(&program);
        assert!(read(&bytes).is_err());
    }
    #[test]
    fn a_foreign_magic_is_rejected() {
        let mut bytes = write(&sample());
        bytes[0] = b'X';
        assert!(read(&bytes).is_err());
    }
    #[test]
    fn a_truncated_file_is_rejected() {
        let bytes = write(&sample());
        assert!(read(&bytes[..bytes.len() - 3]).is_err());
    }
}
