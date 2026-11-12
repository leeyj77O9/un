//! `Un.Native.Iter` — the native `iter` module.

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::function;
use crate::runtime::iters;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "array".to_owned(),
            native(
                closure,
                "array",
                vec![arg("value", true, false, false), arg("size", false, false, true)],
                array_impl,
            ),
        ),
        (
            "range".to_owned(),
            native(
                closure,
                "range",
                vec![
                    arg("start", true, false, false),
                    arg("end", false, true, false),
                    arg("step", false, true, false),
                ],
                range_impl,
            ),
        ),
        (
            "counter".to_owned(),
            native(
                closure,
                "counter",
                vec![arg("start", false, true, false)],
                counter_impl,
            ),
        ),
        (
            "reverse".to_owned(),
            native(
                closure,
                "reverse",
                vec![arg("obj", true, false, false)],
                reverse_impl,
            ),
        ),
        (
            "repeat".to_owned(),
            native(
                closure,
                "repeat",
                vec![
                    arg("obj", true, false, false),
                    arg("count", false, true, false),
                ],
                repeat_impl,
            ),
        ),
        (
            "zip".to_owned(),
            native(
                closure,
                "zip",
                vec![arg("iterables", false, false, true)],
                zip_impl,
            ),
        ),
        (
            "enumerate".to_owned(),
            native(
                closure,
                "enumerate",
                vec![arg("iterable", true, false, false)],
                enumerate_impl,
            ),
        ),
        (
            "sum".to_owned(),
            native(
                closure,
                "sum",
                vec![arg("iterable", false, false, true)],
                sum_impl,
            ),
        ),
        (
            "max".to_owned(),
            native(
                closure,
                "max",
                vec![arg("iterable", false, false, true)],
                max_impl,
            ),
        ),
        (
            "min".to_owned(),
            native(
                closure,
                "min",
                vec![arg("iterable", false, false, true)],
                min_impl,
            ),
        ),
        (
            "filter".to_owned(),
            native(
                closure,
                "filter",
                vec![
                    arg("predicate", true, false, false),
                    arg("iterable", true, false, false),
                ],
                filter_impl,
            ),
        ),
        (
            "map".to_owned(),
            native(
                closure,
                "map",
                vec![
                    arg("transform", true, false, false),
                    arg("iterable", true, false, false),
                ],
                map_impl,
            ),
        ),
        (
            "take".to_owned(),
            native(
                closure,
                "take",
                vec![arg("count", true, false, false), arg("iterable", true, false, false)],
                take_impl,
            ),
        ),
        (
            "skip".to_owned(),
            native(
                closure,
                "skip",
                vec![arg("count", true, false, false), arg("iterable", true, false, false)],
                skip_impl,
            ),
        ),
        (
            "chain".to_owned(),
            native(
                closure,
                "chain",
                vec![arg("iterables", false, false, true)],
                chain_impl,
            ),
        ),
        (
            "flatten".to_owned(),
            native(
                closure,
                "flatten",
                vec![arg("iterable", true, false, false)],
                flatten_impl,
            ),
        ),
        (
            "all".to_owned(),
            native(
                closure,
                "all",
                vec![
                    arg("predicate", true, false, false),
                    arg("iterable", true, false, false),
                ],
                all_impl,
            ),
        ),
        (
            "any".to_owned(),
            native(
                closure,
                "any",
                vec![
                    arg("predicate", true, false, false),
                    arg("iterable", true, false, false),
                ],
                any_impl,
            ),
        ),
        (
            "count".to_owned(),
            native(
                closure,
                "count",
                vec![
                    arg("predicate", true, false, false),
                    arg("iterable", true, false, false),
                ],
                count_impl,
            ),
        ),
        (
            "sorted".to_owned(),
            native(
                closure,
                "sorted",
                vec![arg("iterable", true, false, false)],
                sorted_impl,
            ),
        ),
    ]
}

/// the values of an iterable argument.
fn iter_values(value: &Obj) -> Result<Vec<Obj>, Obj> {
    let iter = obj::unary(value, obj::UnOp::Iter);
    if !matches!(obj::payload(&iter), Payload::Iter(_)) {
        return Err(obj::err("expected 'iterable' argument to be of type 'iter'"));
    }

    let list = obj::unary(&iter, obj::UnOp::ToList);
    match obj::payload(&list) {
        Payload::List(values) => Ok(values.borrow().clone()),
        _ => Err(obj::err("expected 'iterable' argument to be of type 'iter'")),
    }
}

