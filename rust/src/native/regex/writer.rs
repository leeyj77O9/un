//! The .NET `RegexWriter` equivalent — emits the opcode stream the VM executes.

use crate::native::regex::charclass::CharClass;
use crate::native::regex::node::Node;
use crate::native::regex::opts::*;

const T_CONCAT_BEFORE: i32 = K_CONCATENATE | K_BEFORE_CHILD;
const T_CONCAT_AFTER: i32 = K_CONCATENATE | K_AFTER_CHILD;
const T_ALT_BEFORE: i32 = K_ALTERNATE | K_BEFORE_CHILD;
const T_ALT_AFTER: i32 = K_ALTERNATE | K_AFTER_CHILD;
const T_BACKCOND_BEFORE: i32 = K_BACKCOND | K_BEFORE_CHILD;
const T_BACKCOND_AFTER: i32 = K_BACKCOND | K_AFTER_CHILD;
const T_EXPRCOND_BEFORE: i32 = K_EXPRCOND | K_BEFORE_CHILD;
const T_EXPRCOND_AFTER: i32 = K_EXPRCOND | K_AFTER_CHILD;
const T_LOOP_BEFORE: i32 = K_LOOP | K_BEFORE_CHILD;
const T_LOOP_AFTER: i32 = K_LOOP | K_AFTER_CHILD;
const T_LAZYLOOP_BEFORE: i32 = K_LAZYLOOP | K_BEFORE_CHILD;
const T_LAZYLOOP_AFTER: i32 = K_LAZYLOOP | K_AFTER_CHILD;
const T_CAPTURE_BEFORE: i32 = K_CAPTURE | K_BEFORE_CHILD;
const T_CAPTURE_AFTER: i32 = K_CAPTURE | K_AFTER_CHILD;
const T_POSLOOK_BEFORE: i32 = K_POSLOOK | K_BEFORE_CHILD;
const T_POSLOOK_AFTER: i32 = K_POSLOOK | K_AFTER_CHILD;
const T_NEGLOOK_BEFORE: i32 = K_NEGLOOK | K_BEFORE_CHILD;
const T_NEGLOOK_AFTER: i32 = K_NEGLOOK | K_AFTER_CHILD;
const T_ATOMIC_BEFORE: i32 = K_ATOMIC | K_BEFORE_CHILD;
const T_ATOMIC_AFTER: i32 = K_ATOMIC | K_AFTER_CHILD;

#[derive(Debug, Clone)]
pub struct Code {
    pub codes: Vec<i32>,
    pub strings: Vec<Vec<char>>,
    pub sets: Vec<CharClass>,
    pub trackcount: i32,
}

/// Map a raw capture slot to a dense index when the capture set is sparse.
pub struct SparseMap {
    pub map: Vec<i32>,
}

impl SparseMap {
    pub fn map_capture(&self, raw: i32) -> i32 {
        if self.map.is_empty() {
            raw
        } else {
            self.map[raw as usize]
        }
    }
}

struct StringTable {
    map: std::collections::HashMap<Vec<char>, i32>,
    entries: Vec<Vec<char>>,
}

impl StringTable {
    fn new() -> Self {
        StringTable {
            map: Default::default(),
            entries: Vec::new(),
        }
    }

    fn intern(&mut self, s: &[char]) -> i32 {
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        let i = self.entries.len() as i32;
        self.entries.push(s.to_vec());
        self.map.insert(s.to_vec(), i);
        i
    }
}

struct SetTable {
    entries: Vec<CharClass>,
}

impl SetTable {
    fn new() -> Self {
        SetTable { entries: Vec::new() }
    }

    fn intern(&mut self, set: &CharClass) -> i32 {
        for (i, s) in self.entries.iter().enumerate() {
            if sets_equal(s, set) {
                return i as i32;
            }
        }

        self.entries.push(set.clone());
        (self.entries.len() - 1) as i32
    }
}

fn sets_equal(a: &CharClass, b: &CharClass) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    a.ensure_canonical();
    b.ensure_canonical();
    a == b
}

