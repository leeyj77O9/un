//! `Un.Native.Random` — the native `random` module.
//!
//! the reference shares one `System.Random` between the functions, and `seed` replaces
//! it, so the same seed produces the same values. the .NET generator itself is not
//! reproduced (the values are not a parity target), but the reproducibility is.

use std::cell::RefCell;

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

// the shared generator, the reference's `shared` field.
thread_local! {
    static RANDOM: RefCell<Rng> = RefCell::new(Rng::new(0));
}

/// a small xorshift generator, seeded the same way for every seed.
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        // a zero state would stay zero, so the seed is mixed
        let state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x2545_F491_4F6C_DD1D;
        Self {
            state: if state == 0 { 0x9E37_79B9_7F4A_7C15 } else { state },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// a value in `0..bound`, the reference's `Next(bound)`.
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        self.next_u64() % bound
    }

    /// a value in `min..=max`, the reference's `Next(min, max + 1)`.
    fn between(&mut self, min: i64, max: i64) -> i64 {
        if max <= min {
            return min;
        }
        min + self.below((max - min + 1) as u64) as i64
    }

    /// a value in `0.0..1.0`, the reference's `NextDouble()`.
    fn next_double(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn with_random<T>(f: impl FnOnce(&mut Rng) -> T) -> T {
    RANDOM.with(|r| f(&mut r.borrow_mut()))
}

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "random".to_owned(),
            native(closure, "random", Vec::new(), random_impl),
        ),
        ("next".to_owned(), native(closure, "next", Vec::new(), next_impl)),
        (
            "int".to_owned(),
            native(
                closure,
                "int",
                vec![arg("min", true, false, false), arg("max", true, false, false)],
                int_impl,
            ),
        ),
        (
            "float".to_owned(),
            native(
                closure,
                "float",
                vec![arg("min", false, true, false), arg("max", false, true, false)],
                float_impl,
            ),
        ),
        (
            "bool".to_owned(),
            native(closure, "bool", Vec::new(), bool_impl),
        ),
        (
            "bytes".to_owned(),
            native(
                closure,
                "bytes",
                vec![arg("length", true, false, false)],
                bytes_impl,
            ),
        ),
        (
            "shuffle".to_owned(),
            native(
                closure,
                "shuffle",
                vec![arg("obj", true, false, false)],
                shuffle_impl,
            ),
        ),
        (
            "choice".to_owned(),
            native(
                closure,
                "choice",
                vec![arg("obj", true, false, false)],
                choice_impl,
            ),
        ),
        (
            "choices".to_owned(),
            native(
                closure,
                "choices",
                vec![
                    arg("obj", true, false, false),
                    arg("count", true, false, false),
                ],
                choices_impl,
            ),
        ),
        (
            "sample".to_owned(),
            native(
                closure,
                "sample",
                vec![
                    arg("obj", true, false, false),
                    arg("count", true, false, false),
                ],
                sample_impl,
            ),
        ),
        (
            "chance".to_owned(),
            native(
                closure,
                "chance",
                vec![arg("probability", true, false, false)],
                chance_impl,
            ),
        ),
        (
            "seed".to_owned(),
            native(
                closure,
                "seed",
                vec![arg("seed", true, false, false)],
                seed_impl,
            ),
        ),
        ("uuid".to_owned(), native(closure, "uuid", Vec::new(), uuid_impl)),
        (
            "str".to_owned(),
            native(
                closure,
                "str",
                vec![arg("length", true, false, false)],
                str_impl,
            ),
        ),
        (
            "weighted".to_owned(),
            native(
                closure,
                "weighted",
                vec![
                    arg("values", true, false, false),
                    arg("weights", true, false, false),
                ],
                weighted_impl,
            ),
        ),
        (
            "range".to_owned(),
            native(
                closure,
                "range",
                vec![arg("start", true, false, false), arg("end", false, true, false)],
                range_impl,
            ),
        ),
        (
            "value".to_owned(),
            native(
                closure,
                "value",
                vec![arg("obj", true, false, false)],
                value_impl,
            ),
        ),
    ]
}

fn random_impl(_scope: &Scope) -> Obj {
    obj::plain(
        crate::runtime::world::intern_type("random"),
        crate::runtime::world::obj_type(),
    )
}

fn next_impl(_scope: &Scope) -> Obj {
    let value = with_random(|rng| rng.next_u64() >> 11);
    obj::int(value as i64)
}

fn int_impl(scope: &Scope) -> Obj {
    let min = scope::get(scope, "min");
    let max = scope::get(scope, "max");

    let min = match obj::payload(&min) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.int' argument 'min' is int"),
    };
    let max = match obj::payload(&max) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.int' argument 'max' is int"),
    };

    if min > max {
        return obj::err("'min' cannot be greater than 'max'");
    }

    let value = with_random(|rng| rng.between(min, max));
    obj::int(value)
}