/// `NormalizeToTuple` — the values of a tuple or a list. a single element is
/// unwrapped: a list or a range becomes its values, a tuple stays, and an iterator
/// that is not a tuple is an error. the result is converted with `ToTuple()` again, so
/// a string becomes its characters.
fn normalize_to_tuple(value: &Obj) -> Result<Vec<Obj>, Obj> {
    let raw = obj::to_tuple(value);
    let raw_values = match obj::payload(&raw) {
        Payload::Tup(values, _) => (*values).clone(),
        _ => return Err(obj::err("expected 'iterable' argument to be a tuple")),
    };

    if raw_values.is_empty() {
        return Err(obj::err(
            "expected 'iterable' argument to have at least one value",
        ));
    }

    let values: Obj = if raw_values.len() > 1 {
        collections::tup(raw_values, Vec::new())
    } else {
        let item = &raw_values[0];
        match obj::payload(item) {
            Payload::List(values) => collections::tup(values.borrow().clone(), Vec::new()),
            Payload::Tup(values, _) => collections::tup((*values).clone(), Vec::new()),
            Payload::Iter(st) => {
                // only a range carries its values, the other iterators are an error
                let (kind, _) = iters::head(&st);
                if kind != iters::Kind::Range {
                    return Err(obj::err("invalid argument type of 'iterable'"));
                }

                let tuple = obj::unary(item, obj::UnOp::ToTuple);
                match obj::payload(&tuple) {
                    Payload::Tup(_, _) => tuple,
                    Payload::Error { message, .. } => return Err(obj::err(message)),
                    _ => return Err(obj::err("invalid argument type of 'iterable'")),
                }
            }
            _ => item.clone(),
        }
    };

    // the final `values.ToTuple()`
    let tuple = obj::to_tuple(&values);
    match obj::payload(&tuple) {
        Payload::Tup(values, _) => Ok((*values).clone()),
        _ => Err(obj::err("expected 'iterable' argument to be a tuple")),
    }
}

fn array_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "value");
    let size = scope::get(scope, "size");

    let size_list = match obj::payload(&obj::to_list(&size)) {
        Payload::List(values) => values.borrow().clone(),
        _ => return obj::err("expected 'size' argument to be a 'list'"),
    };

    // a nested list is the dimension list
    let size_list = if size_list.len() == 1 {
        match obj::payload(&size_list[0]) {
            Payload::List(values) => values.borrow().clone(),
            _ => size_list,
        }
    } else {
        size_list
    };

    let mut sizes: Vec<i64> = Vec::new();
    for item in &size_list {
        let converted = obj::unary(item, obj::UnOp::ToInt);
        match obj::payload(&converted) {
            Payload::Int(v) => sizes.push(v),
            _ => return obj::err("expected all items in 'size' list to be integers"),
        }
    }

    if sizes.is_empty() {
        return obj::err("expected 'size' argument to have at least one dimension");
    }

    collections::list(create(&value, &sizes))
}

/// the nested list of `array`.
fn create(value: &Obj, lengths: &[i64]) -> Vec<Obj> {
    let mut list = Vec::new();

    for _ in 0..lengths[0] {
        if lengths.len() == 1 {
            list.push(obj::clone_obj(value));
        } else {
            list.push(collections::list(create(value, &lengths[1..])));
        }
    }

    list
}

fn range_impl(scope: &Scope) -> Obj {
    let start = scope::get(scope, "start");
    let end = scope::get(scope, "end");
    let step = scope::get(scope, "step");

    let start = match to_int(&start) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let end = if obj::is_none(&end) {
        0
    } else {
        match to_int(&end) {
            Ok(v) => v,
            Err(e) => return e,
        }
    };
    let step = if obj::is_none(&step) {
        1
    } else {
        match to_int(&step) {
            Ok(v) => v,
            Err(e) => return e,
        }
    };

    if step == 0 {
        return obj::err("step argument must not be zero");
    }

    let (start, end) = if start > end && step > 0 {
        (end, start)
    } else {
        (start, end)
    };

    iters::range_new(start, end, step)
}

fn counter_impl(scope: &Scope) -> Obj {
    let start = scope::get(scope, "start");

    let start = if obj::is_none(&start) {
        0
    } else {
        match to_int(&start) {
            Ok(v) => v,
            Err(e) => return e,
        }
    };

    iters::counter_new(start)
}

fn reverse_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "obj");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    // the `Reverse` stage walks the values backwards
    iters::reverse_new(values)
}

fn repeat_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "obj");
    let count = scope::get(scope, "count");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let count = if obj::is_none(&count) {
        -1
    } else {
        match obj::payload(&count) {
            Payload::Int(v) => v,
            _ => return obj::err("expected 'count' argument to be of type 'int'"),
        }
    };

    iters::repeat_source(values, count)
}

