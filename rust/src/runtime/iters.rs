//! Phase 7 iterators: the lazy generator model behind `Un.Object.Iter`.
//!
//! the reference stores an `IEnumerable<Obj>` in `Iters.Value` and re-enumerates it on
//! every `Count()` / `ToList()` / `foreach`, so a sequence definition is reusable. the
//! Rust model keeps the definition as an immutable tree (`StageDef`) and puts the
//! enumeration position in mutable cells (`Stage`), so `Copy`/`Clone` build fresh
//! cursors over the same data and `Iter()` returns the same cell.

use std::cell::RefCell;
use std::rc::Rc;

use super::collections;
use super::obj::{self, BinOp, Obj, Payload, UnOp};
use super::world;

/// the data a sequence walks over. a `Live` source sees items appended during iteration,
/// like the reference's `foreach` over the same `List<Obj>` cell.
#[derive(Clone)]
pub enum Source {
    Live(Rc<RefCell<Vec<Obj>>>),
    Fixed(Rc<Vec<Obj>>),
}

impl Source {
    fn len(&self) -> i64 {
        match self {
            Source::Live(v) => v.borrow().len() as i64,
            Source::Fixed(v) => v.len() as i64,
        }
    }

    fn get(&self, i: usize) -> Option<Obj> {
        match self {
            Source::Live(v) => v.borrow().get(i).cloned(),
            Source::Fixed(v) => v.get(i).cloned(),
        }
    }
}

/// a reusable sequence definition. the wrapper stages keep their inner definition, so
/// a chain is a tree and `Take`/`Skip`/`Repeat` can be re-enumerated from the start.
#[derive(Clone)]
pub enum StageDef {
    Source(Source),
    Reverse(Source),
    Range { start: i64, stop: i64, step: i64 },
    Counter { start: i64 },
    Repeat { inner: Rc<StageDef>, count: i64 },
    Take { inner: Rc<StageDef>, n: i64 },
    Skip { inner: Rc<StageDef>, n: i64 },
    /// `Iters.Map` — the transform runs while the sequence is consumed
    Map {
        inner: Rc<StageDef>,
        func: Obj,
        wants_index: bool,
    },
    /// `Iters.Filter` — the predicate runs while the sequence is consumed
    Filter { inner: Rc<StageDef>, func: Obj },
}

/// the enumeration position of one stage.
#[derive(Clone, Copy)]
enum Cursor {
    Pos(i64),
    Val(i64),
    Rem(i64),
    Round(i64),
}

fn init_cursor(def: &StageDef) -> Cursor {
    match def {
        StageDef::Source(..) | StageDef::Reverse(..) => Cursor::Pos(0),
        StageDef::Range { start, .. } => Cursor::Val(*start),
        StageDef::Counter { start } => Cursor::Val(*start),
        StageDef::Repeat { .. } => Cursor::Round(0),
        StageDef::Take { n, .. } | StageDef::Skip { n, .. } => Cursor::Rem(*n),
        StageDef::Map { .. } => Cursor::Val(0),
        StageDef::Filter { .. } => Cursor::Pos(0),
    }
}

pub struct StageCell {
    def: Rc<StageDef>,
    cursor: Cursor,
    inner: Option<Stage>,
}

/// a stage with its mutable position.
pub type Stage = Rc<RefCell<StageCell>>;

fn build(def: &Rc<StageDef>) -> Stage {
    let inner = match def.as_ref() {
        StageDef::Repeat { inner, .. }
        | StageDef::Take { inner, .. }
        | StageDef::Skip { inner, .. }
        | StageDef::Map { inner, .. }
        | StageDef::Filter { inner, .. } => Some(build(inner)),
        _ => None,
    };
    Rc::new(RefCell::new(StageCell {
        def: def.clone(),
        cursor: init_cursor(def),
        inner,
    }))
}

/// `foreach` over the same enumerable starts a fresh enumerator, so a finished round
/// resets the inner chain's positions.
fn reset(stage: &Stage) {
    let (def, inner) = {
        let cell = stage.borrow();
        (cell.def.clone(), cell.inner.clone())
    };
    stage.borrow_mut().cursor = init_cursor(&def);
    if let Some(i) = inner {
        reset(&i);
    }
}

enum Step {
    Value(Obj),
    End,
    /// `Default(step == 0)` throws on the first `MoveNext`, so the panic is lazy and
    /// `ToStr()` / `Clone()` of a zero-step range still work.
    Panic(Obj),
}

