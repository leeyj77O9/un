//! file modules — `Global.Load` and `Global.Import` (`src/Core/Global.cs`).
//!
//! a `use` of a path that is not a native module loads a file module: the file (or
//! the directory's `mod.un`, or every `.un` of the directory) is compiled and run in
//! its own scope, and the scope's own symbols become the module's members.
//!
//! the module cache and the circular import guard are global state, so they live in
//! `world`.

use std::rc::Rc;

use crate::error::LangError;
use crate::runtime::context;
use crate::runtime::global;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope;
use crate::runtime::world;
use crate::runner::{compile_source, RunError};
use crate::source::Source;
use crate::vm::{err_header, err_message, Outcome, VM};

/// the failure of a module load: a host panic (`Panic`) or a language error.
pub enum ModuleError {
    Panic(Obj),
    Error(LangError),
}

impl From<ModuleError> for crate::vm::Outcome {
    fn from(error: ModuleError) -> Self {
        match error {
            ModuleError::Panic(value) => Outcome::Panic(value),
            ModuleError::Error(error) => Outcome::Error(error),
        }
    }
}

/// `Global.Import(path, currentSource, moduleAlias, imports)` — the `use` form for a
/// file module. the current source's directory is tried first, then `Global.PATH`.
pub fn import_file(
    parts: &[String],
    source_path: &str,
    module_alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
) -> Result<(), ModuleError> {
    let mut joined = std::path::PathBuf::new();
    for part in parts {
        joined.push(part);
    }

    let full = match resolve_module(&joined, source_path) {
        Some(p) => p,
        None => {
            // `Global.Load` reports `Path.Combine(PATH, path)`, the PATH-based location,
            // when the relative lookup finds nothing
            let message = std::path::Path::new(&world::path())
                .join(&joined)
                .display()
                .to_string();
            return Err(ModuleError::Panic(obj::panic(
                "panic",
                format!("module '{message}' not found"),
            )));
        }
    };

    let map = load(&full)?;

    let module_name = parts.join(".");
    let object_name = parts.last().cloned().unwrap_or_default();

    global::import_map(&map, &module_name, &object_name, module_alias, imports)
        .map_err(ModuleError::Panic)
}

/// `Global.Load` — the module map of a path, cached by the full path.
pub fn load(full_path: &str) -> Result<Vec<(String, Obj)>, ModuleError> {
    let cache_key = cache_key(full_path);

    if let Some(cached) = world::module_cache_get(&cache_key) {
        return Ok(cached);
    }

    // `_loading.Add(cacheKey)` fails when the path is already being loaded, which is
    // the circular import
    if !world::loading_add(&cache_key) {
        return Err(ModuleError::Panic(obj::panic(
            "panic",
            format!("circular import detected: '{full_path}'"),
        )));
    }

    let result = load_inner(full_path);

    world::loading_remove(&cache_key);

    if let Ok(map) = &result {
        world::module_cache_set(&cache_key, map.clone());
    }

    result
}

/// the file, the `mod.un`, or every `.un` of the directory.
fn load_inner(full_path: &str) -> Result<Vec<(String, Obj)>, ModuleError> {
    let file_path = if full_path.ends_with(".un") {
        full_path.to_owned()
    } else {
        format!("{full_path}.un")
    };

    if std::path::Path::new(&file_path).is_file() {
        return load_file(&file_path);
    }

    let dir = std::path::Path::new(full_path);
    if dir.is_dir() {
        let mod_path = dir.join("mod.un");
        if mod_path.is_file() {
            return load_file(&mod_path.display().to_string());
        }

        let mut map: Vec<(String, Obj)> = Vec::new();
        let mut files: Vec<String> = Vec::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(ModuleError::Panic(obj::panic(
                    "panic",
                    format!("module '{full_path}' not found: {e}"),
                )))
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "un").unwrap_or(false) {
                files.push(path.display().to_string());
            }
        }
        files.sort();

        for file in files {
            for (name, value) in load_file(&file)? {
                match map.iter_mut().find(|(k, _)| *k == name) {
                    Some(entry) => entry.1 = value,
                    None => map.push((name, value)),
                }
            }
        }

        return Ok(map);
    }

    Err(ModuleError::Panic(obj::panic(
        "panic",
        format!("module '{full_path}' not found"),
    )))
}

/// `Global.Load`'s `LoadFile` — the file runs in a scope whose parent is the global
/// scope, and the scope's own symbols become the module's members.
fn load_file(path: &str) -> Result<Vec<(String, Obj)>, ModuleError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            return Err(ModuleError::Panic(obj::panic(
                "panic",
                format!("file '{path}' not found: {e}"),
            )))
        }
    };

    let source = Rc::new(Source::new(path, &text));

    // `Runner.Load(file, inner)` — a fresh scope over the global scope and a fresh
    // frame list
    let module_scope = scope::new(Some(world::global_scope()), false);
    let ctx = context::new(module_scope.clone(), source.clone(), context::new_frames());

    let program = compile_source(&source).map_err(|error| match error {
        RunError::Parse(error) => ModuleError::Error(error),
        RunError::Panic(panic) => ModuleError::Panic(obj::panic("panic", panic.message)),
    })?;

    let mut vm = VM::with_context(Rc::new(program), source.clone(), ctx);
    let outcome = vm.run();

    match outcome {
        Outcome::Value(_) => {}
        Outcome::Error(error) => return Err(ModuleError::Error(error)),
        Outcome::Panic(value) => return Err(ModuleError::Panic(value)),
        Outcome::Propagate(value, start, length) => {
            let error = LangError::new(start, length, err_message(&value)).with_header(err_header(&value));
            return Err(ModuleError::Error(error));
        }
        Outcome::Return(_, start, length) => {
            return Err(ModuleError::Error(LangError::new(
                start,
                length,
                "'->' outside function",
            )))
        }
        Outcome::Break(start, length) => {
            return Err(ModuleError::Error(LangError::new(
                start,
                length,
                "'break' outside loop",
            )))
        }
        Outcome::Skip(start, length) => {
            return Err(ModuleError::Error(LangError::new(
                start,
                length,
                "'skip' outside loop",
            )))
        }
    }

    Ok(scope::local_symbols(&module_scope))
}

/// `Global.Import`'s location rule: `Path.Combine(currentDir, path)` when `<rel>.un`,
/// `<rel>/mod.un`, or the directory itself exists, otherwise `Path.Combine(PATH, path)`.
fn resolve_module(joined: &std::path::Path, source_path: &str) -> Option<String> {
    if let Some(dir) = std::path::Path::new(source_path).parent() {
        let rel = dir.join(joined);
        if module_exists(&rel) {
            return Some(rel.display().to_string());
        }
    }

    let full = std::path::Path::new(&world::path()).join(joined);
    if module_exists(&full) {
        return Some(full.display().to_string());
    }

    None
}

/// `FileSystem.FileExists(fullPath + ".un") || FileSystem.DirectoryExists(fullPath)`.
/// a `fullPath` already ending in `.un` is not extended again.
fn module_exists(p: &std::path::Path) -> bool {
    let text = p.display().to_string();
    let file = if text.ends_with(".un") {
        std::path::PathBuf::from(&text)
    } else {
        std::path::PathBuf::from(text + ".un")
    };

    file.is_file() || p.is_dir()
}

/// `Path.GetFullPath(fullPath)` — the cache key of the module map.
fn cache_key(full_path: &str) -> String {
    match std::fs::canonicalize(full_path) {
        Ok(path) => path.display().to_string(),
        Err(_) => full_path.to_owned(),
    }
}
