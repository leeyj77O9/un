//! `Un.Native.Fs` — the native `fs` module.

use std::path::{Path, PathBuf};

use crate::runtime::collections;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};

use super::{arg, native};

pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    let path = |name: &str| vec![arg(name, true, false, false)];
    let two = |a: &str, b: &str| vec![arg(a, true, false, false), arg(b, true, false, false)];

    vec![
        (
            "exists".to_owned(),
            native(closure, "exists", path("path"), exists_impl),
        ),
        (
            "file".to_owned(),
            native(closure, "file", path("path"), file_impl),
        ),
        (
            "dir".to_owned(),
            native(closure, "dir", path("path"), dir_impl),
        ),
        (
            "read".to_owned(),
            native(closure, "read", path("path"), read_impl),
        ),
        (
            "write".to_owned(),
            native(
                closure,
                "write",
                vec![arg("path", true, false, false), arg("text", true, false, false)],
                write_impl,
            ),
        ),
        (
            "append".to_owned(),
            native(
                closure,
                "append",
                vec![arg("path", true, false, false), arg("text", true, false, false)],
                append_impl,
            ),
        ),
        (
            "delete".to_owned(),
            native(closure, "delete", path("path"), delete_impl),
        ),
        (
            "copy".to_owned(),
            native(closure, "copy", two("source", "destination"), copy_impl),
        ),
        (
            "move".to_owned(),
            native(closure, "move", two("source", "destination"), move_impl),
        ),
        (
            "rename".to_owned(),
            native(closure, "rename", two("path", "name"), rename_impl),
        ),
        (
            "touch".to_owned(),
            native(closure, "touch", path("path"), touch_impl),
        ),
        (
            "mkdir".to_owned(),
            native(closure, "mkdir", path("path"), mkdir_impl),
        ),
        (
            "rmdir".to_owned(),
            native(
                closure,
                "rmdir",
                vec![arg("path", true, false, false), arg("recursive", false, true, false)],
                rmdir_impl,
            ),
        ),
        (
            "list".to_owned(),
            native(closure, "list", path("path"), list_impl),
        ),
        (
            "files".to_owned(),
            native(closure, "files", path("path"), files_impl),
        ),
        (
            "dirs".to_owned(),
            native(closure, "dirs", path("path"), dirs_impl),
        ),
        (
            "walk".to_owned(),
            native(closure, "walk", path("path"), walk_impl),
        ),
        (
            "size".to_owned(),
            native(closure, "size", path("path"), size_impl),
        ),
        (
            "created".to_owned(),
            native(closure, "created", path("path"), created_impl),
        ),
        (
            "modified".to_owned(),
            native(closure, "modified", path("path"), modified_impl),
        ),
        (
            "accessed".to_owned(),
            native(closure, "accessed", path("path"), accessed_impl),
        ),
        (
            "readonly".to_owned(),
            native(closure, "readonly", path("path"), readonly_impl),
        ),
        (
            "hidden".to_owned(),
            native(closure, "hidden", path("path"), hidden_impl),
        ),
        (
            "join".to_owned(),
            native(
                closure,
                "join",
                vec![arg("paths", false, false, true)],
                join_impl,
            ),
        ),
        (
            "parent".to_owned(),
            native(closure, "parent", path("path"), parent_impl),
        ),
        (
            "name".to_owned(),
            native(closure, "name", path("path"), name_impl),
        ),
        (
            "stem".to_owned(),
            native(closure, "stem", path("path"), stem_impl),
        ),
        (
            "ext".to_owned(),
            native(closure, "ext", path("path"), ext_impl),
        ),
        (
            "absolute".to_owned(),
            native(closure, "absolute", path("path"), absolute_impl),
        ),
        (
            "relative".to_owned(),
            native(closure, "relative", two("from", "to"), relative_impl),
        ),
        (
            "normalize".to_owned(),
            native(closure, "normalize", path("path"), normalize_impl),
        ),
        ("cwd".to_owned(), native(closure, "cwd", Vec::new(), cwd_impl)),
        (
            "chdir".to_owned(),
            native(closure, "chdir", path("path"), chdir_impl),
        ),
        (
            "temp".to_owned(),
            native(closure, "temp", Vec::new(), temp_impl),
        ),
        (
            "home".to_owned(),
            native(closure, "home", Vec::new(), home_impl),
        ),
        ("exe".to_owned(), native(closure, "exe", Vec::new(), exe_impl)),
        ("app".to_owned(), native(closure, "app", Vec::new(), app_impl)),
    ]
}

/// `GetPath` — the argument has to be a `Str`.
fn get_path(value: &Obj) -> Result<String, Obj> {
    match obj::payload(value) {
        Payload::Str(s) => Ok(s.to_string()),
        _ => Err(obj::err("expected path is string")),
    }
}

/// the path argument of a member.
fn path_arg(scope: &Scope, name: &str) -> Result<String, Obj> {
    get_path(&scope::get(scope, name))
}

fn exists_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    obj::boolean(path.exists())
}

fn file_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    obj::boolean(Path::new(&path).is_file())
}

