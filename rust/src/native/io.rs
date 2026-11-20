use std::io;
use crate::runtime::context::Context;
use crate::runtime::obj::{self, Obj, Payload};
use crate::runtime::scope::{self, Scope};
use crate::runtime::world;
use super::{arg, native};
pub fn members(closure: &Context) -> Vec<(String, Obj)> {
    vec![
        (
            "write".to_owned(),
            native(
                closure,
                "write",
                vec![

                    arg("values", false, false, true),
                    arg("sep", false, true, false),
                    arg("end", false, true, false),
                    arg("stream", false, true, false),
                ],
                write_impl,
            ),
        ),
        (
            "read".to_owned(),
            native(
                closure,
                "read",
                vec![
                    arg("prompt", false, true, false),
                    arg("input", false, true, false),
                    arg("output", false, true, false),
                ],
                read_impl,
            ),
        ),
        (
            "open".to_owned(),
            native(
                closure,
                "open",
                vec![arg("path", true, false, false), arg("mode", false, true, false)],
                open_impl,
            ),
        ),
    ]
}
fn normalize_under(base: &std::path::Path, path: &str) -> Option<String> {
    use std::path::Component;
    let trimmed = path.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return None;    }
    if trimmed.starts_with("\\\\") {
        return None;    }
    let mut out = base.to_path_buf();
    for component in std::path::Path::new(path).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return None;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    if !starts_with_case_insensitive(&out, base) {
        return None;
    }

    // Symlink policy (AUDIT-006): the lexical check above is necessary but not
    // sufficient — a symlink placed under the root may resolve outside it.
    // Resolve symlinks in both the root and the candidate and re-check
    // containment. Return the canonical path so the later open uses exactly
    // the path that passed the check (no check/use gap through a swapped
    // symlink). For a virtual/non-existent root (e.g. the playground's
    // "memory://") canonicalization is impossible, so we keep the lexical
    // result — there is no real filesystem in which a symlink could exist.
    if let Ok(canon_base) = std::fs::canonicalize(base) {
        let resolved = match std::fs::canonicalize(&out) {
            Ok(path) => path,
            Err(_) => {
                // The file may not exist yet (write mode): resolve the nearest
                // existing ancestor and re-append the final component.
                let parent = out.parent()?;
                let name = out.file_name()?;
                std::fs::canonicalize(parent).ok()?.join(name)
            }
        };
        if !starts_with_case_insensitive(&resolved, &canon_base) {
            return None;
        }
        return Some(resolved.display().to_string());
    }
    Some(out.display().to_string())
}
fn starts_with_case_insensitive(path: &std::path::Path, base: &std::path::Path) -> bool {
    let mut path_components = path.components();
    for base_component in base.components() {
        match path_components.next() {
            Some(component) => {
                if !component
                    .as_os_str()
                    .eq_ignore_ascii_case(base_component.as_os_str())
                {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

fn open_impl(scope: &Scope) -> Obj {
    let path = scope::get(scope, "path");
    let mode = scope::get(scope, "mode");
    let path = match obj::payload(&obj::to_str(&path)) {
        Payload::Str(s) => s.to_string(),
        _ => return obj::err("expected 'path' argument to be a string"),
    };
    let mode = if obj::is_none(&mode) {
        "r".to_owned()
    } else {
        match obj::payload(&obj::to_str(&mode)) {
            Payload::Str(s) => s.to_string(),
            _ => return obj::err("expected 'mode' argument to be a string"),
        }
    };
    if !matches!(mode.as_str(), "r" | "w" | "a" | "rw") {
        return obj::err("invalid 'mode' argument, expected one of 'r', 'w', 'a', or 'rw'");
    }

    let full = {
        let base = world::path();
        let base_path = std::path::Path::new(&base);
        if world::sandbox_active() {
            let normalized = normalize_under(base_path, &path);
            match normalized {
                Some(p) => p,
                None => return obj::err("io.open: path escapes the sandbox root"),
            }
        } else {
            base_path.join(&path).display().to_string()
        }
    };
    if mode == "r" && !std::path::Path::new(&full).is_file() {
        return obj::err("file not found");
    }
    let handle = match super::stream::open_file(&full, &mode) {
        Ok(handle) => handle,
        Err(e) => return e,
    };
    super::stream::new_stream(&full, &mode, handle)
}
fn write_impl(scope: &Scope) -> Obj {
    let values = scope::get(scope, "values");
    let sep = scope::get(scope, "sep");
    let end = scope::get(scope, "end");
    let stream = scope::get(scope, "stream");

    if !obj::is_none(&stream) {
        return obj::err("expected 'stream' argument to be of type 'stream'");
    }
    let sep = if obj::is_none(&sep) { obj::str(" ") } else { sep };
    let end = if obj::is_none(&end) { obj::str("\n") } else { end };
    let items = match obj::payload(&obj::to_tuple(&values)) {
        Payload::Tup(v, _) => v,
        _ => return obj::err("expected 'value' argument to be a tuple"),
    };
    let sep = match obj::payload(&obj::to_str(&sep)) {
        Payload::Str(s) => s,
        _ => return obj::err("expected 'sep' argument to be a string"),
    };
    let end = match obj::payload(&obj::to_str(&end)) {
        Payload::Str(s) => s,
        _ => return obj::err("expected 'end' argument to be a string"),
    };
    let count = items.len();
    for (i, item) in items.iter().enumerate() {
        if let Err(e) = write_text(item) {
            return e;
        }
        if i != count - 1 {
            world::stdout().borrow_mut().push_str(&sep);
        }
    }
    world::stdout().borrow_mut().push_str(&end);
    world::flush_stdout();
    obj::none()
}
fn read_impl(scope: &Scope) -> Obj {
    let prompt = scope::get(scope, "prompt");
    let input = scope::get(scope, "input");
    let output = scope::get(scope, "output");

    if !obj::is_none(&input) {
        return obj::err("expected 'input' argument to be of type 'stream'");
    }
    if !obj::is_none(&output) {
        return obj::err("expected 'output' argument to be of type 'stream'");
    }
    let prompt = if obj::is_none(&prompt) { obj::str("") } else { prompt };
    let prompt_text = match obj::payload(&prompt) {
        Payload::Str(s) => s.to_string(),
        _ => String::new(),
    };
    match write_text(&prompt) {
        Ok(()) => {}
        Err(e) => return e,
    }
    world::flush_stdout();
    match world::stdin_next_line() {
        world::StdinLine::Line(line) => return obj::str(&line),
        world::StdinLine::Eof => {
            if world::allow_eof() {

                return obj::str("");
            }

            return obj::panic("need_input", prompt_text);
        }
        world::StdinLine::NotSet => {}
    }
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return obj::none();
    }

    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    obj::str(&line)
}
fn write_text(item: &Obj) -> Result<(), Obj> {
    let text = match obj::payload(&obj::to_str(item)) {
        Payload::Str(s) => s,
        _ => match obj::payload(&obj::repr(item)) {
            Payload::Str(s) => s,
            _ => {
                return Err(obj::err(
                    "expected all values to be strings or representable as strings",
                ))
            }
        },
    };
    world::stdout().borrow_mut().push_str(&text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_under;
    use std::path::Path;

    fn base() -> &'static Path {
        Path::new("C:\\sandbox")
    }

    #[test]
    fn sandbox_rejects_absolute_drive_and_unc_paths() {
        assert!(normalize_under(base(), "C:/secret").is_none());
        assert!(normalize_under(base(), "\\\\server\\share\\secret").is_none());
    }

    #[test]
    fn sandbox_rejects_parent_directory_escapes() {
        assert!(normalize_under(base(), "..//escape").is_none());
        assert!(normalize_under(base(), "sub/../../escape").is_none());
    }

    #[test]
    fn sandbox_accepts_paths_under_the_base() {
        let ok = normalize_under(base(), "sub/file.txt").expect("path under base");
        assert!(ok.to_lowercase().replace('/', "\\").ends_with("sub\\file.txt"));
        let ok = normalize_under(base(), "./file.txt").expect("dot-prefixed path");
        assert!(ok.to_lowercase().replace('/', "\\").ends_with("file.txt"));
    }

    #[test]
    fn sandbox_rejects_symlink_pointing_outside_the_base() {
        let tmp = std::env::temp_dir().join(format!("un_sandbox_symlink_{}", std::process::id()));
        let root = tmp.join("root");
        let outside = tmp.join("outside");
        let _ = std::fs::remove_dir_all(&tmp);
        if std::fs::create_dir_all(&root).is_err() || std::fs::create_dir_all(&outside).is_err() {
            return;
        }
        std::fs::write(outside.join("secret.txt"), b"secret").expect("write outside file");
        let link = root.join("link");
        let created = {
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(&outside, &link)
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&outside, &link)
            }
        };
        if created.is_err() {
            // Creating symlinks needs extra privileges on some Windows setups.
            // Skip the dynamic check in that case; the policy itself is still
            // exercised by the canonicalization path in normalize_under.
            let _ = std::fs::remove_dir_all(&tmp);
            return;
        }
        assert!(
            normalize_under(&root, "link/secret.txt").is_none(),
            "symlink escaping the sandbox root must be rejected"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
