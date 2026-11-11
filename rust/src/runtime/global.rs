//! `Global` — the class registry, the native module registry, and `Global.Include` /
//! `Global.ImportMap`, the `use` statement's runtime binding rules.
//!
//! the class registry is populated by `init_classes`; the native module registry is filled
//! by `Un::native::init`. file-module imports (`Global.Import`) land in phase 11.

use crate::runtime::collections;
use crate::runtime::iters;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope;
use crate::runtime::world;

/// `Global.Builtin` minus the built-in functions. every `[BuiltinType]` class is registered
/// as `Activator.CreateInstance(type)`, the parameterless instance of that type, and every
/// `[NativeType]` contributes its `[Native]` member table to `originalClasses`.
///
/// phase 8 registers the builtin instances; the builtin member tables (`list.add`,
/// `str.to_upper`, ...) land with the native member phase, so `originalClasses` entries
/// start empty.
pub fn init_classes() {
    let instances: Vec<(&str, Obj)> = vec![
        ("int", obj::int(0)),
        ("float", obj::float(0.0)),
        ("bool", obj::boolean(false)),
        ("str", obj::str("")),
        ("date", obj::date_obj(0)),
        ("list", collections::list(vec![])),
        ("tuple", collections::tup(vec![], vec![])),
        ("set", collections::set(vec![])),
        ("dict", collections::dict(vec![])),
        ("iter", iters::seq(vec![])),
    ];

    for (name, instance) in instances {
        world::register_original_class(name, Vec::new());
        world::set_class(name, instance);
    }
}

/// `new Obj(UnType.Module) { Members = new(map) }`
pub fn module_object(members: Vec<(String, Obj)>) -> Obj {
    let module = obj::plain(world::intern_type("module"), world::obj_type());
    for (name, value) in members {
        obj::member_set(&module, &name, &value);
    }
    module
}

/// `Global.Include` — native modules only. file modules are phase 11.
/// `Global.Import` — the `use` form for file modules. the current source's directory is
/// tried first, then `Global.PATH`; a location that exists in neither answers
/// `module '{fullPath}' not found`, and that message is observable in phase 8.
///
/// a module that does exist is loaded by phase 11 (`Runner.Load` of the file, of
/// `mod.un`, or of every `.un` in the directory), so reaching the second arm is the
/// documented divergence point.
pub fn import_file(
    parts: &[String],
    source_path: &str,
    _module_alias: Option<String>,
    _imports: Option<Vec<(String, String)>>,
) -> Result<(), Obj> {
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
            return Err(obj::panic("panic", format!("module '{message}' not found")));
        }
    };

    Err(obj::panic(
        "panic",
        format!("file module '{full}' is not loaded (phase 11)"),
    ))
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

pub fn include(
    name: &str,
    alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
) -> Result<(), Obj> {
    if !world::is_allowed(name) {
        return Err(obj::panic(
            "panic",
            format!("module '{name}' is not allowed in this environment"),
        ));
    }

    let members = match world::native_members(name) {
        Some(m) => m,
        None => {
            return Err(obj::panic(
                "panic",
                format!("native module '{name}' not found"),
            ));
        }
    };

    import_map(&members, name, name, alias, imports)
}

/// `Global.ImportMap` — the four binding shapes of `use`. `imports` is `Some` when the
/// source wrote a parenthesised list, `alias` is the `as` name.
pub fn import_map(
    map: &[(String, Obj)],
    module_name: &str,
    module_object_name: &str,
    module_alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
) -> Result<(), Obj> {
    const WILDCARD: &str = "*";
    let scope = world::global_scope();

    if let Some(imports) = imports {
        if let Some(alias) = module_alias {
            if scope::contains_key(&scope, &alias) {
                // the alias is already a binding: rebuild it as `UnType.Create(alias)`
                // and merge the imported members into it
                let existing = obj::plain(world::intern_type(&alias), world::obj_type());

                for (name, import_alias) in imports {
                    if name == WILDCARD {
                        for (k, v) in map {
                            obj::member_set(&existing, k, &v);
                        }
                        continue;
                    }
                    let value = find_member(map, &name, module_name)?;
                    obj::member_set(&existing, &import_alias, &value);
                }

                scope::set(&scope, &alias, existing);
                return Ok(());
            }

            let module = obj::plain(world::intern_type("module"), world::obj_type());

            for (name, import_alias) in imports {
                if name == WILDCARD {
                    for (k, v) in map {
                        if obj::member(&module, k).is_some() {
                            return Err(obj::panic(
                                "panic",
                                format!("'{k}' already exists in module '{alias}'"),
                            ));
                        }
                        obj::member_set(&module, k, &v);
                    }
                    continue;
                }

                let value = find_member(map, &name, module_name)?;

                if obj::member(&module, &import_alias).is_some() {
                    return Err(obj::panic(
                        "panic",
                        format!("'{import_alias}' already exists in module '{alias}'"),
                    ));
                }
                obj::member_set(&module, &import_alias, &value);
            }

            scope::set(&scope, &alias, module);
            return Ok(());
        }

        for (name, import_alias) in imports {
            if name == WILDCARD {
                for (k, v) in map {
                    scope::set(&scope, k, v.clone());
                }
                continue;
            }

            let value = find_member(map, &name, module_name)?;
            scope::set(&scope, &import_alias, value);
        }

        return Ok(());
    }

    let object_name = module_alias.unwrap_or_else(|| module_object_name.to_owned());
    let module = module_object(map.to_vec());
    scope::set(&scope, &object_name, module);
    Ok(())
}

/// `map.TryGetValue(name, out value)` — a missing member is a `Panic`.
fn find_member(map: &[(String, Obj)], name: &str, module_name: &str) -> Result<Obj, Obj> {
    map.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| {
            obj::panic(
                "panic",
                format!("module '{module_name}' has no member '{name}'"),
            )
        })
}

#[cfg(test)]
mod tests;