fn next(stage: &Stage) -> Step {
    let (def, cursor, inner) = {
        let cell = stage.borrow();
        (cell.def.clone(), cell.cursor, cell.inner.clone())
    };

    match def.as_ref() {
        StageDef::Source(src) => {
            let Cursor::Pos(p) = cursor else { return Step::End };
            if p < 0 {
                return Step::End;
            }
            match src.get(p as usize) {
                Some(o) => {
                    stage.borrow_mut().cursor = Cursor::Pos(p + 1);
                    Step::Value(o)
                }
                None => Step::End,
            }
        }
        StageDef::Reverse(src) => {
            let len = src.len();
            let Cursor::Pos(p) = cursor else { return Step::End };
            if p >= len {
                return Step::End;
            }
            match src.get((len - 1 - p) as usize) {
                Some(o) => {
                    stage.borrow_mut().cursor = Cursor::Pos(p + 1);
                    Step::Value(o)
                }
                None => Step::End,
            }
        }
        StageDef::Range { start: _, stop, step } => {
            if *step == 0 {
                return Step::Panic(obj::panic("panic", "step cannot be zero"));
            }
            let v = cursor_val(cursor);
            let in_bound = if *step > 0 { v < *stop } else { v > *stop };
            if !in_bound {
                return Step::End;
            }
            // `i += step` is unchecked in the reference, so it wraps
            stage.borrow_mut().cursor = Cursor::Val(v.wrapping_add(*step));
            Step::Value(obj::int(v))
        }
        StageDef::Counter { start: _ } => {
            let v = cursor_val(cursor);
            stage.borrow_mut().cursor = Cursor::Val(v.wrapping_add(1));
            Step::Value(obj::int(v))
        }
        StageDef::Repeat { count, .. } => {
            let round = cursor_val(cursor);
            if *count >= 0 && round >= *count {
                return Step::End;
            }
            let inner = inner.expect("repeat has an inner stage");
            loop {
                match next(&inner) {
                    Step::Value(o) => return Step::Value(o),
                    Step::Panic(p) => return Step::Panic(p),
                    Step::End => {
                        let r = round + 1;
                        stage.borrow_mut().cursor = Cursor::Round(r);
                        reset(&inner);
                        if *count >= 0 && r >= *count {
                            return Step::End;
                        }
                    }
                }
            }
        }
        StageDef::Take { .. } => {
            let rem = cursor_val(cursor);
            if rem <= 0 {
                return Step::End;
            }
            let inner = inner.expect("take has an inner stage");
            match next(&inner) {
                Step::Value(o) => {
                    stage.borrow_mut().cursor = Cursor::Rem(rem - 1);
                    Step::Value(o)
                }
                other => other,
            }
        }
        StageDef::Skip { .. } => {
            let mut rem = cursor_val(cursor);
            let inner = inner.expect("skip has an inner stage");
            while rem > 0 {
                match next(&inner) {
                    Step::Value(_) => {
                        rem -= 1;
                        stage.borrow_mut().cursor = Cursor::Rem(rem);
                    }
                    other => return other,
                }
            }
            next(&inner)
        }
        StageDef::Map { func, wants_index, .. } => {
            let inner = inner.expect("map has an inner stage");
            match next(&inner) {
                Step::Value(o) => {
                    // the index of the mapped value, the reference counts the values
                    // of the inner sequence
                    let index = cursor_val(cursor);
                    stage.borrow_mut().cursor = Cursor::Val(index + 1);

                    let args = if *wants_index {
                        vec![o, obj::int(index)]
                    } else {
                        vec![o]
                    };

                    let value = crate::runtime::function::fn_call(
                        func,
                        &crate::runtime::function::Args::positional(args),
                    );
                    Step::Value(value)
                }
                other => other,
            }
        }
        StageDef::Filter { func, .. } => {
            let inner = inner.expect("filter has an inner stage");
            loop {
                match next(&inner) {
                    Step::Value(o) => {
                        let keep = crate::runtime::function::fn_call(
                            func,
                            &crate::runtime::function::Args::positional(vec![o.clone()]),
                        );
                        if matches!(obj::payload(&keep), Payload::Bool(true)) {
                            return Step::Value(o);
                        }
                    }
                    other => return other,
                }
            }
        }
    }
}

fn cursor_val(cursor: Cursor) -> i64 {
    match cursor {
        Cursor::Pos(v) | Cursor::Val(v) | Cursor::Rem(v) | Cursor::Round(v) => v,
    }
}

