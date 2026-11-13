use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use super::{arg, native};
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let one = |name: &str| vec![arg(name, true, false, false)];
    vec![
        (
            "year".to_owned(),
            native(closure, "year", Vec::new(), year),
        ),
        (
            "month".to_owned(),
            native(closure, "month", Vec::new(), month),
        ),
        (
            "day".to_owned(),
            native(closure, "day", Vec::new(), day),
        ),
        (
            "hour".to_owned(),
            native(closure, "hour", Vec::new(), hour),
        ),
        (
            "minute".to_owned(),
            native(closure, "minute", Vec::new(), minute),
        ),
        (
            "second".to_owned(),
            native(closure, "second", Vec::new(), second),
        ),
        ("ms".to_owned(), native(closure, "ms", Vec::new(), ms)),
        (
            "timestamp".to_owned(),
            native(closure, "timestamp", Vec::new(), timestamp),
        ),
        (
            "timestamp_ms".to_owned(),
            native(closure, "timestamp_ms", Vec::new(), timestamp_ms),
        ),
        (
            "format".to_owned(),
            native(closure, "format", one("format"), format),
        ),
        (
            "add_years".to_owned(),
            native(closure, "add_years", one("years"), add_years),
        ),
        (
            "add_months".to_owned(),
            native(closure, "add_months", one("months"), add_months),
        ),
        (
            "add_days".to_owned(),
            native(closure, "add_days", one("days"), add_days),
        ),
        (
            "add_hours".to_owned(),
            native(closure, "add_hours", one("hours"), add_hours),
        ),
        (
            "add_minutes".to_owned(),
            native(closure, "add_minutes", one("minutes"), add_minutes),
        ),
        (
            "add_seconds".to_owned(),
            native(closure, "add_seconds", one("seconds"), add_seconds),
        ),
        (
            "add_ms".to_owned(),
            native(closure, "add_ms", one("milliseconds"), add_ms),
        ),
        (
            "weekday".to_owned(),
            native(closure, "weekday", Vec::new(), weekday),
        ),
        (
            "day_of_year".to_owned(),
            native(closure, "day_of_year", Vec::new(), day_of_year),
        ),
        (
            "days_in_month".to_owned(),
            native(closure, "days_in_month", Vec::new(), days_in_month),
        ),
        (
            "is_leap_year".to_owned(),
            native(closure, "is_leap_year", Vec::new(), is_leap_year),
        ),
        (
            "date".to_owned(),
            native(closure, "date", Vec::new(), date_only),
        ),
        (
            "before".to_owned(),
            native(closure, "before", one("other"), before),
        ),
        (
            "after".to_owned(),
            native(closure, "after", one("other"), after),
        ),
        (
            "compare".to_owned(),
            native(closure, "compare", one("other"), compare),
        ),
        (
            "to_utc".to_owned(),
            native(closure, "to_utc", Vec::new(), to_utc),
        ),
        (
            "to_local".to_owned(),
            native(closure, "to_local", Vec::new(), to_local),
        ),
    ]
}
fn date_self(scope: &Scope) -> Option<i64> {
    match obj::payload(&scope::get(scope, "self")) {
        Payload::Date(ticks) => Some(ticks),
        _ => None,
    }
}
fn year(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::year(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn month(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::month(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn day(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::day(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn hour(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::hour(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn minute(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::minute(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn second(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::second(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn ms(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::ms(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn timestamp(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::timestamp(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn timestamp_ms(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::timestamp_ms(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn format(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let format = scope::get(scope, "format");
    let format = match obj::payload(&format) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'format' argument to be of type 'str'"),
    };
    obj::str(&crate::runtime::date::format_pattern(ticks, &format))
}
fn amount(scope: &Scope, name: &str) -> Option<i64> {
    match obj::payload(&scope::get(scope, name)) {
        Payload::Int(v) => Some(v),
        _ => None,
    }
}
fn add_years(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(years) = amount(scope, "years") else {
        return obj::err("expected 'years' argument to be of type 'int'");
    };
    let (year, month, day, hour, minute, second, ms) = crate::runtime::date::parts(ticks);
    let ticks = crate::runtime::date::ticks_from_civil(
        year + years,
        month,
        day,
        hour,
        minute,
        second,
        ms,
    );
    obj::date_obj(ticks)
}
fn add_months(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(months) = amount(scope, "months") else {
        return obj::err("expected 'months' argument to be of type 'int'");
    };
    let (year, month, day, hour, minute, second, ms) = crate::runtime::date::parts(ticks);
    let total = year * 12 + (month - 1) + months;
    let new_year = total.div_euclid(12);
    let new_month = total.rem_euclid(12) + 1;
    let new_day = day.min(crate::runtime::date::days_in_month(new_year, new_month));
    let ticks = crate::runtime::date::ticks_from_civil(
        new_year,
        new_month,
        new_day,
        hour,
        minute,
        second,
        ms,
    );
    obj::date_obj(ticks)
}
fn add_days(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(days) = amount(scope, "days") else {
        return obj::err("expected 'days' argument to be of type 'int'");
    };
    obj::date_obj(ticks + days * crate::runtime::date::TICKS_PER_DAY)
}
fn add_hours(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(hours) = amount(scope, "hours") else {
        return obj::err("expected 'hours' argument to be of type 'int'");
    };
    obj::date_obj(ticks + hours * 36_000_000_000)
}
fn add_minutes(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(minutes) = amount(scope, "minutes") else {
        return obj::err("expected 'minutes' argument to be of type 'int'");
    };
    obj::date_obj(ticks + minutes * 600_000_000)
}
fn add_seconds(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(seconds) = amount(scope, "seconds") else {
        return obj::err("expected 'seconds' argument to be of type 'int'");
    };
    obj::date_obj(ticks + seconds * 10_000_000)
}
fn add_ms(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let Some(ms) = amount(scope, "milliseconds") else {
        return obj::err("expected 'milliseconds' argument to be of type 'int'");
    };
    obj::date_obj(ticks + ms * 10_000)
}
fn weekday(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::weekday(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn day_of_year(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::int(crate::runtime::date::day_of_year(ticks)),
        None => obj::err("expected 'date' object"),
    }
}
fn days_in_month(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => {
            let (year, month, ..) = crate::runtime::date::parts(ticks);
            obj::int(crate::runtime::date::days_in_month(year, month))
        }
        None => obj::err("expected 'date' object"),
    }
}
fn is_leap_year(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => {
            let (year, ..) = crate::runtime::date::parts(ticks);
            obj::boolean(crate::runtime::date::is_leap_year(year))
        }
        None => obj::err("expected 'date' object"),
    }
}
fn date_only(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => {
            let (year, month, day, ..) = crate::runtime::date::parts(ticks);
            let ticks = crate::runtime::date::ticks_from_civil(year, month, day, 0, 0, 0, 0);
            obj::date_obj(ticks)
        }
        None => obj::err("expected 'date' object"),
    }
}
fn before(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let other = scope::get(scope, "other");
    match obj::payload(&other) {
        Payload::Date(other) => obj::boolean(ticks < other),
        _ => obj::err("expected 'other' argument to be of type 'date'"),
    }
}
fn after(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let other = scope::get(scope, "other");
    match obj::payload(&other) {
        Payload::Date(other) => obj::boolean(ticks > other),
        _ => obj::err("expected 'other' argument to be of type 'date'"),
    }
}
fn compare(scope: &Scope) -> Obj {
    let Some(ticks) = date_self(scope) else {
        return obj::err("expected 'date' object");
    };
    let other = scope::get(scope, "other");
    match obj::payload(&other) {
        Payload::Date(other) => {
            if ticks == other {
                obj::int(0)
            } else if ticks < other {
                obj::int(-1)
            } else {
                obj::int(1)
            }
        }
        _ => obj::err("expected 'other' argument to be of type 'date'"),
    }
}
fn to_utc(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::date_obj(ticks),
        None => obj::err("expected 'date' object"),
    }
}
fn to_local(scope: &Scope) -> Obj {
    match date_self(scope) {
        Some(ticks) => obj::date_obj(ticks),
        None => obj::err("expected 'date' object"),
    }
}