fn dir_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    obj::boolean(Path::new(&path).is_dir())
}

fn read_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    if !Path::new(&path).is_file() {
        return obj::err("file not found");
    }

    match std::fs::read_to_string(&path) {
        Ok(text) => obj::str(&text),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn write_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let text = match get_path(&scope::get(scope, "text")) {
        Ok(text) => text,
        Err(e) => return e,
    };

    match std::fs::write(&path, text) {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn append_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let text = match get_path(&scope::get(scope, "text")) {
        Ok(text) => text,
        Err(e) => return e,
    };

    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()));

    match result {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn delete_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    let result = if path.is_file() {
        std::fs::remove_file(path)
    } else if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        Ok(())
    };

    match result {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn copy_impl(scope: &Scope) -> Obj {
    let source = match path_arg(scope, "source") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let destination = match path_arg(scope, "destination") {
        Ok(path) => path,
        Err(e) => return e,
    };

    match std::fs::copy(&source, &destination) {
        Ok(_) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn move_impl(scope: &Scope) -> Obj {
    let source = match path_arg(scope, "source") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let destination = match path_arg(scope, "destination") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let result = if Path::new(&destination).exists() {
        std::fs::remove_file(&destination).and_then(|_| std::fs::rename(&source, &destination))
    } else {
        std::fs::rename(&source, &destination)
    };

    match result {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn rename_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let name = match get_path(&scope::get(scope, "name")) {
        Ok(name) => name,
        Err(e) => return e,
    };

    let parent = Path::new(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(""));
    let destination = parent.join(name);

    let result = if destination.exists() {
        std::fs::remove_file(&destination).and_then(|_| std::fs::rename(&path, &destination))
    } else {
        std::fs::rename(&path, &destination)
    };

    match result {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn touch_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    if Path::new(&path).is_file() {
        return obj::none();
    }

    match std::fs::File::create(&path) {
        Ok(_) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn mkdir_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    match std::fs::create_dir_all(&path) {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn rmdir_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let recursive = scope::get(scope, "recursive");

    let mut value = true;
    if !obj::is_none(&recursive) {
        match obj::payload(&recursive) {
            Payload::Bool(v) => value = v,
            _ => return obj::err("expected recursive is bool"),
        }
    }

    let path = Path::new(&path);
    let result = if value {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_dir(path)
    };

    match result {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

/// the entries of a directory, the reference enumerates them in the file system's order.
fn entries(path: &Path) -> Option<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    Some(entries)
}

fn list_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_dir() {
        return obj::err("directory not found");
    }

    match entries(path) {
        Some(entries) => collections::list(
            entries
                .iter()
                .map(|p| obj::str(&p.display().to_string()))
                .collect(),
        ),
        None => obj::err("directory not found"),
    }
}

fn files_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_dir() {
        return obj::err("directory not found");
    }

    match entries(path) {
        Some(entries) => collections::list(
            entries
                .iter()
                .filter(|p| p.is_file())
                .map(|p| obj::str(&p.display().to_string()))
                .collect(),
        ),
        None => obj::err("directory not found"),
    }
}

fn dirs_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_dir() {
        return obj::err("directory not found");
    }

    match entries(path) {
        Some(entries) => collections::list(
            entries
                .iter()
                .filter(|p| p.is_dir())
                .map(|p| obj::str(&p.display().to_string()))
                .collect(),
        ),
        None => obj::err("directory not found"),
    }
}

fn walk_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_dir() {
        return obj::err("directory not found");
    }

    let mut found: Vec<PathBuf> = Vec::new();
    walk_dir(path, &mut found);
    found.sort();

    collections::list(
        found
            .iter()
            .map(|p| obj::str(&p.display().to_string()))
            .collect(),
    )
}

fn walk_dir(path: &Path, found: &mut Vec<PathBuf>) {
    let Ok(reader) = std::fs::read_dir(path) else {
        return;
    };

    for entry in reader.flatten() {
        let entry_path = entry.path();
        found.push(entry_path.clone());
        if entry_path.is_dir() {
            walk_dir(&entry_path, found);
        }
    }
}

fn size_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_file() {
        return obj::err("file not found");
    }

    match std::fs::metadata(path) {
        Ok(meta) => obj::int(meta.len() as i64),
        Err(e) => obj::err(&e.to_string()),
    }
}

/// the "O" format of .NET: the round-trip time of a UTC timestamp.
fn time_string(time: std::time::SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = duration.as_nanos() / 100;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:07}Z",
        crate::runtime::date::civil_from_days((ticks / 10_000_000_000) as i64).0,
        crate::runtime::date::civil_from_days((ticks / 10_000_000_000) as i64).1,
        crate::runtime::date::civil_from_days((ticks / 10_000_000_000) as i64).2,
        (ticks / 3_600_000_000) % 24,
        (ticks / 60_000_000) % 60,
        (ticks / 1_000_000) % 60,
        (ticks % 10_000_000) * 10,
    )
}

fn created_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    match created_time(Path::new(&path)) {
        Some(time) => obj::str(&time_string(time)),
        None => obj::err("file not found"),
    }
}