/// `Value.Count()`: a formula, not an enumeration, so counting a sequence does not
/// consume it. the reference's LINQ counts `Take`/`Skip`/`Repeat` arithmetically too.
fn count(stage: &Stage) -> Obj {
    let (def, inner) = {
        let cell = stage.borrow();
        (cell.def.clone(), cell.inner.clone())
    };

    match def.as_ref() {
        StageDef::Source(src) | StageDef::Reverse(src) => obj::int(src.len()),
        StageDef::Range { start, stop, step } => {
            if *step == 0 {
                // `Len()` divides by zero inside the host
                return obj::divide_by_zero();
            }
            if (*step > 0 && start >= stop) || (*step < 0 && start <= stop) {
                return obj::int(0);
            }
            let n = stop
                .wrapping_sub(*start)
                .wrapping_add(*step)
                .wrapping_sub(if *step > 0 { 1 } else { -1 });
            obj::int(n / *step)
        }
        StageDef::Counter { .. } => obj::int(i64::MAX),
        StageDef::Repeat { count: n, .. } => {
            let inner_count = count(&inner.expect("repeat has an inner stage"));
            combine(inner_count, |c| c.wrapping_mul(*n))
        }
        StageDef::Take { n, .. } => {
            let inner_count = count(&inner.expect("take has an inner stage"));
            combine(inner_count, |c| c.min(*n))
        }
        StageDef::Skip { n, .. } => {
            let inner_count = count(&inner.expect("skip has an inner stage"));
            combine(inner_count, |c| c.saturating_sub(*n))
        }
        StageDef::Map { inner: def, .. } => count(&build(def)),
        StageDef::Filter { .. } => {
            // the predicate decides how many values survive, so the sequence is walked
            let values = match collect(stage) {
                Ok(values) => values,
                Err(e) => return e,
            };
            obj::int(values.len() as i64)
        }
    }
}

fn combine(counted: Obj, f: impl Fn(i64) -> i64) -> Obj {
    match obj::payload(&counted) {
        Payload::Int(v) => obj::int(f(v)),
        _ => counted,
    }
}

/// a chain can be enumerated to the end only when every stage is bounded.
fn bounded(def: &Rc<StageDef>) -> bool {
    match def.as_ref() {
        StageDef::Counter { .. } => false,
        StageDef::Source(..) | StageDef::Reverse(..) => true,
        StageDef::Range { step, .. } => *step != 0,
        StageDef::Repeat { inner, count } => *count >= 0 && bounded(inner),
        StageDef::Take { .. } => true,
        StageDef::Skip { inner, .. } => bounded(inner),
        StageDef::Map { inner, .. } => bounded(inner),
        StageDef::Filter { inner, .. } => bounded(inner),
    }
}

/// `new List([.. Value])`: a fresh enumerator over the definition, so the shared
/// enumeration position of `Next()` is untouched.
fn collect(stage: &Stage) -> Result<Vec<Obj>, Obj> {
    let def = stage.borrow().def.clone();
    if !bounded(&def) {
        // documented divergence: the reference hangs here
        return Err(obj::panic("panic", "iteration is infinite"));
    }
    let fresh = build(&def);
    let mut out = Vec::new();
    loop {
        match next(&fresh) {
            Step::Value(o) => out.push(o),
            Step::End => return Ok(out),
            Step::Panic(p) => return Err(p),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Iters,
    Range,
    Counter,
    Repeat,
    Reverse,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Iters => "iter",
            Kind::Range => "range",
            Kind::Counter => "counter",
            Kind::Repeat => "repeat",
            Kind::Reverse => "reverse",
        }
    }
}

pub struct IterState {
    pub kind: Kind,
    pub head: Stage,
}

/// `Iters` and its subclasses share one payload; the `Kind` carries the subclass rules
/// (`Type`, `ToStr`, `Len`, `Init`) that the reference splits across classes.
pub type IterCell = Rc<RefCell<IterState>>;

pub fn head(st: &IterCell) -> (Kind, Stage) {
    let s = st.borrow();
    (s.kind, s.head.clone())
}

pub fn iter_new(kind: Kind, def: Rc<StageDef>) -> Obj {
    let name = world::intern_type(kind.name());
    obj::obj_new(
        name.as_ref(),
        Payload::Iter(Rc::new(RefCell::new(IterState {
            kind,
            head: build(&def),
        }))),
    )
}

pub fn seq(values: Vec<Obj>) -> Obj {
    iter_new(Kind::Iters, Rc::new(StageDef::Source(Source::Fixed(Rc::new(values)))))
}