/// Emit the opcode stream for a parsed tree. `sparse` maps raw slots to dense indices.
pub fn write(root: &Node, sparse: &Option<Vec<i32>>) -> Code {
    let sparse_map = SparseMap {
        map: sparse.clone().unwrap_or_default(),
    };

    let mut writer = Writer {
        emitted: Vec::new(),
        int_stack: Vec::new(),
        strings: StringTable::new(),
        sets: SetTable::new(),
        track_count: 0,
        sparse: sparse_map,
    };

    writer.emit_args(OP_LAZYBRANCH, 0);

    // depth-first walk
    let mut cur_node = root;
    let mut cur_child = 0;
    let mut stack: Vec<(usize, &Node)> = Vec::new();

    loop {
        let child_count = cur_node.children.len();
        if child_count == 0 {
            writer.emit_fragment(cur_node.kind, cur_node, 0, None);
        } else if cur_child < child_count {
            writer.emit_fragment(cur_node.kind | K_BEFORE_CHILD, cur_node, cur_child, None);
            stack.push((cur_child, cur_node));
            cur_node = &cur_node.children[cur_child];
            cur_child = 0;
            continue;
        }

        let Some((parent_child, parent)) = stack.pop() else {
            break;
        };

        writer.emit_fragment(parent.kind | K_AFTER_CHILD, parent, parent_child, None);
        cur_child = parent_child + 1;
        cur_node = parent;
    }

    let patch = writer.emitted.len();

    writer.patch_jump(0, patch as i32);
    writer.emit(OP_STOP);

    let trackcount = writer.track_count;

    Code {
        codes: writer.emitted,
        strings: writer.strings.entries,
        sets: writer.sets.entries,
        trackcount,
    }
}

struct Writer {
    emitted: Vec<i32>,
    int_stack: Vec<i32>,
    strings: StringTable,
    sets: SetTable,
    track_count: i32,
    sparse: SparseMap,
}

impl Writer {
    fn emit(&mut self, op: i32) {
        if opcode_backtracks(op) {
            self.track_count += 1;
        }

        self.emitted.push(op);
    }

    fn emit_args(&mut self, op: i32, opd1: i32) {
        if opcode_backtracks(op) {
            self.track_count += 1;
        }

        self.emitted.push(op);
        self.emitted.push(opd1);
    }

    fn emit_args2(&mut self, op: i32, opd1: i32, opd2: i32) {
        if opcode_backtracks(op) {
            self.track_count += 1;
        }

        self.emitted.push(op);
        self.emitted.push(opd1);
        self.emitted.push(opd2);
    }

    fn patch_jump(&mut self, offset: i32, dest: i32) {
        self.emitted[offset as usize + 1] = dest;
    }

    fn int_stack_push(&mut self, v: i32) {
        self.int_stack.push(v);
    }

    fn int_stack_pop(&mut self) -> i32 {
        self.int_stack.pop().expect("stack")
    }

    fn map_capture(&self, raw: i32) -> i32 {
        self.sparse.map_capture(raw)
    }

    fn emit_fragment(&mut self, node_type: i32, node: &Node, cur_index: usize, _prev: Option<()>) {
        let mut bits = 0;
        if (node.options & OPT_RTL) != 0 {
            bits |= OP_RTL;
        }
        if (node.options & OPT_ICASE) != 0 {
            bits |= OP_CASEINS;
        }

        match node_type {
            // Concatenate | BeforeChild / AfterChild, Empty — emit nothing
            T_CONCAT_BEFORE => {}
            T_CONCAT_AFTER => {}
            K_EMPTY => {}

            // Alternate | BeforeChild
            T_ALT_BEFORE => {
                if cur_index < node.children.len() - 1 {
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_LAZYBRANCH, 0);
                }
            }

            // Alternate | AfterChild
            T_ALT_AFTER => {
                if cur_index < node.children.len() - 1 {
                    if cur_index < node.children.len() - 1 {
                        let lazy_branch_pos = self.int_stack_pop();
                        self.int_stack_push(self.emitted.len() as i32);
                        self.emit_args(OP_GOTO, 0);
                        self.patch_jump(lazy_branch_pos, self.emitted.len() as i32);
                    }
                } else {
                    // last branch: patch all the Lazybranch positions to here
                    let mut n = 0;
                    while n < cur_index {
                        let pos = self.int_stack_pop();
                        self.patch_jump(pos, self.emitted.len() as i32);
                        n += 1;
                    }
                }
            }

            // BackreferenceConditional | BeforeChild
            T_BACKCOND_BEFORE => {
                if cur_index == 0 {
                    self.emit(OP_SETJUMP);
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_LAZYBRANCH, 0);
                    self.emit_args(OP_TESTREF, self.map_capture(node.m));
                    self.emit(OP_FOREJUMP);
                }
            }

