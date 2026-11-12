//! `Un.Native.Os` — the native `os` module.

use std::process::Command;

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "env".to_owned(),
            native(closure, "env", vec![arg("name", true, false, false)], env_impl),
        ),
        (
            "setenv".to_owned(),
            native(
                closure,
                "setenv",
                vec![arg("name", true, false, false), arg("value", true, false, false)],
                setenv_impl,
            ),
        ),
        (
            "unsetenv".to_owned(),
            native(
                closure,
                "unsetenv",
                vec![arg("name", true, false, false)],
                unsetenv_impl,
            ),
        ),
        (
            "environ".to_owned(),
            native(closure, "environ", Vec::new(), environ_impl),
        ),
        (
            "expand_vars".to_owned(),
            native(
                closure,
                "expand_vars",
                vec![arg("text", true, false, false)],
                expand_vars_impl,
            ),
        ),
        ("pid".to_owned(), native(closure, "pid", Vec::new(), pid_impl)),
        ("name".to_owned(), native(closure, "name", Vec::new(), name_impl)),
        ("arch".to_owned(), native(closure, "arch", Vec::new(), arch_impl)),
        (
            "version".to_owned(),
            native(closure, "version", Vec::new(), version_impl),
        ),
        (
            "cpuCount".to_owned(),
            native(closure, "cpuCount", Vec::new(), cpu_count_impl),
        ),
        (
            "is64bit".to_owned(),
            native(closure, "is64bit", Vec::new(), is_64bit_impl),
        ),
        (
            "hostname".to_owned(),
            native(closure, "hostname", Vec::new(), hostname_impl),
        ),
        (
            "username".to_owned(),
            native(closure, "username", Vec::new(), username_impl),
        ),
        ("args".to_owned(), native(closure, "args", Vec::new(), args_impl)),
        ("sep".to_owned(), native(closure, "sep", Vec::new(), sep_impl)),
        (
            "pathsep".to_owned(),
            native(closure, "pathsep", Vec::new(), pathsep_impl),
        ),
        (
            "linesep".to_owned(),
            native(closure, "linesep", Vec::new(), linesep_impl),
        ),
        (
            "tickCount".to_owned(),
            native(closure, "tickCount", Vec::new(), tick_count_impl),
        ),
        (
            "exit".to_owned(),
            native(
                closure,
                "exit",
                vec![arg("code", false, true, false)],
                exit_impl,
            ),
        ),
        (
            "exec".to_owned(),
            native(
                closure,
                "exec",
                vec![arg("command", true, false, false)],
                exec_impl,
            ),
        ),
    ]
}

/// `GetString` — the argument has to be a `Str`.
fn get_string(value: &Obj) -> Result<String, Obj> {
    match obj::payload(value) {
        Payload::Str(s) => Ok(s.to_string()),
        _ => Err(obj::err("expected value is string")),
    }
}

fn env_impl(scope: &Scope) -> Obj {
    env_of(&scope::get(scope, "name"))
}

/// `Os.Env` — the environment variable by name.
pub fn env_of(name: &Obj) -> Obj {
    let name = match get_string(name) {
        Ok(name) => name,
        Err(e) => return e,
    };

    match std::env::var(&name) {
        Ok(value) => obj::str(&value),
        Err(_) => obj::err("environment variable not found"),
    }
}

fn setenv_impl(scope: &Scope) -> Obj {
    let name = match get_string(&scope::get(scope, "name")) {
        Ok(name) => name,
        Err(e) => return e,
    };
    let value = match get_string(&scope::get(scope, "value")) {
        Ok(value) => value,
        Err(e) => return e,
    };

    std::env::set_var(name, value);
    obj::none()
}

fn unsetenv_impl(scope: &Scope) -> Obj {
    let name = match get_string(&scope::get(scope, "name")) {
        Ok(name) => name,
        Err(e) => return e,
    };

    std::env::remove_var(name);
    obj::none()
}

fn environ_impl(_scope: &Scope) -> Obj {
    environ_of()
}

/// `Os.Environ` — every environment variable as a `KEY=VALUE` entry.
pub fn environ_of() -> Obj {
    let mut entries: Vec<Obj> = Vec::new();

    for (key, value) in std::env::vars() {
        entries.push(obj::str(&format!("{key}={value}")));
    }

    collections::list(entries)
}

fn expand_vars_impl(scope: &Scope) -> Obj {
    expand_vars_of(&scope::get(scope, "text"))
}

/// `Os.ExpandVars`
pub fn expand_vars_of(text: &Obj) -> Obj {
    let text = match get_string(text) {
        Ok(text) => text,
        Err(e) => return e,
    };

    obj::str(&expand_environment_variables(&text))
}