pub fn seq_live(values: &Rc<RefCell<Vec<Obj>>>) -> Obj {
    iter_new(
        Kind::Iters,
        Rc::new(StageDef::Source(Source::Live(values.clone()))),
    )
}

pub fn range_new(start: i64, stop: i64, step: i64) -> Obj {
    iter_new(Kind::Range, Rc::new(StageDef::Range { start, stop, step }))
}

pub fn counter_new(start: i64) -> Obj {
    iter_new(Kind::Counter, Rc::new(StageDef::Counter { start }))
}

pub fn repeat_new(inner: Rc<StageDef>, count: i64) -> Obj {
    iter_new(Kind::Repeat, Rc::new(StageDef::Repeat { inner, count }))
}

pub fn repeat_source(values: Vec<Obj>, count: i64) -> Obj {
    repeat_new(
        Rc::new(StageDef::Source(Source::Fixed(Rc::new(values)))),
        count,
    )
}

pub fn reverse_new(values: Vec<Obj>) -> Obj {
    iter_new(
        Kind::Reverse,
        Rc::new(StageDef::Reverse(Source::Fixed(Rc::new(values)))),
    )
}

pub fn take_new(inner: Rc<StageDef>, n: i64) -> Obj {
    iter_new(Kind::Iters, Rc::new(StageDef::Take { inner, n }))
}

/// `Iters.Map` — a lazy transform over a sequence definition.
pub fn map_new(inner: Rc<StageDef>, func: Obj, wants_index: bool) -> Rc<StageDef> {
    Rc::new(StageDef::Map {
        inner,
        func,
        wants_index,
    })
}

/// `Iters.Filter` — a lazy predicate over a sequence definition.
pub fn filter_new(inner: Rc<StageDef>, func: Obj) -> Rc<StageDef> {
    Rc::new(StageDef::Filter { inner, func })
}

/// an iterator object over a sequence definition.
pub fn seq_obj(def: Rc<StageDef>) -> Obj {
    iter_new(Kind::Iters, def)
}

/// a fresh enumerator over a sequence definition. the reference's `Zip` walks several
/// enumerators in parallel, so the enumerators are independent of the shared position.
pub struct Enumerator {
    stage: Stage,
}

impl Enumerator {
    pub fn new(def: &Rc<StageDef>) -> Self {
        Self { stage: build(def) }
    }

    /// the next value, `None` when the sequence ended
    pub fn next(&mut self) -> Option<Obj> {
        match next(&self.stage) {
            Step::Value(value) => Some(value),
            Step::End => None,
            Step::Panic(p) => Some(p),
        }
    }
}

pub fn skip_new(inner: Rc<StageDef>, n: i64) -> Obj {
    iter_new(Kind::Iters, Rc::new(StageDef::Skip { inner, n }))
}

/// `iterator.take(n)` / `iterator.skip(n)` build a wrapper stage over the receiver's
/// definition, so the receiver keeps its own position.
pub fn take_of(st: &IterCell, n: i64) -> Obj {
    let (_kind, stage) = head(st);
    let def = stage.borrow().def.clone();
    take_new(def, n)
}

pub fn skip_of(st: &IterCell, n: i64) -> Obj {
    let (_kind, stage) = head(st);
    let def = stage.borrow().def.clone();
    skip_new(def, n)
}

pub fn def_of(st: &IterCell) -> Rc<StageDef> {
    st.borrow().head.borrow().def.clone()
}

fn contains(start: i64, stop: i64, step: i64, value: i64) -> bool {
    if step > 0 {
        return value >= start && value < stop && (value - start) % step == 0;
    }
    value <= start && value > stop && (start - value) % -step == 0
}

/// `Str.Iter()`: one `Str` per UTF-16 code unit. a lone surrogate cannot exist in a Rust
/// `String`, so it becomes U+FFFD (D-020, divergence list).
pub fn str_units(s: &str) -> Vec<String> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..=0xDFFF).contains(&u) {
            // .NET yields a lone surrogate char as '?'; a Rust String cannot hold one,
            // so U+FFFD is used (D-020 divergence).
            if u <= 0xDBFF && i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                let c = char::decode_utf16([u, units[i + 1]])
                    .next()
                    .unwrap_or(Ok('\u{FFFD}'))
                    .unwrap_or('\u{FFFD}');
                out.push(c.to_string());
                i += 2;
                continue;
            }
            out.push('\u{FFFD}'.to_string());
            i += 1;
            continue;
        }
        out.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}').to_string());
        i += 1;
    }
    out
}

