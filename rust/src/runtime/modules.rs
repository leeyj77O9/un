use crate::runtime::collections;
use crate::runtime::iters;
use crate::runtime::obj::{self, Obj};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;
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
        ("template", obj::plain(world::intern_type("template"))),
    ];
    for (name, instance) in instances {
        world::register_original_class(name, Vec::new());
        world::set_class(name, instance);
    }
}
pub fn module_object(members: Vec<(String, Obj)>) -> Obj {
    let module = obj::plain(world::intern_type("module"));
    for (name, value) in members {
        obj::member_set(&module, &name, &value);
    }
    module
}
pub fn include(
    name: &str,
    alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
    target_scope: &Scope,
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
    import_map(&members, name, name, alias, imports, target_scope)
}
pub fn import_map(
    map: &[(String, Obj)],
    module_name: &str,
    module_object_name: &str,
    module_alias: Option<String>,
    imports: Option<Vec<(String, String)>>,
    target_scope: &Scope,
) -> Result<(), Obj> {
    const WILDCARD: &str = "*";
    let scope = target_scope.clone();
    if let Some(imports) = imports {
        if let Some(alias) = module_alias {
            if scope::contains_key(&scope, &alias) {

                let existing = obj::plain(world::intern_type(&alias));
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
                scope::set_local(&scope, &alias, existing);
                return Ok(());
            }
            let module = obj::plain(world::intern_type("module"));
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
            scope::set_local(&scope, &alias, module);
            return Ok(());
        }
        for (name, import_alias) in imports {
            if name == WILDCARD {
                for (k, v) in map {
                    scope::set_local(&scope, k, v.clone());
                }
                continue;
            }
            let value = find_member(map, &name, module_name)?;
            scope::set_local(&scope, &import_alias, value);
        }
        return Ok(());
    }
    let object_name = module_alias.unwrap_or_else(|| module_object_name.to_owned());
    let module = module_object(map.to_vec());
    scope::set_local(&scope, &object_name, module);
    Ok(())
}
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
