use crate::bytecode::instr::{Instr, Op};
use crate::bytecode::{Constant, ExcEntry, ExcKind, Param, Pattern, Program, Proto, MAGIC, VERSION};
const NONE: u32 = u32::MAX;
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
const EXC_CATCH: u8 = 0;
const EXC_FINALLY: u8 = 1;
const EXC_PROPAGATE: u8 = 2;
pub fn write(program: &Program) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());

    let flags: u16 = if program.source.is_some() { 1 } else { 0 };
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
        }
    }

    match proto.ext_type {
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
    if flags & !1 != 0 {
        return Err(format!("unsupported .unbc flags {flags:#06x}"));
    }
    let source_path = reader.string()?;
    let source = if flags & 1 != 0 {
        Some(reader.string()?)
    } else {
        None
    };
    let name_count = reader.u32()? as usize;
    let mut names = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        names.push(reader.string()?);
    }
    let const_count = reader.u32()? as usize;
    let mut consts = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        consts.push(read_constant(&mut reader)?);
    }
    let msg_count = reader.u32()? as usize;
    let mut msgs = Vec::with_capacity(msg_count);
    for _ in 0..msg_count {
        msgs.push(reader.string()?);
    }
    let proto_count = reader.u32()? as usize;
    let mut protos = Vec::with_capacity(proto_count);
    for _ in 0..proto_count {
        protos.push(read_proto(&mut reader)?);
    }
    let program = Program {
        source_path,
        source,
        names,
        consts,
        protos,
        msgs,
        root: 0,
    };
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
fn read_proto(reader: &mut Reader) -> Result<Proto, String> {
    let name = reader.u32()? as usize;
    let arity = reader.u32()? as usize;
    let flags = reader.u16()?;
    let local_count = reader.u32()? as usize;
    let mut locals = Vec::with_capacity(local_count);
    for _ in 0..local_count {
        locals.push(reader.u32()? as usize);
    }
    let param_count = reader.u32()? as usize;
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
    let code_count = reader.u32()? as usize;
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
    let call_count = reader.u32()? as usize;
    let mut call_names = Vec::with_capacity(call_count);
    for _ in 0..call_count {
        let count = reader.u32()? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(reader.u32()? as i32);
        }
        call_names.push(entries);
    }
    let map_count = reader.u32()? as usize;
    let mut source_map = Vec::with_capacity(map_count);
    for _ in 0..map_count {
        let start = reader.u32()? as usize;
        let length = reader.u32()? as usize;
        source_map.push((start, length));
    }
    let exc_count = reader.u32()? as usize;
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
    let pattern_count = reader.u32()? as usize;
    let mut patterns = Vec::with_capacity(pattern_count);
    for _ in 0..pattern_count {
        patterns.push(read_pattern(reader)?);
    }
    let ext_type = reader.optional_u32()?;
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
    })
}
fn read_pattern(reader: &mut Reader) -> Result<Pattern, String> {
    let tag = reader.u8()?;
    Ok(match tag {
        PATTERN_WILDCARD => Pattern::Wildcard,
        PATTERN_BIND => Pattern::Bind(reader.u32()? as usize),
        PATTERN_LITERAL => Pattern::Literal(reader.u32()? as usize),
        PATTERN_TUPLE => {
            let count = reader.u32()? as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(read_pattern(reader)?);
            }
            Pattern::Tuple(items)
        }
        other => return Err(format!("unknown pattern tag {other}")),
    })
}
fn read_table(reader: &mut Reader) -> Result<Vec<usize>, String> {
    let count = reader.u32()? as usize;
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
pub fn opcode_of(instr: &Instr) -> Option<Op> {
    instr.op_code()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Program {
        let mut program = Program {
            source_path: "test.un".to_string(),
            source: Some("x = 1\n".to_string()),
            names: vec!["<module>".to_string(), "io".to_string(), "x".to_string()],
            consts: vec![
                Constant::Int(1),
                Constant::Float(1.5),
                Constant::Str("text".to_string()),
                Constant::Bool(true),
                Constant::None,
                Constant::Null,
            ],
            protos: Vec::new(),
            msgs: vec!["unsupported node kind Match".to_string()],
            root: 0,
        };
        let mut proto = Proto::new(0, 0);
        proto.arity = 1;
        proto.locals = vec![2];
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
            var_slot: Some(2),
        }];
        proto.patterns = vec![
            Pattern::Wildcard,
            Pattern::Bind(2),
            Pattern::Literal(0),
            Pattern::Tuple(vec![Pattern::Wildcard, Pattern::Bind(1)]),
        ];
        proto.ext_type = Some(1);
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