pub fn str_iter(s: &str) -> Obj {
    seq(str_units(s).into_iter().map(|c| obj::str(&c)).collect())
}

pub fn str_list(s: &str) -> Obj {
    collections::list(str_units(s).into_iter().map(|c| obj::str(&c)).collect())
}

pub fn str_tuple(s: &str) -> Obj {
    collections::tup(str_units(s).into_iter().map(|c| obj::str(&c)).collect(), Vec::new())
}

// ---------------------------------------------------------------------------
// dispatch arms
// ---------------------------------------------------------------------------

/// `Iters.Next()`, `Len()`, `Iter()`, `ToList()`, `ToTuple()`, `ToStr()`, `Spread()`.
pub fn iter_unop(o: &Obj, st: &IterCell, op: UnOp, t: &Rc<str>) -> Obj {
    let (kind, stage) = head(st);

    match op {
        UnOp::Iter => o.clone(),
        UnOp::Len => count(&stage),
        UnOp::Next => match next(&stage) {
            Step::Value(v) => v,
            Step::End => obj::err("iteration stopped"),
            Step::Panic(p) => p,
        },
        UnOp::ToList => match materialize(kind, &stage) {
            Ok(items) => collections::list(items),
            Err(e) => e,
        },
        UnOp::ToTuple => match materialize(kind, &stage) {
            Ok(items) => collections::tup(items, Vec::new()),
            Err(e) => e,
        },
        UnOp::Spread => {
            if kind == Kind::Counter {
                return obj::err("counter is infinite");
            }
            if kind == Kind::Repeat && !bounded(&stage.borrow().def) {
                return obj::err("repeat is infinite");
            }
            match collect(&stage) {
                Ok(items) => collections::spread(items),
                Err(p) => p,
            }
        }
        // `ToStr` is not a `UnOp`; `obj::to_str` calls `iter_to_str` from its payload arm
        _ => obj::err(UnOp::message(&op, t)),
    }
}

fn materialize(kind: Kind, stage: &Stage) -> Result<Vec<Obj>, Obj> {
    if kind == Kind::Counter {
        return Err(obj::err("counter is infinite"));
    }
    if kind == Kind::Repeat && !bounded(&stage.borrow().def) {
        return Err(obj::err("repeat is infinite"));
    }
    collect(stage)
}

/// `Iters.ToStr()` joins the elements, `Range.ToStr()` prints the bounds,
/// `Counter.ToStr()` is the infinite error, and `Repeat.ToStr()` is `Repr()`.
pub fn iter_to_str(o: &Obj, st: &IterCell) -> Obj {
    let (kind, stage) = head(st);
    let def = stage.borrow().def.clone();

    match kind {
        Kind::Counter => obj::err("counter is infinite"),
        Kind::Repeat => obj::repr(o),
        Kind::Range => {
            if let StageDef::Range { start, stop, step } = def.as_ref() {
                if *step == 1 {
                    obj::str(&format!("{}..{}", start, stop))
                } else {
                    obj::str(&format!("{}..{}..{}", start, stop, step))
                }
            } else {
                obj::str("")
            }
        }
        Kind::Iters | Kind::Reverse => match collect(&stage) {
            Ok(items) => obj::str(
                &items
                    .iter()
                    .map(|e| obj::display(e))
                    .collect::<Vec<String>>()
                    .join(", "),
            ),
            Err(p) => p,
        },
    }
}

pub fn iter_binop(o: &Obj, st: &IterCell, op: BinOp, other: &Obj, t: &Rc<str>, ot: &Rc<str>) -> Obj {
    let (kind, stage) = head(st);

    if kind == Kind::Range && op == BinOp::Eq {
        if let Payload::Iter(os) = obj::payload(other) {
            let (okind, odef) = head(&os);
            if okind == Kind::Range {
                let mine = stage.borrow().def.clone();
                let theirs = odef.borrow().def.clone();
                if let (
                    StageDef::Range {
                        start: a,
                        stop: b,
                        step: c,
                    },
                    StageDef::Range {
                        start: x,
                        stop: y,
                        step: z,
                    },
                ) = (mine.as_ref(), theirs.as_ref())
                {
                    return obj::boolean(a == x && b == y && c == z);
                }
            }
        }
    }

    obj::base_binop(o, op, other, t, ot)
}