fn zip_impl(scope: &Scope) -> Obj {
    let iterables = scope::get(scope, "iterables");

    let arrays = match iter_values(&iterables) {
        Ok(values) => values,
        Err(e) => return e,
    };

    // the reference walks the enumerators in parallel, so an infinite sequence is fine
    let mut enumerators: Vec<iters::Enumerator> = Vec::new();
    for array in &arrays {
        let iter = obj::unary(array, obj::UnOp::Iter);
        match obj::payload(&iter) {
            Payload::Iter(st) => {
                let def = iters::def_of(&st);
                enumerators.push(iters::Enumerator::new(&def));
            }            _ => return obj::err("expected all 'iterables' items to be of type 'iter'"),
        }
    }

    if enumerators.is_empty() {
        return obj::err("expected 'iterables' argument to have a non-zero length");
    }

    let mut result: Vec<Obj> = Vec::new();

    loop {
        let mut buffer: Vec<Obj> = Vec::new();
        let mut done = false;

        for enumerator in enumerators.iter_mut() {
            match enumerator.next() {
                Some(value) => buffer.push(value),
                None => {
                    done = true;
                    break;
                }
            }
        }

        if done {
            break;
        }

        result.push(collections::tup(buffer, Vec::new()));
    }

    collections::list(result)
}

fn enumerate_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let pairs: Vec<Obj> = values
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            collections::tup(
                vec![obj::int(index as i64), item],
                vec!["index".to_owned(), "value".to_owned()],
            )
        })
        .collect();

    iters::seq(pairs)
}

fn sum_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let values = match normalize_to_tuple(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    if values.is_empty() {
        return obj::err("expected 'iterable' argument to be a tuple");
    }

    let mut total = values[0].clone();
    for item in values.iter().skip(1) {
        let sum = obj::binop(&total, obj::BinOp::Add, item);
        if obj::is_err(&sum) {
            return sum;
        }
        total = sum;
    }

    total
}

fn max_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let values = match normalize_to_tuple(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    if values.is_empty() {
        return obj::err("expected 'iterable' argument to be a tuple");
    }

    let mut max = values[0].clone();
    for item in values.iter().skip(1) {
        let less = obj::binop(&max, obj::BinOp::Lt, item);
        match obj::payload(&less) {
            Payload::Bool(true) => max = item.clone(),
            Payload::Bool(false) => {}
            _ => {
                return obj::err(format!(
                    "{} < {} is not a boolean",
                    obj::display(&max),
                    obj::display(item)
                ))
            }
        }
    }

    max
}

fn min_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let values = match normalize_to_tuple(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    if values.is_empty() {
        return obj::err("expected 'iterable' argument to be a tuple");
    }

    let mut min = values[0].clone();
    for item in values.iter().skip(1) {
        let greater = obj::binop(&min, obj::BinOp::Gt, item);
        match obj::payload(&greater) {
            Payload::Bool(true) => min = item.clone(),
            Payload::Bool(false) => {}
            _ => {
                return obj::err(format!(
                    "{} > {} is not a boolean",
                    obj::display(&min),
                    obj::display(item)
                ))
            }
        }
    }

    min
}

fn filter_impl(scope: &Scope) -> Obj {
    let predicate = scope::get(scope, "predicate");
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut list: Vec<Obj> = Vec::new();
    for item in values {
        let result = function::fn_call(&predicate, &function::Args::positional(vec![item.clone()]));
        if obj::is_err(&result) {
            return result;
        }

        match obj::payload(&result) {
            Payload::Bool(true) => list.push(item),
            Payload::Bool(false) => {}
            _ => return obj::err("expected 'predicate' to return a boolean"),
        }
    }

    collections::list(list)
}

fn map_impl(scope: &Scope) -> Obj {
    let transform = scope::get(scope, "transform");
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut list: Vec<Obj> = Vec::new();
    for item in values {
        let result = function::fn_call(&transform, &function::Args::positional(vec![item]));
        if obj::is_err(&result) {
            return result;
        }
        list.push(result);
    }

    collections::list(list)
}