fn created_time(path: &Path) -> Option<std::time::SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    let created = meta.created().ok()?;
    Some(created)
}

fn modified_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.exists() {
        return obj::err("file not found");
    }

    match std::fs::metadata(path).and_then(|meta| meta.modified()) {
        Ok(time) => obj::str(&time_string(time)),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn accessed_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.exists() {
        return obj::err("file not found");
    }

    match std::fs::metadata(path).and_then(|meta| meta.accessed()) {
        Ok(time) => obj::str(&time_string(time)),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn readonly_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.is_file() {
        return obj::err("file not found");
    }

    match std::fs::metadata(path) {
        Ok(meta) => obj::boolean(meta.permissions().readonly()),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn hidden_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let path = Path::new(&path);
    if !path.exists() {
        return obj::err("path not found");
    }

    // the hidden attribute is a Windows concept, the Rust runtime has no portable
    // form of it, so the dot prefix of the name is used (D-052)
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    obj::boolean(name.starts_with('.'))
}

fn join_impl(scope: &Scope) -> Obj {
    let paths = scope::get(scope, "paths");

    let values = match obj::payload(&paths) {
        Payload::Tup(values, _) => values.clone(),
        _ => return obj::err("expected tuple"),
    };

    let mut parts: Vec<String> = Vec::new();
    for value in values.iter() {
        match obj::payload(value) {
            Payload::Str(s) => parts.push(s.to_string()),
            _ => return obj::err("expected path is string"),
        }
    }

    obj::str(&PathBuf::from_iter(parts).display().to_string())
}

fn parent_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let parent = Path::new(&path)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // `Path.GetDirectoryName` uses the separator of the platform
    obj::str(&platform_path(&parent))
}

/// the separators of the platform, the reference's path functions normalize them.
fn platform_path(path: &str) -> String {
    if cfg!(target_os = "windows") {
        path.replace('/', "\\")
    } else {
        path.to_string()
    }
}

fn name_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    obj::str(&name)
}

fn stem_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let stem = Path::new(&path)
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    obj::str(&stem)
}

fn ext_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    // `Path.GetExtension` keeps the dot
    let ext = Path::new(&path)
        .extension()
        .map(|n| format!(".{}", n.to_string_lossy()))
        .unwrap_or_default();

    obj::str(&ext)
}

fn absolute_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    match std::fs::canonicalize(&path) {
        Ok(full) => obj::str(&full.display().to_string()),
        Err(_) => {
            // `Path.GetFullPath` does not need the path to exist
            let full = if Path::new(&path).is_absolute() {
                PathBuf::from(&path)
            } else {
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(&path)
            };
            obj::str(&full.display().to_string())
        }
    }
}

fn relative_impl(scope: &Scope) -> Obj {
    let from = match path_arg(scope, "from") {
        Ok(path) => path,
        Err(e) => return e,
    };
    let to = match path_arg(scope, "to") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let from = std::fs::canonicalize(&from).unwrap_or_else(|_| PathBuf::from(&from));
    let to = std::fs::canonicalize(&to).unwrap_or_else(|_| PathBuf::from(&to));

    match to.strip_prefix(&from) {
        Ok(rest) => obj::str(&rest.display().to_string()),
        Err(_) => obj::str(&to.display().to_string()),
    }
}

fn normalize_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    let full = if Path::new(&path).is_absolute() {
        PathBuf::from(&path)
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(&path)
    };

    // `Path.GetFullPath` resolves the `..` and `.` parts
    let mut parts: Vec<String> = Vec::new();
    for part in full.components() {
        match part {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().to_string()),
        }
    }

    let mut normalized = String::new();
    for part in parts {
        normalized.push_str(&part);
        normalized.push_str(if cfg!(target_os = "windows") { "\\" } else { "/" });
    }

    obj::str(&normalized)
}

fn cwd_impl(_scope: &Scope) -> Obj {
    let cwd = std::env::current_dir().unwrap_or_default();
    obj::str(&cwd.display().to_string())
}

fn chdir_impl(scope: &Scope) -> Obj {
    let path = match path_arg(scope, "path") {
        Ok(path) => path,
        Err(e) => return e,
    };

    if !Path::new(&path).is_dir() {
        return obj::err("directory not found");
    }

    match std::env::set_current_dir(&path) {
        Ok(()) => obj::none(),
        Err(e) => obj::err(&e.to_string()),
    }
}

fn temp_impl(_scope: &Scope) -> Obj {
    let temp = std::env::temp_dir();
    obj::str(&temp.display().to_string())
}

fn home_impl(_scope: &Scope) -> Obj {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    obj::str(&home)
}

fn exe_impl(_scope: &Scope) -> Obj {
    let exe = std::env::current_exe().unwrap_or_default();
    obj::str(&exe.display().to_string())
}

fn app_impl(_scope: &Scope) -> Obj {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().unwrap_or(std::path::Path::new("."));
    obj::str(&dir.display().to_string())
}
