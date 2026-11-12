//! The .NET `RegexNode` equivalent — tree nodes plus the reductions the parser
//! relies on. Only the semantics-preserving parts of .NET's `Reduce` are ported;
//! the alternation-reordering and ending-backtracking passes are omitted as
//! they do not change observable behavior.

use crate::native::regex::charclass::CharClass;
use crate::native::regex::opts::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: i32,
    pub options: i32,
    pub m: i32,
    pub n: i32,
    pub ch: u32,
    pub str_val: String,
    pub set: Option<CharClass>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(kind: i32, options: i32) -> Self {
        Node {
            kind,
            options,
            m: 0,
            n: 0,
            ch: 0,
            str_val: String::new(),
            set: None,
            children: Vec::new(),
        }
    }

    pub fn one(ch: char, options: i32) -> Self {
        let mut n = Node::new(K_ONE, options);
        n.ch = ch as u32;
        n
    }

    pub fn notone(ch: char, options: i32) -> Self {
        let mut n = Node::new(K_NOTONE, options);
        n.ch = ch as u32;
        n
    }

    pub fn set(set: CharClass, options: i32) -> Self {
        let mut n = Node::new(K_SET, options);
        n.set = Some(set);
        n
    }

    pub fn multi(text: &str, options: i32) -> Self {
        let mut n = Node::new(K_MULTI, options);
        n.str_val = text.to_owned();
        n
    }

    pub fn reference(capnum: i32, options: i32) -> Self {
        let mut n = Node::new(K_REF, options);
        n.m = capnum;
        n
    }

    pub fn empty(options: i32) -> Self {
        Node::new(K_EMPTY, options)
    }

    pub fn nothing(options: i32) -> Self {
        Node::new(K_NOTHING, options)
    }

    pub fn add_child(&mut self, child: Node) {
        let reduced = child.reduce();
        self.children.push(reduced);
    }

    pub fn replace_child(&mut self, index: usize, child: Node) {
        let reduced = child.reduce();
        self.children[index] = reduced;
    }

    pub fn insert_child(&mut self, index: usize, child: Node) {
        let reduced = child.reduce();
        self.children.insert(index, reduced);
    }

    pub fn child(&self, i: usize) -> &Node {
        &self.children[i]
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    fn take_child(&mut self, i: usize) -> Node {
        self.children.remove(i)
    }

    /// `ReverseConcatenationIfRightToLeft` — lookbehinds reverse the order of the
    /// concatenation so the pattern reads from the end backwards.
    pub fn reverse_concatenation_if_rtl(mut self) -> Node {
        if (self.options & OPT_RTL) != 0 && self.kind == K_CONCATENATE && self.children.len() > 1 {
            self.children.reverse();
        }
        self
    }

    /// `MakeRep` — convert a One/Notone/Set to the loop/lazy kind.
    pub fn make_rep(&mut self, kind: i32, min: i32, max: i32) {
        self.kind = kind;
        self.m = min;
        self.n = max;
    }

    /// `MakeQuantifier` — the parser entry point for `x*`, `x+`, `x{n,m}`.
    pub fn make_quantifier(mut self, lazy: bool, min: i32, max: i32) -> Node {
        if min == max {
            match max {
                0 => return Node::empty(self.options),
                1 => return self,
                _ if max <= MULTI_VS_REPEATER_LIMIT && self.kind == K_ONE => {
                    // The same character is repeated a fixed number of times, so it's actually a multi.
                    self.kind = K_MULTI;
                    let c = char::from_u32(self.ch).unwrap_or('\u{FFFD}');
                    self.str_val = std::iter::repeat(c).take(max as usize).collect();
                    self.ch = 0;
                    return self;
                }
                _ => {}
            }
        }

        match self.kind {
            K_ONE | K_NOTONE | K_SET => {
                let loop_kind = if lazy {
                    match self.kind {
                        K_ONE => K_ONELAZY,
                        K_NOTONE => K_NOTONELAZY,
                        _ => K_SETLAZY,
                    }
                } else {
                    match self.kind {
                        K_ONE => K_ONELLOOP,
                        K_NOTONE => K_NOTONELLOOP,
                        _ => K_SETLOOP,
                    }
                };
                self.make_rep(loop_kind, min, max);
                self
            }
            _ => {
                let loop_kind = if lazy { K_LAZYLOOP } else { K_LOOP };
                let mut result = Node::new(loop_kind, self.options);
                result.m = min;
                result.n = max;
                result.add_child(self);
                result
            }
        }
    }

    /// `MakeLoopAtomic` — turn a loop/lazy kind into its atomic variant,
    /// mirroring .NET's rules.
    pub fn make_loop_atomic(&mut self, no_backtracking_by_position: bool, parent_kind: Option<i32>) {
        match self.kind {
            K_ONELLOOP | K_NOTONELLOOP | K_SETLOOP => {
                self.kind += K_ONELLOOP_ATOMIC - K_ONELLOOP;
            }
            K_ONELAZY | K_NOTONELAZY | K_SETLAZY => {
                self.kind += K_ONELLOOP_ATOMIC - K_ONELAZY;
                if no_backtracking_by_position {
                    self.n = self.m;
                }
                if self.n == 0 {
                    self.kind = K_EMPTY;
                    self.ch = 0;
                    self.str_val.clear();
                    self.set = None;
                } else if self.kind == K_ONELLOOP_ATOMIC && self.n >= 2 && self.n <= MULTI_VS_REPEATER_LIMIT {
                    self.kind = K_MULTI;
                    let c = char::from_u32(self.ch).unwrap_or('\u{FFFD}');
                    self.str_val = std::iter::repeat(c).take(self.n as usize).collect();
                    self.ch = 0;
                    self.m = 0;
                    self.n = 0;
                }
            }
            K_LOOP => {
                if parent_kind != Some(K_ATOMIC) {
                    self.wrap_in_atomic_loop();
                }
            }
            K_LAZYLOOP => {
                if no_backtracking_by_position {
                    self.n = self.m;
                }
                if self.n != 0 {
                    if parent_kind != Some(K_ATOMIC) {
                        self.wrap_in_atomic_loop();
                    }
                } else {
                    self.kind = K_EMPTY;
                    self.children.clear();
                }
            }
            _ => {}
        }
    }

    fn wrap_in_atomic_loop(&mut self) {
        let child = self.take_child(0);
        let mut loop_as_child = Node::new(K_LOOP, self.options);
        loop_as_child.m = self.m;
        loop_as_child.n = self.n;
        loop_as_child.add_child(child);
        self.kind = K_ATOMIC;
        self.m = 0;
        self.n = 0;
        self.children.clear();
        self.add_child(loop_as_child);
    }

    /// `Reduce` — the dispatcher. Strips IgnoreCase from everything except
    /// Backreference, then reduces per kind.
    pub fn reduce(mut self) -> Node {
        if self.kind != K_REF {
            self.options &= !OPT_ICASE;
        }

        match self.kind {
            K_ALTERNATE => Self::reduce_alternate(self),
            K_ATOMIC => Self::reduce_atomic(self),
            K_CONCATENATE => {
                match self.children.len() {
                    0 => Node::empty(self.options),
                    1 => self.children.into_iter().next().unwrap(),
                    _ => self,
                }
            }
            K_GROUP => Self::reduce_group(self),
            K_LOOP | K_LAZYLOOP => Self::reduce_loops(self),
            K_SET | K_SETLOOP | K_SETLOOP_ATOMIC | K_SETLAZY => Self::reduce_set(self),
            K_EXPRCOND => {
                let mut node = self;
                if node.children.len() == 2 {
                    let empty = Node::empty(node.options);
                    node.children.push(empty);
                }
                if node.child_count() >= 1
                    && node.child(0).kind == K_POSLOOK
                    && (node.child(0).options & OPT_RTL) == 0
                    && node.child(0).child_count() == 1
                {
                    let inner = node.take_child(0).take_child(0);
                    node.replace_child(0, inner);
                }
                node
            }
            K_BACKCOND => {
                if self.children.len() == 1 {
                    let mut node = self;
                    let empty = Node::empty(node.options);
                    node.children.push(empty);
                    node
                } else {
                    self
                }
            }
            _ => self,
        }
    }

    fn reduce_alternate(node: Node) -> Node {
        match node.children.len() {
            0 => Node::nothing(node.options),
            1 => node.children.into_iter().next().unwrap(),
            _ => node,
        }
    }

    fn reduce_atomic(node: Node) -> Node {
        // collapse nested atomics
        let mut atomic = node;
        let mut child = atomic.take_child(0);
        while child.kind == K_ATOMIC {
            atomic = child;
            child = atomic.take_child(0);
        }

        match child.kind {
            K_EMPTY | K_NOTHING => child,
            K_ONELLOOP_ATOMIC | K_NOTONELLOOP_ATOMIC | K_SETLOOP_ATOMIC => child,
            K_ONELLOOP | K_NOTONELLOOP | K_SETLOOP | K_ONELAZY | K_NOTONELAZY | K_SETLAZY => {
                child.make_loop_atomic(true, Some(K_ATOMIC));
                child
            }
            _ => {
                atomic.children.clear();
                atomic.add_child(child);
                atomic
            }
        }
    }

    fn reduce_group(node: Node) -> Node {
        let mut u = node;
        while u.kind == K_GROUP {
            u = u.take_child(0);
        }
        u
    }

    fn reduce_loops(node: Node) -> Node {
        let kind = node.kind;
        let mut min = node.m;
        let mut max = node.n;

        let mut u = node;

        loop {
            if u.children.is_empty() {
                break;
            }
            let child = u.take_child(0);

            // multiply reps of the same type only
            if child.kind != kind {
                let valid = match kind {
                    K_LOOP => matches!(
                        child.kind,
                        K_ONELLOOP
                            | K_ONELLOOP_ATOMIC
                            | K_NOTONELLOOP
                            | K_NOTONELLOOP_ATOMIC
                            | K_SETLOOP
                            | K_SETLOOP_ATOMIC
                    ),
                    _ => matches!(child.kind, K_ONELAZY | K_NOTONELAZY | K_SETLAZY),
                };

                if !valid {
                    u.children.insert(0, child);
                    break;
                }
            }

            // child can be too lumpy to blur, e.g., (a {100,105}) {3} or (a {2,})?
            if (u.m == 0 && child.m > 1) || (child.n as i64) < (child.m as i64) * 2 {
                u.children.insert(0, child);
                break;
            }

            u = child;

            if u.m > 0 {
                let mul = u.m as i64 * min as i64;
                u.m = if mul > i32::MAX as i64 { i32::MAX } else { mul as i32 };
                min = u.m;
            }

            if u.n > 0 {
                let mul = u.n as i64 * max as i64;
                u.n = if mul > i32::MAX as i64 { i32::MAX } else { mul as i32 };
                max = u.n;
            }
        }

        if min == i32::MAX {
            return Node::nothing(u.options);
        }

        if u.child_count() == 1 {
            let child = u.take_child(0);
            match child.kind {
                K_ONE | K_NOTONE | K_SET => {
                    let mut child = child;
                    let rep_kind = if u.kind == K_LAZYLOOP {
                        match child.kind {
                            K_ONE => K_ONELAZY,
                            K_NOTONE => K_NOTONELAZY,
                            _ => K_SETLAZY,
                        }
                    } else {
                        match child.kind {
                            K_ONE => K_ONELLOOP,
                            K_NOTONE => K_NOTONELLOOP,
                            _ => K_SETLOOP,
                        }
                    };
                    child.make_rep(rep_kind, u.m, u.n);
                    u = child;
                }
                K_EMPTY => {
                    u = child;
                }
                K_NEGLOOK | K_BEGINNING | K_START | K_BOL | K_EOL | K_END | K_ENDZ | K_BOUNDARY
                | K_NONBOUNDARY | K_ECMABOUNDARY | K_NONECMABOUNDARY => {
                    u = if u.m == 0 { Node::empty(u.options) } else { child };
                }
                K_POSLOOK => {
                    if !Self::contains_kind(&child, K_CAPTURE) {
                        u = if u.m == 0 { Node::empty(u.options) } else { child };
                    } else {
                        u.add_child(child);
                    }
                }
                _ => {
                    u.add_child(child);
                }
            }
        }

        u
    }

    fn contains_kind(node: &Node, kind: i32) -> bool {
        if node.kind == kind {
            return true;
        }
        node.children.iter().any(|c| Self::contains_kind(c, kind))
    }

    fn reduce_set(mut node: Node) -> Node {
        let Some(set) = node.set.take() else {
            return node;
        };

        if set.is_empty() {
            node.kind = K_NOTHING;
            return node;
        }

        if set.is_singleton() {
            node.ch = set.singleton_char() as u32;
            node.kind = match node.kind {
                K_SET => K_ONE,
                K_SETLOOP => K_ONELLOOP,
                K_SETLOOP_ATOMIC => K_ONELLOOP_ATOMIC,
                _ => K_ONELAZY,
            };
            return node;
        }

        if set.is_singleton_inverse() {
            node.ch = set.singleton_char() as u32;
            node.kind = match node.kind {
                K_SET => K_NOTONE,
                K_SETLOOP => K_NOTONELLOOP,
                K_SETLOOP_ATOMIC => K_NOTONELLOOP_ATOMIC,
                _ => K_NOTONELAZY,
            };
            return node;
        }

        node.set = Some(set);
        node
    }
}