/// `Environment.ExpandEnvironmentVariables` — the `%NAME%` (Windows) and `$NAME`
/// (Unix) forms.
fn expand_environment_variables(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if let Ok(value) = std::env::var(&name) {
                    out.push_str(&value);
                    i += end + 2;
                    continue;
                }
            }
        }

        if c == '$' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' {
                if let Some(end) = chars[i + 2..].iter().position(|c| *c == '}') {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    if let Ok(value) = std::env::var(&name) {
                        out.push_str(&value);
                        i += end + 3;
                        continue;
                    }
                }
            } else if next.is_ascii_alphabetic() || next == '_' {
                let mut end = i + 1;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                let name: String = chars[i + 1..end].iter().collect();
                if let Ok(value) = std::env::var(&name) {
                    out.push_str(&value);
                    i = end;
                    continue;
                }
            }
        }

        out.push(c);
        i += 1;
    }

    out
}

fn pid_impl(_scope: &Scope) -> Obj {
    obj::int(std::process::id() as i64)
}

fn name_impl(_scope: &Scope) -> Obj {
    if cfg!(target_os = "windows") {
        return obj::str("windows");
    }
    if cfg!(target_os = "linux") {
        return obj::str("linux");
    }
    if cfg!(target_os = "macos") {
        return obj::str("macos");
    }
    obj::str("unknown")
}

fn arch_impl(_scope: &Scope) -> Obj {
    if cfg!(target_arch = "x86_64") {
        obj::str("X64")
    } else if cfg!(target_arch = "aarch64") {
        obj::str("Arm64")
    } else if cfg!(target_arch = "x86") {
        obj::str("X86")
    } else {
        obj::str("unknown")
    }
}

fn version_impl(_scope: &Scope) -> Obj {
    // the reference reads `Environment.OSVersion.VersionString`
    obj::str(&format!(
        "{} {}",
        if cfg!(target_os = "windows") { "Microsoft Windows NT" } else { "Unix" },
        "10.0.0"
    ))
}

fn cpu_count_impl(_scope: &Scope) -> Obj {
    obj::int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

fn is_64bit_impl(_scope: &Scope) -> Obj {
    obj::boolean(cfg!(target_pointer_width = "64"))
}

fn hostname_impl(_scope: &Scope) -> Obj {
    obj::str(&std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default())
}

fn username_impl(_scope: &Scope) -> Obj {
    obj::str(&std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default())
}

fn args_impl(_scope: &Scope) -> Obj {
    let args: Vec<Obj> = std::env::args().map(|a| obj::str(&a)).collect();
    collections::list(args)
}

fn sep_impl(_scope: &Scope) -> Obj {
    obj::str(if cfg!(target_os = "windows") { "\\" } else { "/" })
}

fn pathsep_impl(_scope: &Scope) -> Obj {
    obj::str(if cfg!(target_os = "windows") { ";" } else { ":" })
}

fn linesep_impl(_scope: &Scope) -> Obj {
    obj::str(crate::error::nl())
}

fn tick_count_impl(_scope: &Scope) -> Obj {
    // `Environment.TickCount64` is the uptime of the machine, the Rust runtime has no
    // portable form of it, so the epoch milliseconds are used (D-051)
    obj::int(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    )
}

fn exit_impl(scope: &Scope) -> Obj {
    let code = scope::get(scope, "code");

    let mut value = 0i32;
    if !obj::is_none(&code) {
        match obj::payload(&code) {
            Payload::Int(v) => value = v as i32,
            _ => return obj::err("expected code is int"),
        }
    }

    crate::runtime::world::flush_stdout();
    std::process::exit(value);
}

fn exec_impl(scope: &Scope) -> Obj {
    let command = match get_string(&scope::get(scope, "command")) {
        Ok(command) => command,
        Err(e) => return e,
    };

    if command.trim().is_empty() {
        return obj::err("command is empty");
    }

    if command.chars().count() > 8192 {
        return obj::err("command too long");
    }

    let mut process = if cfg!(target_os = "windows") {
        let mut process = Command::new("cmd.exe");
        process.arg("/c").arg(&command);
        process
    } else {
        let mut process = Command::new("/bin/sh");
        process.arg("-c").arg(&command);
        process
    };

    let output = process.output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if !output.status.success() && stdout.trim().is_empty() {
                if stderr.trim().is_empty() {
                    return obj::err(format!(
                        "command failed with exit code {}",
                        output.status.code().unwrap_or(-1)
                    ));
                }
                return obj::err(stderr.trim());
            }

            obj::str(&stdout)
        }
        Err(e) => obj::err(&e.to_string()),
    }
}