            T_BACKCOND_AFTER => {
                if cur_index == 0 {
                    let branchpos = self.int_stack_pop();
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_GOTO, 0);
                    self.patch_jump(branchpos, self.emitted.len() as i32);
                    self.emit(OP_FOREJUMP);
                } else {
                    let pos = self.int_stack_pop();
                    self.patch_jump(pos, self.emitted.len() as i32);
                }
            }

            // ExpressionConditional | BeforeChild
            T_EXPRCOND_BEFORE => {
                if cur_index == 0 {
                    self.emit(OP_SETJUMP);
                    self.emit(OP_SETMARK);
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_LAZYBRANCH, 0);
                }
            }

            T_EXPRCOND_AFTER => {
                if cur_index == 0 {
                    self.emit(OP_GETMARK);
                    self.emit(OP_FOREJUMP);
                } else if cur_index == 1 {
                    let branchpos = self.int_stack_pop();
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_GOTO, 0);
                    self.patch_jump(branchpos, self.emitted.len() as i32);
                    self.emit(OP_GETMARK);
                    self.emit(OP_FOREJUMP);
                } else {
                    let pos = self.int_stack_pop();
                    self.patch_jump(pos, self.emitted.len() as i32);
                }
            }

            // Loop / Lazyloop | BeforeChild
            T_LOOP_BEFORE => {
                if node.n < i32::MAX || node.m > 1 {
                    if node.m == 0 {
                        self.emit_args(OP_NULLCOUNT, 0);
                    } else {
                        self.emit_args(OP_SETCOUNT, 1 - node.m);
                    }
                } else if node.m == 0 {
                    self.emit(OP_NULLMARK);
                } else {
                    self.emit(OP_SETMARK);
                }

                if node.m == 0 {
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_GOTO, 0);
                }
                self.int_stack_push(self.emitted.len() as i32);
            }

            T_LAZYLOOP_BEFORE => {
                if node.n < i32::MAX || node.m > 1 {
                    if node.m == 0 {
                        self.emit_args(OP_NULLCOUNT, 0);
                    } else {
                        self.emit_args(OP_SETCOUNT, 1 - node.m);
                    }
                } else if node.m == 0 {
                    self.emit(OP_NULLMARK);
                } else {
                    self.emit(OP_SETMARK);
                }

                if node.m == 0 {
                    self.int_stack_push(self.emitted.len() as i32);
                    self.emit_args(OP_GOTO, 0);
                }
                self.int_stack_push(self.emitted.len() as i32);
            }

            // Loop / Lazyloop | AfterChild
            T_LOOP_AFTER => {
                let start_jump_pos = self.emitted.len();
                if node.n < i32::MAX || node.m > 1 {
                    let target = self.int_stack_pop();
                    self.emit_args2(
                        OP_BRANCHCOUNT,
                        target,
                        if node.n == i32::MAX { i32::MAX } else { node.n - node.m },
                    );
                } else {
                    let target = self.int_stack_pop();
                    self.emit_args(OP_BRANCHMARK, target);
                }

                if node.m == 0 {
                    let target = self.int_stack_pop();
                    self.patch_jump(target, start_jump_pos as i32);
                }
            }

            T_LAZYLOOP_AFTER => {
                let start_jump_pos = self.emitted.len();
                if node.n < i32::MAX || node.m > 1 {
                    let target = self.int_stack_pop();
                    self.emit_args2(
                        OP_LAZYBRANCHCOUNT,
                        target,
                        if node.n == i32::MAX { i32::MAX } else { node.n - node.m },
                    );
                } else {
                    let target = self.int_stack_pop();
                    self.emit_args(OP_LAZYBRANCHMARK, target);
                }

                if node.m == 0 {
                    let target = self.int_stack_pop();
                    self.patch_jump(target, start_jump_pos as i32);
                }
            }

            // Capture | BeforeChild
            T_CAPTURE_BEFORE => {
                self.emit(OP_SETMARK);
            }

            // Capture | AfterChild
            T_CAPTURE_AFTER => {
                self.emit_args2(OP_CAPTUREMARK, self.map_capture(node.m), self.map_capture(node.n));
            }

            // PositiveLookaround | BeforeChild
            T_POSLOOK_BEFORE => {
                self.emit(OP_SETJUMP);
                self.emit(OP_SETMARK);
            }

            T_POSLOOK_AFTER => {
                self.emit(OP_GETMARK);
                self.emit(OP_FOREJUMP);
            }

            // NegativeLookaround | BeforeChild
            T_NEGLOOK_BEFORE => {
                self.emit(OP_SETJUMP);
                self.int_stack_push(self.emitted.len() as i32);
                self.emit_args(OP_LAZYBRANCH, 0);
            }

            T_NEGLOOK_AFTER => {
                self.emit(OP_BACKJUMP);
                let pos = self.int_stack_pop();
                self.patch_jump(pos, self.emitted.len() as i32);
                self.emit(OP_FOREJUMP);
            }

            // Atomic | BeforeChild / AfterChild
            T_ATOMIC_BEFORE => {
                self.emit(OP_SETJUMP);
            }

            T_ATOMIC_AFTER => {
                self.emit(OP_FOREJUMP);
            }

            // One / Notone
            K_ONE | K_NOTONE => {
                self.emit_args(node.kind | bits, node.ch as i32);
            }

            // Onerep / Notonerep / Setrep / Oneloop / Notoneloop / Setloop /
            // Onelazy / Notonelazy / Setlazy / the atomic variants
            K_ONEREP | K_NOTONEREP | K_SETREP | K_ONELLOOP | K_NOTONELLOOP | K_SETLOOP
            | K_ONELLOOP_ATOMIC | K_NOTONELLOOP_ATOMIC | K_SETLOOP_ATOMIC | K_ONELAZY
            | K_NOTONELAZY | K_SETLAZY => {
                let set_kind = matches!(node.kind, K_SETREP | K_SETLOOP | K_SETLOOP_ATOMIC | K_SETLAZY);
                let oneish_rep = matches!(node.kind,
                    K_ONELLOOP | K_ONELLOOP_ATOMIC | K_ONELAZY | K_ONEREP);

                if node.m > 0 {
                    if set_kind {
                        let code = self.set_code(node.set.as_ref().expect("set"));
                        self.emit_args2(OP_SETREP | bits, code, node.m);
                    } else if oneish_rep {
                        self.emit_args2(OP_ONEREP | bits, node.ch as i32, node.m);
                    } else {
                        self.emit_args2(OP_NOTONEREP | bits, node.ch as i32, node.m);
                    }
                }

                if node.n > node.m {
                    let tail = if node.n == i32::MAX { i32::MAX } else { node.n - node.m };
                    if set_kind {
                        let code = self.set_code(node.set.as_ref().expect("set"));
                        self.emit_args2(node.kind | bits, code, tail);
                    } else {
                        self.emit_args2(node.kind | bits, node.ch as i32, tail);
                    }
                }
            }

            // Multi — a string of chars
            K_MULTI => {
                let code = self.string_code(&node.str_val);
                self.emit_args(node.kind | bits, code);
            }

            // Set — a character class
            K_SET => {
                let code = self.set_code(node.set.as_ref().expect("set"));
                self.emit_args(OP_SET | bits, code);
            }

            // Backreference
            K_REF => {
                self.emit_args(OP_BACKREF | bits, self.map_capture(node.m));
            }

            // anchors and Nothing
            K_NOTHING | K_BOL | K_EOL | K_BOUNDARY | K_NONBOUNDARY | K_BEGINNING | K_START | K_ENDZ
            | K_END | K_ECMABOUNDARY | K_NONECMABOUNDARY | K_UPDATEBUMPALONG => {
                self.emit(node.kind | bits);
            }

            _ => {}
        }
    }

    fn string_code(&mut self, s: &str) -> i32 {
        self.strings.intern(&s.chars().collect::<Vec<_>>())
    }

    fn set_code(&mut self, set: &CharClass) -> i32 {
        self.sets.intern(set)
    }
}