fn float_impl(scope: &Scope) -> Obj {
    let min = scope::get(scope, "min");
    let max = scope::get(scope, "max");

    if obj::is_none(&min) && obj::is_none(&max) {
        let value = with_random(|rng| rng.next_double());
        return obj::float(value);
    }

    if obj::is_none(&min) || obj::is_none(&max) {
        return obj::err("'min' and 'max' must be specified together");
    }

    let min = match obj::payload(&min) {
        Payload::Float(v) => v,
        _ => return obj::err("expected 'random.float' argument 'min' is float"),
    };
    let max = match obj::payload(&max) {
        Payload::Float(v) => v,
        _ => return obj::err("expected 'random.float' argument 'max' is float"),
    };

    if min > max {
        return obj::err("'min' cannot be greater than 'max'");
    }

    let value = with_random(|rng| rng.next_double() * (max - min) + min);
    obj::float(value)
}

fn bool_impl(_scope: &Scope) -> Obj {
    let value = with_random(|rng| rng.below(2) == 0);
    obj::boolean(value)
}

fn bytes_impl(scope: &Scope) -> Obj {
    let length = scope::get(scope, "length");

    let length = match obj::payload(&length) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.bytes' argument 'length' is int"),
    };

    if length < 0 {
        return obj::err("'length' cannot be negative");
    }

    let values: Vec<Obj> = with_random(|rng| {
        (0..length).map(|_| obj::int((rng.next_u64() & 0xFF) as i64)).collect()
    });

    collections::list(values)
}

/// the reference's `Random.Shuffle` — the Fisher-Yates shuffle.
fn shuffle_values(values: &mut Vec<Obj>) {
    let len = values.len();
    if len < 2 {
        return;
    }

    for i in (1..len).rev() {
        let j = with_random(|rng| rng.below((i + 1) as u64) as usize);
        values.swap(i, j);
    }
}

fn shuffle_impl(scope: &Scope) -> Obj {
    let target = scope::get(scope, "obj");

    let list = match obj::payload(&target) {
        Payload::List(list) => list.clone(),
        _ => return obj::err("expected 'random.shuffle' argument to be a list"),
    };

    let mut values = list.borrow().clone();
    shuffle_values(&mut values);
    *list.borrow_mut() = values;

    obj::none()
}

fn choice_impl(scope: &Scope) -> Obj {
    let target = scope::get(scope, "obj");

    let list = match obj::payload(&target) {
        Payload::List(list) => list.clone(),
        _ => return obj::err("expected 'random.choice' argument to be a list"),
    };

    let count = list.borrow().len();
    if count == 0 {
        return obj::err("expected 'random.choice' argument to be a list");
    }

    let index = with_random(|rng| rng.below(count as u64) as usize);
    let items = list.borrow().clone();
    items[index].clone()
}

fn choices_impl(scope: &Scope) -> Obj {
    let target = scope::get(scope, "obj");
    let count = scope::get(scope, "count");

    let list = match obj::payload(&target) {
        Payload::List(list) => list.clone(),
        _ => return obj::err("expected 'random.choices' argument 'obj' is list"),
    };
    let count = match obj::payload(&count) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.choices' argument 'count' is int"),
    };

    let items = list.borrow().clone();
    if items.is_empty() {
        return obj::err("expected 'random.choices' argument 'obj' is list");
    }

    let mut result: Vec<Obj> = Vec::new();
    for _ in 0..count {
        let index = with_random(|rng| rng.below(items.len() as u64) as usize);
        result.push(items[index].clone());
    }

    collections::list(result)
}

fn sample_impl(scope: &Scope) -> Obj {
    let target = scope::get(scope, "obj");
    let count = scope::get(scope, "count");

    let list = match obj::payload(&target) {
        Payload::List(list) => list.clone(),
        _ => return obj::err("expected 'random.sample' argument 'obj' is list"),
    };
    let count = match obj::payload(&count) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.sample' argument 'count' is int"),
    };

    let items = list.borrow().clone();
    if count > items.len() as i64 {
        return obj::err(
            "'random.sample' argument 'count' cannot be greater than the length of 'obj'",
        );
    }

    let mut clone = items;
    shuffle_values(&mut clone);
    clone.truncate(count.max(0) as usize);

    collections::list(clone)
}
fn chance_impl(scope: &Scope) -> Obj {
    let probability = scope::get(scope, "probability");

    let value = match obj::payload(&probability) {
        Payload::Float(v) => v,
        _ => return obj::err("expected 'random.chance' argument 'probability' is float"),
    };

    if !(0.0..=1.0).contains(&value) {
        return obj::err("'probability' must be between 0 and 1");
    }

    let hit = with_random(|rng| rng.next_double() < value);
    obj::boolean(hit)
}

