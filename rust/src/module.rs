use std::rc::Rc;
use crate::error::{outcome_error, LangError};
use crate::runtime::context;
use crate::runtime::modules;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;
use crate::runner::{compile_source, RunError};
use crate::source::Source;
use crate::vm::{err_header, err_message, Outcome, VM};
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
pub fn import_file(
    parts: &[String],
    source_path: &str,
    module_alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
    trace: Option<context::Traces>,
    target_scope: &Scope,
) -> Result<(), ModuleError> {
    // The VM splits the `Op::Use` name on `.`. An explicit relative import is
    // encoded by the parser as leading empty segments: `use .foo` arrives as
    // ["", "foo"], `use ..pkg` as ["", "", "pkg"]. Absolute paths have no
    // empty prefix and keep the original "source dir -> working dir" lookup.
    let depth = parts.iter().take_while(|part| part.is_empty()).count();
    let name_parts = &parts[depth..];

    let mut joined = std::path::PathBuf::new();
    for part in name_parts {
        joined.push(part);
    }
    let resolved = if depth > 0 {
        resolve_relative_module(&joined, source_path, depth)
    } else {
        resolve_module(&joined, source_path)
    };
    let full = match resolved {
        Some(p) => p,
        None => {
            let searched = if depth > 0 {
                relative_base(source_path, depth).join(&joined)
            } else {
                std::path::Path::new(&world::path()).join(&joined)
            };
            let message = searched.display().to_string();
            return Err(ModuleError::Panic(obj::panic(
                "panic",
                format!("module '{message}' not found"),
            )));
        }
    };
    let map = load(&full, trace)?;
    let module_name = name_parts.join(".");
    let object_name = name_parts.last().cloned().unwrap_or_default();
    modules::import_map(&map, &module_name, &object_name, module_alias, imports, target_scope)
        .map_err(ModuleError::Panic)
}
pub fn load(full_path: &str, trace: Option<context::Traces>) -> Result<Vec<(String, Obj)>, ModuleError> {
    let cache_key = cache_key(full_path);
    if let Some(cached) = world::module_cache_get(&cache_key) {
        return Ok(cached.exports);
    }

    if !world::loading_add(&cache_key) {
        return Err(ModuleError::Panic(obj::panic(
            "panic",
            format!("circular import detected: '{full_path}'"),
        )));
    }
    let result = load_inner(full_path, trace);
    world::loading_remove(&cache_key);
    if let Ok((exports, program_ids)) = &result {
        let mut ids = program_ids.iter().copied();
        world::module_cache_set(
            &cache_key,
            world::CachedModule {
                exports: exports.clone(),
                program_id: ids.next().unwrap_or(0),
                extra_program_ids: ids.collect(),
            },
        );
    }
    result.map(|(exports, _)| exports)
}
/// Returns the exports together with every `Program.id` compiled while loading.
/// Directory modules merge several files, so all of their ids are returned;
/// the cache keeps them so unload can release each program's JIT entries.
fn load_inner(
    full_path: &str,
    trace: Option<context::Traces>,
) -> Result<(Vec<(String, Obj)>, Vec<u64>), ModuleError> {
    let file_path = if full_path.ends_with(".un") {
        full_path.to_owned()
    } else {
        format!("{full_path}.un")
    };
    if std::path::Path::new(&file_path).is_file() {
        return load_file(&file_path, trace).map(|(exports, id)| (exports, vec![id]));
    }
    let dir = std::path::Path::new(full_path);
    if dir.is_dir() {
        let mod_path = dir.join("mod.un");
        if mod_path.is_file() {
            return load_file(&mod_path.display().to_string(), trace)
                .map(|(exports, id)| (exports, vec![id]));
        }
        let mut map: Vec<(String, Obj)> = Vec::new();
        let mut program_ids: Vec<u64> = Vec::new();
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
            let (exports, program_id) = load_file(&file, trace.clone())?;
            program_ids.push(program_id);
            for (name, value) in exports {
                match map.iter_mut().find(|(k, _)| *k == name) {
                    Some(entry) => entry.1 = value,
                    None => map.push((name, value)),
                }
            }
        }
        return Ok((map, program_ids));
    }
    Err(ModuleError::Panic(obj::panic(
        "panic",
        format!("module '{full_path}' not found"),
    )))
}
fn load_file(
    path: &str,
    trace: Option<context::Traces>,
) -> Result<(Vec<(String, Obj)>, u64), ModuleError> {
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

    let module_scope = scope::new(Some(world::global_scope()), false);
    let ctx = context::new(module_scope.clone(), source.clone(), context::new_frames());
    if let Some(trace) = trace {
        ctx.borrow_mut().trace = trace;
    }
    let program = compile_source(&source).map_err(|error| match error {
        RunError::Parse(error) => {
            ModuleError::Error(error.with_source(source.clone()))
        }
        RunError::Panic(panic) => ModuleError::Panic(obj::panic("panic", panic.message)),
    })?;
    if let Err(message) = program.validate() {
        return Err(ModuleError::Panic(obj::panic("panic", message)));
    }
    // Declare every compiled name locally before running the module. Without
    // this a module top-level `len = 999` walks the parent chain and overwrites
    // the global builtin `len`. Uninitialized slots are skipped by lookups, so
    // names the module never assigns still resolve to globals as before.
    scope::preseed_slots(&module_scope, &program.names);
    let program_id = program.id;
    let mut vm = VM::with_context(Rc::new(program), source.clone(), ctx)
        .map_err(|message| ModuleError::Panic(obj::panic("panic", message)))?;
    let outcome = vm.run();
    match outcome {
        Outcome::Value(_) => {}
        Outcome::Error(error) => return Err(ModuleError::Error(error)),
        Outcome::Panic(value) => return Err(ModuleError::Panic(value)),
        Outcome::Propagate(value, start, length) => {
            let error = LangError::new(start, length, err_message(&value)).with_header(err_header(&value));
            return Err(ModuleError::Error(error));
        }
        other => {
            if let Some(error) = outcome_error(&other) {
                return Err(ModuleError::Error(error));
            }
        }
    }
    Ok((scope::local_symbols(&module_scope), program_id))
}
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
/// Base directory for an explicit relative import. `.` (depth 1) resolves in
/// the importing file's own directory; every additional level walks one
/// parent directory (`..` = one parent). Reaching the filesystem root keeps
/// the root as the base instead of panicking on a missing parent.
fn relative_base(source_path: &str, depth: usize) -> std::path::PathBuf {
    let mut base = std::path::Path::new(source_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""))
        .to_path_buf();
    for _ in 1..depth {
        base = base
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .to_path_buf();
    }
    base
}
/// Resolve an explicit relative import strictly against the directory
/// selected by `relative_base`. Unlike absolute imports it does not fall back
/// to the working directory: the leading dots are an explicit request for a
/// specific location.
fn resolve_relative_module(
    joined: &std::path::Path,
    source_path: &str,
    depth: usize,
) -> Option<String> {
    let candidate = relative_base(source_path, depth).join(joined);
    if module_exists(&candidate) {
        Some(candidate.display().to_string())
    } else {
        None
    }
}
fn module_exists(p: &std::path::Path) -> bool {
    let text = p.display().to_string();
    let file = if text.ends_with(".un") {
        std::path::PathBuf::from(&text)
    } else {
        std::path::PathBuf::from(text + ".un")
    };
    file.is_file() || p.is_dir()
}
fn cache_key(full_path: &str) -> String {
    match std::fs::canonicalize(full_path) {
        Ok(path) => path.display().to_string(),
        Err(_) => full_path.to_owned(),
    }
}