fn take_impl(scope: &Scope) -> Obj {
    let count = scope::get(scope, "count");
    let value = scope::get(scope, "iterable");

    let n = match to_int(&count) {
        Ok(v) => v,
        Err(e) => return e,
    };

    if n < 0 {
        return obj::err("expected 'count' argument to be non-negative");
    }

    // the reference's `Take` is lazy, so an infinite iterator is bounded by the count
    let iter = obj::unary(&value, obj::UnOp::Iter);
    if !matches!(obj::payload(&iter), Payload::Iter(_)) {
        return obj::err("expected 'iterable' argument to be of type 'iter'");
    }

    let taken = iters::take_of(&match obj::payload(&iter) {
        Payload::Iter(st) => st.clone(),
        _ => return obj::err("expected 'iterable' argument to be of type 'iter'"),
    }, n);

    let list = obj::unary(&taken, obj::UnOp::ToList);
    list
}

fn skip_impl(scope: &Scope) -> Obj {
    let count = scope::get(scope, "count");
    let value = scope::get(scope, "iterable");

    let n = match to_int(&count) {
        Ok(v) => v,
        Err(e) => return e,
    };

    if n < 0 {
        return obj::err("expected 'count' argument to be non-negative");
    }

    let iter = obj::unary(&value, obj::UnOp::Iter);
    if !matches!(obj::payload(&iter), Payload::Iter(_)) {
        return obj::err("expected 'iterable' argument to be of type 'iter'");
    }

    let skipped = iters::skip_of(&match obj::payload(&iter) {
        Payload::Iter(st) => st.clone(),
        _ => return obj::err("expected 'iterable' argument to be of type 'iter'"),
    }, n);

    let list = obj::unary(&skipped, obj::UnOp::ToList);
    list
}

fn chain_impl(scope: &Scope) -> Obj {
    let iterables = scope::get(scope, "iterables");

    let arrays = match iter_values(&iterables) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut list: Vec<Obj> = Vec::new();
    for array in &arrays {
        match iter_values(array) {
            Ok(values) => list.extend(values),
            Err(e) => return e,
        }
    }

    collections::list(list)
}

fn flatten_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut list: Vec<Obj> = Vec::new();
    for item in values {
        let inner = obj::unary(&item, obj::UnOp::Iter);
        if matches!(obj::payload(&inner), Payload::Iter(_)) {
            let sub = obj::unary(&inner, obj::UnOp::ToList);
            if let Payload::List(values) = obj::payload(&sub) {
                list.extend(values.borrow().iter().cloned());
                continue;
            }
        }
        list.push(item);
    }

    collections::list(list)
}

fn all_impl(scope: &Scope) -> Obj {
    let predicate = scope::get(scope, "predicate");
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    for item in values {
        let result = function::fn_call(&predicate, &function::Args::positional(vec![item]));
        if obj::is_err(&result) {
            return result;
        }

        match obj::payload(&result) {
            Payload::Bool(true) => {}
            Payload::Bool(false) => return obj::boolean(false),
            _ => return obj::err("expected 'predicate' to return a boolean"),
        }
    }

    obj::boolean(true)
}

fn any_impl(scope: &Scope) -> Obj {
    let predicate = scope::get(scope, "predicate");
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    for item in values {
        let result = function::fn_call(&predicate, &function::Args::positional(vec![item]));
        if obj::is_err(&result) {
            return result;
        }

        match obj::payload(&result) {
            Payload::Bool(true) => return obj::boolean(true),
            Payload::Bool(false) => {}
            _ => return obj::err("expected 'predicate' to return a boolean"),
        }
    }

    obj::boolean(false)
}

fn count_impl(scope: &Scope) -> Obj {
    let predicate = scope::get(scope, "predicate");
    let value = scope::get(scope, "iterable");

    let values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    let mut total = 0i64;
    for item in values {
        let result = function::fn_call(&predicate, &function::Args::positional(vec![item]));
        if obj::is_err(&result) {
            return result;
        }

        match obj::payload(&result) {
            Payload::Bool(true) => total += 1,
            Payload::Bool(false) => {}
            _ => return obj::err("expected 'predicate' to return a boolean"),
        }
    }

    obj::int(total)
}

fn sorted_impl(scope: &Scope) -> Obj {
    let value = scope::get(scope, "iterable");

    let mut values = match iter_values(&value) {
        Ok(values) => values,
        Err(e) => return e,
    };

    values.sort_by(|a, b| match obj::compare_to(a, b) {
        Ok(order) => order.cmp(&0),
        Err(_) => std::cmp::Ordering::Equal,
    });

    collections::list(values)
}

/// `ToInt().As<Int>(out var value)`
fn to_int(value: &Obj) -> Result<i64, Obj> {
    let converted = obj::unary(value, obj::UnOp::ToInt);
    match obj::payload(&converted) {
        Payload::Int(v) => Ok(v),
        _ => Err(obj::err("expected 'start' argument to be an integer")),
    }
}