fn seed_impl(scope: &Scope) -> Obj {
    let seed = scope::get(scope, "seed");

    let value = match obj::payload(&seed) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.seed' argument 'seed' is int"),
    };

    RANDOM.with(|r| *r.borrow_mut() = Rng::new(value as u64));

    obj::none()
}

fn uuid_impl(_scope: &Scope) -> Obj {
    let mut bytes = [0u8; 16];
    with_random(|rng| {
        for chunk in bytes.chunks_mut(8) {
            let value = rng.next_u64().to_le_bytes();
            chunk.copy_from_slice(&value[..chunk.len()]);
        }
    });

    // the .NET GUID layout: the version and the variant bits
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let uuid = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    );

    obj::str(&uuid)
}

fn str_impl(scope: &Scope) -> Obj {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

    let length = scope::get(scope, "length");

    let length = match obj::payload(&length) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.string' argument 'length' is int"),
    };

    if length < 0 {
        return obj::err("'length' cannot be negative");
    }

    let text: String = with_random(|rng| {
        (0..length)
            .map(|_| CHARS[rng.below(CHARS.len() as u64) as usize] as char)
            .collect()
    });

    obj::str(&text)
}

fn weighted_impl(scope: &Scope) -> Obj {
    let values = scope::get(scope, "values");
    let weights = scope::get(scope, "weights");

    let value_list = match obj::payload(&values) {
        Payload::List(list) => list.borrow().clone(),
        _ => return obj::err("expected 'values' is list"),
    };
    let weight_list = match obj::payload(&weights) {
        Payload::List(list) => list.borrow().clone(),
        _ => return obj::err("expected 'weights' is list"),
    };

    if value_list.len() != weight_list.len() {
        return obj::err("'values' and 'weights' must have the same length");
    }

    let mut nums: Vec<f64> = Vec::with_capacity(weight_list.len());
    let mut total = 0.0;

    for weight in &weight_list {
        let value = match obj::payload(weight) {
            Payload::Float(v) => v,
            _ => return obj::err("weights must be float"),
        };

        if value < 0.0 {
            return obj::err("weights must be non-negative");
        }

        nums.push(value);
        total += value;
    }

    if total == 0.0 {
        return obj::err("sum of weights must be greater than zero");
    }

    let mut r = with_random(|rng| rng.next_double() * total);

    for (index, weight) in nums.iter().enumerate() {
        r -= weight;
        if r <= 0.0 {
            return value_list[index].clone();
        }
    }

    value_list.last().cloned().unwrap_or_else(obj::none)
}

fn range_impl(scope: &Scope) -> Obj {
    let start = scope::get(scope, "start");
    let end = scope::get(scope, "end");

    let start_value = match obj::payload(&start) {
        Payload::Int(v) => v,
        _ => return obj::err("expected 'random.range' argument 'start' is int"),
    };

    let (min, max) = if obj::is_none(&end) {
        (0, start_value - 1)
    } else {
        let end_value = match obj::payload(&end) {
            Payload::Int(v) => v,
            _ => return obj::err("expected 'random.range' argument 'end' is int"),
        };
        (start_value, end_value)
    };

    if min > max {
        return obj::err("'start' cannot be greater than 'end'");
    }

    let mut values: Vec<Obj> = (min..=max).map(obj::int).collect();
    shuffle_values(&mut values);

    collections::list(values)
}

fn value_impl(scope: &Scope) -> Obj {
    let target = scope::get(scope, "obj");

    let name = match obj::payload(&target) {
        Payload::TObj(types) => match types.name() {
            Some(name) => name.to_string(),
            None => return obj::err("expected 'random.value' argument is enum"),
        },
        _ => return obj::err("expected 'random.value' argument is enum"),
    };

    let Some(class) = crate::runtime::world::get_class(&name) else {
        return obj::err("expected 'random.value' argument is enum");
    };

    let members = match obj::payload(&class) {
        Payload::Enu(_) => class.borrow().members.clone(),
        _ => return obj::err("expected 'random.value' argument is enum"),
    };

    if members.is_empty() {
        return obj::err("expected 'random.value' argument is enum");
    }

    let index = with_random(|rng| rng.below(members.len() as u64) as usize);
    let value = members[index].1.clone();

    // the enum instance carries the value
    let created = obj::obj_new("enum", Payload::Enu(index as i64));
    obj::member_set(&created, "value", &value);
    created
}