pub fn iter_in(o: &Obj, st: &IterCell, other: &Obj) -> Obj {
    let (kind, stage) = head(st);

    if kind == Kind::Range {
        if let Payload::Int(v) = obj::payload(other) {
            if let StageDef::Range { start, stop, step } = stage.borrow().def.as_ref() {
                return obj::boolean(contains(*start, *stop, *step, v));
            }
        }
    }

    obj::err(format!(
        "unsupported operand type(s) for in: '{}'",
        obj::type_name(o)
    ))
}

pub fn iter_clone(o: &Obj, st: &IterCell) -> Obj {
    let (kind, stage) = head(st);
    let def = stage.borrow().def.clone();

    match kind {
        // `Counter.Clone() => new(current)`. `current` is the constructor field and the
        // enumerator uses a separate local, so it never advances: a clone restarts at
        // the original start, not at the cursor.
        Kind::Counter => {
            let start = match def.as_ref() {
                StageDef::Counter { start } => *start,
                _ => 0,
            };
            counter_new(start)
        }
        Kind::Range => {
            if let StageDef::Range { start, stop, step } = def.as_ref() {
                range_new(*start, *stop, *step)
            } else {
                o.clone()
            }
        }
        Kind::Iters | Kind::Repeat | Kind::Reverse => iter_new(kind, def),
    }
}

/// `Iters.Copy()` is `this` and no subclass overrides it.
pub fn iter_copy(o: &Obj) -> Obj {
    o.clone()
}

pub fn iter_init(o: &Obj, st: &IterCell, args: &[Obj]) -> Obj {
    let (kind, _) = head(st);

    match kind {
        // `invaild` is the reference's spelling
        Kind::Iters => {
            if args.len() == 1 {
                obj::iter(&args[0])
            } else {
                obj::err("invaild 'iter' initialize")
            }
        }
        Kind::Counter => match args.len() {
            0 => counter_new(0),
            1 => match obj::payload(&args[0]) {
                Payload::Int(v) => counter_new(v),
                _ => obj::err("invalid 'counter' initialize"),
            },
            _ => obj::err("invalid 'counter' initialize"),
        },
        Kind::Repeat => {
            let source = |arg: &Obj| match obj::payload(arg) {
                Payload::List(v) => Some(Source::Live(v.clone())),
                Payload::Tup(vals, _) => Some(Source::Fixed(vals.clone())),
                _ => None,
            };
            match args.len() {
                1 => match source(&args[0]) {
                    Some(src) => iter_new(Kind::Repeat, Rc::new(StageDef::Repeat { inner: Rc::new(StageDef::Source(src)), count: -1 })),
                    None => obj::err("invalid 'repeat' initialize"),
                },
                2 => {
                    let src = source(&args[0]);
                    let count = match obj::payload(&args[1]) {
                        Payload::Int(v) => Some(v),
                        _ => None,
                    };
                    match (src, count) {
                        (Some(src), Some(c)) => iter_new(
                            Kind::Repeat,
                            Rc::new(StageDef::Repeat {
                                inner: Rc::new(StageDef::Source(src)),
                                count: c,
                            }),
                        ),
                        _ => obj::err("invalid 'repeat' initialize"),
                    }
                }
                _ => obj::err("invalid 'repeat' initialize"),
            }
        }
        Kind::Reverse => {
            if args.len() == 1 {
                // `Reverse.Init` materializes the reversed sequence, so the clone is a snapshot
                match obj::payload(&obj::iter(&args[0])) {
                    Payload::Iter(is) => {
                        let items = is.borrow().head.borrow().def.clone();
                        match collect_items(&items) {
                            Ok(v) => reverse_new(v),
                            Err(p) => p,
                        }
                    }
                    _ => obj::err("invalid 'reverse' initialization"),
                }
            } else {
                obj::err("invalid 'reverse' initialization")
            }
        }
        // `Range` has no `Init` override, so the base chain returns `this`
        Kind::Range => o.clone(),
    }
}

/// the forward items of a definition, taken with fresh cursors
fn collect_items(def: &Rc<StageDef>) -> Result<Vec<Obj>, Obj> {
    collect(&build(def))
}

pub fn next_of(st: &IterCell) -> Obj {
    let stage = st.borrow().head.clone();
    match next(&stage) {
        Step::Value(v) => v,
        Step::End => obj::err("iteration stopped"),
        Step::Panic(p) => p,
    }
}

pub fn count_of(st: &IterCell) -> Obj {
    count(&st.borrow().head.clone())
}

#[cfg(test)]
mod tests;
