# NATIVE_INVENTORY.md - native standard library inventory

`rust/scripts/rust_inventory.ps1` generates this file by parsing `src/**/*.cs`
(the C# reference, read only). Regenerating refreshes the tables and the string
inventory; this commentary block is kept in the script so it stays in sync.

## 1. registrations

Reference registration uses four attributes and `Global.Init` collects them by assembly
reflection. Rust must register the same names with the same meaning, because the
module cache, circular import rules, module aliases, and the `inspect` module
(`globals`, `members`, `attribute`) all read this table.

- `BuiltinModule` : global builtin functions (not a module)
- `NativeModule`  : modules loaded with `use <name>`
- `BuiltinType`   : methods of the language builtin types
- `NativeType`    : methods of native types

`NativeModule("x", typeof(T))` attaches a type to a module: `flow` gives `pool`/`lock`,
`iter` gives `range`/`counter`/`reverse`/`repeat`, `time` gives `stopwatch`,
`random` gives `random`, `http` gives `http_client`, `ws` gives `web_socket`.

## 2. call conventions

`[ArgInfo]` fields (reproduce verbatim):

| field | meaning |
|---|---|
| `Essential = true` | required argument; the missing-argument path reports `invalid argument: {name}` |
| `Optional = true`  | default `= null!`; the argument arrives as `none` and the native checks it itself |
| `Positional = true`| positional spread argument (`*args`) |
| `Keyword = true`   | keyword argument collection (`**kw`) |
| `Name = "x"`       | can differ from the C# parameter name; `inspect` and the error strings use this name |
| `[Self]`           | injects the receiver, a different path from bound `fn` methods |

Natives take `Obj` and return `Obj`. Failure returns `new Err(msg)`.
`src/Core/Native/` has no `Panic`: `throw` appears only in `Global.cs`, `Runner.cs`,
`Context.cs`, `Optimizer.cs`, `Evaluator.cs`, `UnType.cs` (Rust: `Payload::Panic`).

Sites that lift a host exception into `Err` through `e.Message` (`http`, `os`, `ws`,
`re`, `HttpClient`, `WebSocket`) must reproduce the .NET exception text. `Re` wraps it
as `invalid regex pattern: {e.Message}`.

## 3. function tables

`arg types` is the `ArgumentTypes = new[] { ... }` list and `C# params` is the signature
parameter declarations with their `[ArgInfo]`/`[Self]` attributes. `ret` is the
`ReturnType` string: it is display metadata for `inspect`, not a VM type check.

## 4. typos are verbatim targets

Reference string typos reach stdout/stderr, so copy them exactly:

- `List.cs` L573: `invalid argumnets: index`
- `Dict.cs` L22 / `Iter.cs` L17: `invaild '{Type}' initialize`

## 5. sandbox

`PlaygroundRunner.DefaultAllowed` is the module allow-list for the memory filesystem
sandbox: `io`, `math`, `iter`, `re`, `random`, `time`, `inspect`, `builtin`, `sys`.
The Rust playground runner must make the same allow/deny decision and fail with
`module '{name}' is not allowed in this environment` for a denied module.

| kind | name | file |
|---|---|---|
| `BuiltinModule` | `builtin` | `Builtin.cs` |
| `BuiltinType` | `bool` | `Bool.cs` |
| `BuiltinType` | `date` | `Date.cs` |
| `BuiltinType` | `dict` | `Dict.cs` |
| `BuiltinType` | `float` | `Float.cs` |
| `BuiltinType` | `int` | `Int.cs` |
| `BuiltinType` | `iter` | `Iter.cs` |
| `BuiltinType` | `list` | `List.cs` |
| `BuiltinType` | `set` | `Set.cs` |
| `BuiltinType` | `str` | `Str.cs` |
| `BuiltinType` | `template` | `Template.cs` |
| `BuiltinType` | `tuple` | `Tup.cs` |
| `NativeModule` | `flow` | `Flow.cs` |
| `NativeModule` | `fs` | `Fs.cs` |
| `NativeModule` | `http` | `Http.cs` |
| `NativeModule` | `inspect` | `Inspect.cs` |
| `NativeModule` | `io` | `IO.cs` |
| `NativeModule` | `iter` | `Iter.cs` |
| `NativeModule` | `math` | `Math.cs` |
| `NativeModule` | `os` | `Os.cs` |
| `NativeModule` | `random` | `Random.cs` |
| `NativeModule` | `re` | `Re.cs` |
| `NativeModule` | `sys` | `Sys.cs` |
| `NativeModule` | `time` | `Time.cs` |
| `NativeModule` | `ws` | `Ws.cs` |
| `NativeType` | `counter` | `Counter.cs` |
| `NativeType` | `http_client` | `HttpClient.cs` |
| `NativeType` | `lock` | `Lock.cs` |
| `NativeType` | `pool` | `Pool.cs` |
| `NativeType` | `random` | `Random.cs` |
| `NativeType` | `range` | `Range.cs` |
| `NativeType` | `repeat` | `Repeat.cs` |
| `NativeType` | `reverse` | `Reverse.cs` |
| `NativeType` | `stopwatch` | `Stopwatch.cs` |
| `NativeType` | `stream` | `Stream.cs` |
| `NativeType` | `web_socket` | `WebSocket.cs` |

BuiltinModule x1, BuiltinType x11, NativeModule x13, NativeType x11

### `Builtin.cs` -- BuiltinModule: builtin

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `len` | `int` | `collection` | `Obj value{Essential = true }` | Returns the number of items in a collection. | `write(len(items))` |
| `exit` | `none` | `int` | `Obj code{Optional = true }` | Stops the current UN program with an exit code. | `exit(0)` |
| `type` | `type` | `any` | `Obj value{Essential = true }` | Returns the UN type of a value. | `write(type(value))` |
| `hash` | `int` | `any` | `Obj value{Essential = true }` | Returns a hash value for an object. | `write(hash(value))` |
| `throw` | `error` | `str str` | `Obj message{Essential = true } ; Obj name = null!{Optional = true }` | Raises a named runtime error. | `throw(\"invalid state\")` |
| `throw_if` | `error` | `bool str str` | `Obj condition{Essential = true } ; Obj message{Essential = true } ; Obj name = null!{Optional = true }` | Raises a named runtime error if the condition is true. | `throw_if(condition, \"invalid state\")` |
| `throw_unless` | `error` | `bool str str` | `Obj condition{Essential = true } ; Obj message{Essential = true } ; Obj name = null!{Optional = true }` | Raises a named runtime error if the condition is false. | `throw_unless(condition, \"invalid state\")` |
| `chr` | `str` | `int` | `Obj code{Essential = true }` | Converts an ASCII/Unicode code point to a single-character string. | `chr(65) // \"A\` |
| `ord` | `int` | `str` | `Obj ch{Essential = true }` | Converts a single-character string to its ASCII/Unicode code point. | `ord(\"A\") // 65` |
| `bin` | `str` | `int` | `Obj value{Essential = true }` | Converts an integer to a binary string with '0b' prefix. | `bin(10) // \"0b1010\` |
| `oct` | `str` | `int` | `Obj value{Essential = true }` | Converts an integer to an octal string with '0o' prefix. | `oct(10) // \"0o12\` |
| `hex` | `str` | `int` | `Obj value{Essential = true }` | Converts an integer to a hexadecimal string with '0x' prefix. | `hex(255) // \"0xff\` |
| `repr` | `str` | `any` | `Obj value{Essential = true }` | Convert to repr string. | `repr(io.write)` |
| `ascii` | `int` | `str` | `Obj value{Essential = true }` | Convert to ascii for char | `ascii('a')` |

### `Date.cs` -- BuiltinType: date

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `year` | `int` | `` | `Date self{ SELF}` | Returns the result of date.year(). | `date.year()` |
| `month` | `int` | `` | `Date self{ SELF}` | Returns the result of date.month(). | `date.month()` |
| `day` | `int` | `` | `Date self{ SELF}` | Returns the result of date.day(). | `date.day()` |
| `hour` | `int` | `` | `Date self{ SELF}` | Returns the result of date.hour(). | `date.hour()` |
| `minute` | `int` | `` | `Date self{ SELF}` | Returns the result of date.minute(). | `date.minute()` |
| `second` | `int` | `` | `Date self{ SELF}` | Returns the result of date.second(). | `date.second()` |
| `ms` | `int` | `` | `Date self{ SELF}` | Returns the result of date.ms(). | `date.ms()` |
| `timestamp` | `int` | `` | `Date self{ SELF}` | Returns the result of date.timestamp(). | `date.timestamp()` |
| `timestamp_ms` | `int` | `` | `Date self{ SELF}` | Returns the result of date.timestamp ms(). | `date.timestamp_ms()` |
| `format` | `str` | `str` | `Date self{ SELF} ; Str format{Essential = true }` | Returns the result of date.format(). | `date.format(format)` |
| `add_years` | `date` | `int` | `Date self{ SELF} ; Int years{Essential = true }` | Adds years to a date value. | `date.add_years(years)` |
| `add_months` | `date` | `int` | `Date self{ SELF} ; Int months{Essential = true }` | Adds months to a date value. | `date.add_months(months)` |
| `add_days` | `date` | `int` | `Date self{ SELF} ; Int days{Essential = true }` | Adds days to a date value. | `date.add_days(days)` |
| `add_hours` | `date` | `int` | `Date self{ SELF} ; Int hours{Essential = true }` | Adds hours to a date value. | `date.add_hours(hours)` |
| `add_minutes` | `date` | `int` | `Date self{ SELF} ; Int minutes{Essential = true }` | Adds minutes to a date value. | `date.add_minutes(minutes)` |
| `add_seconds` | `date` | `int` | `Date self{ SELF} ; Int seconds{Essential = true }` | Adds seconds to a date value. | `date.add_seconds(seconds)` |
| `add_ms` | `date` | `int` | `Date self{ SELF} ; Int milliseconds{Essential = true }` | Adds ms to a date value. | `date.add_ms(milliseconds)` |
| `weekday` | `int` | `` | `Date self{ SELF}` | Returns the result of date.weekday(). | `date.weekday()` |
| `day_of_year` | `int` | `` | `Date self{ SELF}` | Returns the result of date.day of year(). | `date.day_of_year()` |
| `days_in_month` | `int` | `` | `Date self{ SELF}` | Returns the result of date.days in month(). | `date.days_in_month()` |
| `is_leap_year` | `bool` | `` | `Date self{ SELF}` | Checks whether a date value leap year. | `date.is_leap_year()` |
| `date` | `date` | `` | `Date self{ SELF}` | Returns the result of date.date(). | `date.date()` |
| `before` | `bool` | `date` | `Date self{ SELF} ; Date other{Essential = true }` | Returns the result of date.before(). | `date.before(other)` |
| `after` | `bool` | `date` | `Date self{ SELF} ; Date other{Essential = true }` | Returns the result of date.after(). | `date.after(other)` |
| `compare` | `int` | `date` | `Date self{ SELF} ; Date other{Essential = true }` | Returns the result of date.compare(). | `date.compare(other)` |
| `to_utc` | `date` | `` | `Date self{ SELF}` | Converts a date value to utc. | `date.to_utc()` |
| `to_local` | `date` | `` | `Date self{ SELF}` | Converts a date value to local. | `date.to_local()` |

### `Dict.cs` -- BuiltinType: dict

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `add` | `none` | `any any` | `Dict self{ SELF} ; Obj key{Essential = true } ; Obj value{Essential = true }` | Adds a value to data. | `data.add(key, value)` |
| `remove` | `any` | `any` | `Dict self{ SELF} ; Obj key{Essential = true }` | Removes a value from a dict value. | `data.remove(key)` |
| `get` | `any` | `any any` | `Dict self{ SELF} ; Obj key{Essential = true } ; Obj defaultValue = null!{Optional = true, Name = "default_value" }` | Gets a value from data. | `data.get(key, defaultValue)` |
| `contains_key` | `bool` | `any` | `Dict self{ SELF} ; Obj key{Essential = true }` | Returns the result of data.contains key(). | `data.contains_key(key)` |
| `contains_value` | `bool` | `any` | `Dict self{ SELF} ; Obj value{Essential = true }` | Returns the result of data.contains value(). | `data.contains_value(value)` |
| `clear` | `none` | `` | `Dict self{ SELF}` | Removes all values from data. | `data.clear()` |
| `keys` | `list` | `` | `Dict self{ SELF}` | Returns the result of data.keys(). | `data.keys()` |
| `values` | `list` | `` | `Dict self{ SELF}` | Returns the result of data.values(). | `data.values()` |

### `Flow.cs` -- NativeModule: flow

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `spawn` | `pool` | `int` | `Obj workerCount = null!{Optional = true, Name = "worker_count" }` | Creates a worker pool with an optional worker count. | `workers = spawn(4)` |
| `lock` | `lock` | `` | `` | Creates a lock for coordinating concurrent work. | `guard = lock()` |

### `Fs.cs` -- NativeModule: fs

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `exists` | `bool` | `str` | `Obj path{Essential = true }` | Checks whether a file or directory exists. | `write(exists(\"notes.txt\"))` |
| `file` | `bool` | `str` | `Obj path{Essential = true }` | Checks whether a path is an existing file. | `write(file(\"notes.txt\"))` |
| `dir` | `bool` | `str` | `Obj path{Essential = true }` | Checks whether a path is an existing directory. | `write(dir(\"data\"))` |
| `read` | `str` | `str` | `Obj path{Essential = true }` | Reads all text from a file. | `text = read(\"notes.txt\")` |
| `write` | `none` | `str str` | `Obj path{Essential = true } ; Obj text{Essential = true }` | Writes text to a file, replacing existing content. | `write(\"notes.txt\", \"Hello\")` |
| `append` | `none` | `str str` | `Obj path{Essential = true } ; Obj text{Essential = true }` | Appends text to a file. | `append(\"notes.txt\", \"\\nNext line\")` |
| `delete` | `none` | `str` | `Obj path{Essential = true }` | Deletes a file or directory. | `delete(\"old.txt\")` |
| `copy` | `none` | `str str` | `Obj source{Essential = true } ; Obj destination{Essential = true }` | Copies a file to a destination path. | `copy(\"from.txt\", \"to.txt\")` |
| `move` | `none` | `str str` | `Obj source{Essential = true } ; Obj destination{Essential = true }` | Moves a file to a destination path. | `move(\"draft.txt\", \"archive/draft.txt\")` |
| `rename` | `none` | `str str` | `Obj path{Essential = true } ; Obj name{Essential = true }` | Renames a file within its current directory. | `rename(\"draft.txt\", \"final.txt\")` |
| `touch` | `none` | `str` | `Obj path{Essential = true }` | Creates a file when it does not exist. | `touch(\"notes.txt\")` |
| `mkdir` | `none` | `str` | `Obj path{Essential = true }` | Creates a directory and missing parent directories. | `mkdir(\"data/output\")` |
| `rmdir` | `none` | `str bool` | `Obj path{Essential = true } ; Obj recursive = null!{Optional = true }` | Deletes a directory, optionally including its contents. | `rmdir(\"cache\", true)` |
| `list` | `list` | `str` | `Obj path{Essential = true }` | Lists file-system entries in a directory. | `items = list(\"data\")` |
| `files` | `list` | `str` | `Obj path{Essential = true }` | Lists files in a directory. | `items = files(\"data\")` |
| `dirs` | `list` | `str` | `Obj path{Essential = true }` | Lists subdirectories in a directory. | `items = dirs(\"data\")` |
| `walk` | `list` | `str` | `Obj path{Essential = true }` | Lists file-system entries recursively. | `items = walk(\"data\")` |
| `size` | `int` | `str` | `Obj path{Essential = true }` | Returns the byte size of a file. | `write(size(\"notes.txt\"))` |
| `created` | `str` | `str` | `Obj path{Essential = true }` | Returns a path creation time in UTC. | `write(created(\"notes.txt\"))` |
| `modified` | `str` | `str` | `Obj path{Essential = true }` | Returns a path last-modified time in UTC. | `write(modified(\"notes.txt\"))` |
| `accessed` | `str` | `str` | `Obj path{Essential = true }` | Returns a path last-accessed time in UTC. | `write(accessed(\"notes.txt\"))` |
| `readonly` | `bool` | `str` | `Obj path{Essential = true }` | Checks whether a file is read-only. | `write(readonly(\"notes.txt\"))` |
| `hidden` | `bool` | `str` | `Obj path{Essential = true }` | Checks whether a path has the hidden attribute. | `write(hidden(\"notes.txt\"))` |
| `join` | `str` | `tuple` | `Obj paths{Positional = true }` | Joins path segments using the platform separator. | `path = join(\"data\", \"notes.txt\")` |
| `parent` | `str` | `str` | `Obj path{Essential = true }` | Returns the parent directory of a path. | `write(parent(\"data/notes.txt\"))` |
| `name` | `str` | `str` | `Obj path{Essential = true }` | Returns the file name portion of a path. | `write(name(\"data/notes.txt\"))` |
| `stem` | `str` | `str` | `Obj path{Essential = true }` | Returns a file name without its extension. | `write(stem(\"notes.txt\"))` |
| `ext` | `str` | `str` | `Obj path{Essential = true }` | Returns the extension portion of a path. | `write(ext(\"notes.txt\"))` |
| `absolute` | `str` | `str` | `Obj path{Essential = true }` | Returns the absolute form of a path. | `write(absolute(\"notes.txt\"))` |
| `relative` | `str` | `str str` | `Obj from{Essential = true } ; Obj to{Essential = true }` | Returns a path relative to another directory. | `write(relative(\"data\", \"data/notes.txt\"))` |
| `normalize` | `str` | `str` | `Obj path{Essential = true }` | Normalizes a path to its absolute form. | `write(normalize(\"data/../notes.txt\"))` |
| `cwd` | `str` | `` | `` | Returns the current working directory. | `write(cwd())` |
| `chdir` | `none` | `str` | `Obj path{Essential = true }` | Changes the current working directory. | `chdir(\"data\")` |
| `temp` | `str` | `` | `` | Returns the system temporary directory. | `write(temp())` |
| `home` | `str` | `` | `` | Returns the current user's home directory. | `write(home())` |
| `exe` | `str` | `` | `` | Returns the current process executable path. | `write(exe())` |
| `app` | `str` | `` | `` | Returns the application base directory. | `write(app())` |

### `Http.cs` -- NativeModule: http

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `connect` | `http_client` | `` | `` | Creates an HTTP client object. | `client = connect()` |

### `HttpClient.cs` -- NativeType: http_client

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `set_header` | `none` | `any any` | `HttpClient self{ SELF} ; Obj key{Essential = true } ; Obj value{Essential = true }` | Sets header on a http_client value. | `client.set_header(key, value)` |
| `clear_headers` | `none` | `` | `HttpClient self{ SELF}` | Returns the result of client.clear headers(). | `client.clear_headers()` |
| `get` | `str` | `any` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Gets a value from client. | `client.get(url)` |
| `get_async` | `str` | `str` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Asynchronously sends a GET request and returns its response text. | `body = client.get_async(\"https://example.com\")` |
| `delete` | `str` | `any` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Deletes a value from client. | `client.delete(url)` |
| `delete_async` | `str` | `str` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Asynchronously sends a DELETE request and returns its response text. | `body = client.delete_async(\"https://example.com/item\")` |
| `post` | `str` | `any any` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Returns the result of client.post(). | `client.post(url, body)` |
| `post_async` | `str` | `str str` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Asynchronously sends a POST request with text content. | `body = client.post_async(\"https://example.com\", \"data\")` |
| `put` | `str` | `any any` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Returns the result of client.put(). | `client.put(url, body)` |
| `put_async` | `str` | `str str` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Asynchronously sends a PUT request with text content. | `body = client.put_async(\"https://example.com\", \"data\")` |
| `patch` | `str` | `any any` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Returns the result of client.patch(). | `client.patch(url, body)` |
| `patch_async` | `str` | `str str` | `HttpClient self{ SELF} ; Obj url{Essential = true } ; Obj body{Essential = true }` | Asynchronously sends a PATCH request with text content. | `body = client.patch_async(\"https://example.com\", \"data\")` |
| `head` | `int` | `any` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Returns the result of client.head(). | `client.head(url)` |
| `head_async` | `int` | `str` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Asynchronously sends a HEAD request and returns its status code. | `status = client.head_async(\"https://example.com\")` |
| `options` | `list` | `any` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Returns the result of client.options(). | `client.options(url)` |
| `options_async` | `list` | `str` | `HttpClient self{ SELF} ; Obj url{Essential = true }` | Asynchronously lists HTTP methods allowed by an endpoint. | `methods = client.options_async(\"https://example.com\")` |

### `Inspect.cs` -- NativeModule: inspect

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `attr` | `list` | `any` | `Obj value{Essential = true }` | Lists available attributes for a value. | `write(attr(value))` |
| `global` | `dict` | `` | `` | Returns the global namespace object. | `write(global())` |
| `getattr` | `any` | `any str any` | `Obj value{Essential = true } ; Obj name{Essential = true } ; Obj defaultValue = null!{Optional = true, Name = "default_value" }` | Reads an attribute with an optional default. | `name = getattr(user, \"name\", \"guest\")` |
| `hasattr` | `bool` | `any str` | `Obj value{Essential = true } ; Obj name{Essential = true }` | Checks whether an object has an attribute. | `write(hasattr(user, \"name\"))` |
| `setattr` | `any` | `object str any` | `Obj value{Essential = true } ; Obj name{Essential = true } ; Obj newValue{Essential = true, Name = "new_value" }` | Sets an attribute on an object. | `setattr(user, \"name\", \"Ada\")` |
| `delattr` | `none` | `object str` | `Obj value{Essential = true } ; Obj name{Essential = true }` | Deletes an attribute from an object. | `delattr(user, \"name\")` |
| `meta` | `any` | `object str` | `Obj value{Essential = true } ; Obj name{Essential = true }` | Reads metadata from an object. | `write(meta(value, \"type\"))` |

### `IO.cs` -- NativeModule: io

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `write` | `none` | `tuple str str stream` | `Obj values{Positional = true } ; Obj sep = null!{Optional = true } ; Obj end = null!{Optional = true } ; Obj stream = null!{Optional = true }` | Writes values to standard output. | `write(\"Hello\", \"UN\")` |
| `read` | `str` | `str stream stream` | `Obj prompt = null!{Optional = true } ; Obj input = null!{Optional = true } ; Obj output = null!{Optional = true }` | Reads one line from an input stream. | `name = read(\"Name: \")` |
| `open` | `stream` | `str str` | `Obj path{Essential = true } ; Obj mode = null!{Optional = true }` | Opens a sandbox-relative file stream. | `stream = open(\"notes.txt\", \"r\")` |
| `clear` | `none` | `` | `` | Clears the active output stream. | `clear()` |

### `Iter.cs` -- NativeModule: iter, BuiltinType: iter

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `array` | `list` | `any int` | `Obj value{Essential = true } ; Obj size = null!{Positional = true }` | Creates an array from a value and dimensions. | `items = array(1, 3)` |
| `range` | `iter` | `int int int` | `Obj start{Essential = true } ; Obj end = null!{Optional = true } ; Obj step = null!{Optional = true }` | Creates a sequence of integers. | `for index in range(0, 3)\n    write(index)` |
| `counter` | `counter` | `int` | `Obj start = null!{Optional = true }` | Creates an unbounded integer counter. | `numbers = counter(0)` |
| `reverse` | `reverse` | `iter` | `Obj obj{Essential = true }` | Iterates over values in reverse order. | `items = reverse(values)` |
| `repeat` | `repeat` | `iter int` | `Obj obj{Essential = true } ; Obj count = null!{Optional = true }` | Repeats an iterable a fixed number of times. | `items = repeat(values, 3)` |
| `zip` | `list` | `iter` | `Obj iterables{Positional = true }` | Combines corresponding values from iterables. | `pairs = zip(first, second)` |
| `enumerate` | `iter` | `iter` | `Obj iterable{Essential = true }` | Pairs iterable values with their indexes. | `for pair in enumerate(items)\n    write(pair)` |
| `sum` | `any` | `iter` | `Obj iterable{Positional = true }` | Adds all values in an iterable. | `write(sum(values))` |
| `max` | `any` | `iter` | `Obj iterable{Positional = true }` | Returns the greatest value in an iterable. | `write(max(values))` |
| `min` | `any` | `iter` | `Obj iterable{Positional = true }` | Returns the smallest value in an iterable. | `write(min(values))` |
| `filter` | `list` | `func iter` | `Obj predicate{Essential = true } ; Obj iterable{Essential = true }` | Keeps iterable values for which a predicate returns true. | `positives = filter(is_positive, values)` |
| `map` | `list` | `func iter` | `Obj transform{Essential = true } ; Obj iterable{Essential = true }` | Transforms every value in an iterable. | `doubled = map(double, values)` |
| `take` | `list` | `int iter` | `Obj count{Essential = true } ; Obj iterable{Essential = true }` | Takes a number of values from an iterable. | `first = take(3, values)` |
| `skip` | `list` | `int iter` | `Obj count{Essential = true } ; Obj iterable{Essential = true }` | Skips a number of values in an iterable. | `rest = skip(2, values)` |
| `chain` | `list` | `iter` | `Obj iterables{Positional = true }` | Concatenates multiple iterables into one sequence. | `items = chain(first, second)` |
| `flatten` | `list` | `iter` | `Obj iterable{Essential = true }` | Flattens nested iterable values. | `flat = flatten(groups)` |
| `all` | `bool` | `func iter` | `Obj predicate{Essential = true } ; Obj iterable{Essential = true }` | Checks whether every iterable item matches a predicate. | `all(is_valid, values)` |
| `any` | `bool` | `func iter` | `Obj predicate{Essential = true } ; Obj iterable{Essential = true }` | Checks whether at least one iterable item matches a predicate. | `any(is_ready, tasks)` |
| `count` | `int` | `func iter` | `Obj predicate{Essential = true } ; Obj iterable{Essential = true }` | Counts iterable items that match a predicate. | `write(count(is_even, values))` |
| `sorted` | `list` | `iter` | `Obj iterable{Essential = true }` | Returns iterable values in sorted order. | `ordered = sorted(values)` |
| `take` | `iter` | `any` | `Iters self{ SELF} ; Obj n{Essential = true }` | Returns the result of iterator.take(). | `iterator.take(n)` |
| `skip` | `iter` | `any` | `Iters self{ SELF} ; Obj n{Essential = true }` | Returns the result of iterator.skip(). | `iterator.skip(n)` |
| `count` | `int` | `` | `Iters self{ SELF}` | Returns the result of iterator.count(). | `iterator.count()` |
| `to_list` | `list` | `` | `Iters self{ SELF}` | Converts a iter value to list. | `iterator.to_list()` |
| `to_tuple` | `tuple` | `` | `Iters self{ SELF}` | Converts a iter value to tuple. | `iterator.to_tuple()` |
| `map` | `iter` | `any` | `Iters self{ SELF} ; Obj fn{Essential = true }` | Returns the result of iterator.map(). | `iterator.map(fn)` |
| `filter` | `iter` | `any` | `Iters self{ SELF} ; Obj fn{Essential = true }` | Returns the result of iterator.filter(). | `iterator.filter(fn)` |
| `any` | `bool` | `any` | `Iters self{ SELF} ; Obj fn{Essential = true }` | Returns the result of iterator.any(). | `iterator.any(fn)` |
| `all` | `bool` | `any` | `Iters self{ SELF} ; Obj fn{Essential = true }` | Returns the result of iterator.all(). | `iterator.all(fn)` |
| `sum` | `any` | `` | `Iters self{ SELF}` | Returns the result of iterator.sum(). | `iterator.sum()` |
| `first` | `any` | `` | `Iters self{ SELF}` | Returns the result of iterator.first(). | `iterator.first()` |
| `last` | `any` | `` | `Iters self{ SELF}` | Returns the result of iterator.last(). | `iterator.last()` |

### `List.cs` -- BuiltinType: list

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `add` | `none` | `any` | `List self{ SELF} ; Obj value{Essential = true }` | Adds a value to items. | `items.add(value)` |
| `extend` | `none` | `any` | `List self{ SELF} ; Obj value{Essential = true }` | Adds values to items. | `items.extend(value)` |
| `insert` | `none` | `any any` | `List self{ SELF} ; Obj obj{Essential = true } ; Obj index{Essential = true }` | Inserts a value into items. | `items.insert(obj, index)` |
| `extend_insert` | `list` | `any any` | `List self{ SELF} ; Obj obj{Essential = true } ; Obj index{Essential = true }` | Returns the result of items.extend insert(). | `items.extend_insert(obj, index)` |
| `remove` | `bool` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Removes a value from a list value. | `items.remove(obj)` |
| `remove_at` | `any` | `any` | `List self{ SELF} ; Obj index{Essential = true }` | Removes  at from a list value. | `items.remove_at(index)` |
| `index_of` | `int` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Finds a value index in items. | `items.index_of(obj)` |
| `contains` | `bool` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Checks whether items contains a value. | `items.contains(obj)` |
| `order` | `list` | `any` | `List self{ SELF} ; Obj fn{Essential = true }` | Returns the result of items.order(). | `items.order(fn)` |
| `sort` | `none` | `` | `List self{ SELF}` | Sorts items in place. | `items.sort()` |
| `reverse` | `none` | `` | `List self{ SELF}` | Reverses the order of items. | `items.reverse()` |
| `binary_search` | `int` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Returns the result of items.binary search(). | `items.binary_search(obj)` |
| `lower_bound` | `int` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Returns the result of items.lower bound(). | `items.lower_bound(obj)` |
| `upper_bound` | `int` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Returns the result of items.upper bound(). | `items.upper_bound(obj)` |
| `hpush` | `none` | `any` | `List self{ SELF} ; Obj obj{Essential = true }` | Returns the result of items.hpush(). | `items.hpush(obj)` |
| `hpop` | `any` | `` | `List self{ SELF}` | Returns the result of items.hpop(). | `items.hpop()` |
| `pop` | `any` | `any` | `List self{ SELF} ; Obj? index = null{Optional = true }` | Returns the result of items.pop(). | `items.pop(index)` |
| `map` | `list` | `any` | `List self{ SELF} ; Obj type{Essential = true }` | Returns the result of items.map(). | `items.map(type)` |
| `resize` | `none` | `` | `List self{ SELF}` | Returns the result of items.resize(). | `items.resize()` |

### `Lock.cs` -- NativeType: lock

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `acquire` | `none` | `` | `Lock self{ SELF}` | Returns the result of guard.acquire(). | `guard.acquire()` |
| `try_acquire` | `bool` | `` | `Lock self{ SELF}` | Returns the result of guard.try acquire(). | `guard.try_acquire()` |
| `release` | `none` | `` | `Lock self{ SELF}` | Returns the result of guard.release(). | `guard.release()` |
| `dispose` | `none` | `` | `Lock self{ SELF}` | Returns the result of guard.dispose(). | `guard.dispose()` |

### `Math.cs` -- NativeModule: math

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `round` | `int \| float` | `int \| float int` | `Obj value{Essential = true } ; Obj digit = null!{Optional = true }` | Rounds a number to a number of digits. | `write(round(3.14159, 2))` |
| `abs` | `int \| float` | `int \| float` | `Obj value{Essential = true }` | Returns the absolute value of a number. | `write(abs(-5))` |
| `ceil` | `int` | `int \| float` | `Obj value{Essential = true }` | Rounds a number upward. | `write(ceil(2.1))` |
| `floor` | `int` | `int \| float` | `Obj value{Essential = true }` | Rounds a number downward. | `write(floor(2.9))` |
| `trunc` | `int` | `int \| float` | `Obj value{Essential = true }` | Removes the fractional part of a number. | `write(trunc(2.9))` |
| `sign` | `int` | `int \| float` | `Obj value{Essential = true }` | Returns the sign of a number. | `write(sign(-3))` |
| `sqrt` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the square root of a number. | `write(sqrt(9))` |
| `cbrt` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the cube root of a number. | `write(cbrt(27))` |
| `pow` | `float` | `int \| float int \| float` | `Obj value{Essential = true } ; Obj exponent{Essential = true }` | Raises a number to an exponent. | `write(pow(2, 8))` |
| `exp` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns e raised to a power. | `write(exp(2))` |
| `log` | `float` | `int \| float int \| float` | `Obj value{Essential = true } ; Obj @base = null!{Optional = true }` | Returns a logarithm with an optional base. | `write(log(8, 2))` |
| `log2` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the base-2 logarithm. | `write(log2(8))` |
| `log10` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the base-10 logarithm. | `write(log10(100))` |
| `sin` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the sine of an angle. | `write(sin(0))` |
| `cos` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the cosine of an angle. | `write(cos(0))` |
| `tan` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the tangent of an angle. | `write(tan(0))` |
| `asin` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the arcsine of a number. | `write(asin(0))` |
| `acos` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the arccosine of a number. | `write(acos(0))` |
| `atan` | `float` | `int \| float` | `Obj value{Essential = true }` | Returns the arctangent of a number. | `write(atan(1))` |
| `atan2` | `float` | `int \| float int \| float` | `Obj y{Essential = true } ; Obj x{Essential = true }` | Returns the angle for y and x coordinates. | `write(atan2(y, x))` |
| `hypot` | `float` | `int \| float int \| float` | `Obj x{Essential = true } ; Obj y{Essential = true }` | Returns the hypotenuse for two coordinates. | `write(hypot(3, 4))` |
| `clamp` | `int \| float` | `int \| float int \| float int \| float` | `Obj value{Essential = true } ; Obj min{Essential = true } ; Obj max{Essential = true }` | Limits a number to a minimum and maximum. | `write(clamp(score, 0, 100))` |
| `gcd` | `int` | `int int` | `Obj a{Essential = true } ; Obj b{Essential = true }` | Returns the greatest common divisor. | `write(gcd(12, 18))` |
| `lcm` | `int` | `int int` | `Obj a{Essential = true } ; Obj b{Essential = true }` | Returns the least common multiple. | `write(lcm(4, 6))` |
| `is_nan` | `bool` | `int \| float` | `Obj value{Essential = true }` | Checks whether a number is not-a-number. | `write(is_nan(value))` |
| `is_infinite` | `bool` | `int \| float` | `Obj value{Essential = true }` | Checks whether a number is infinite. | `write(is_infinite(value))` |
| `is_finite` | `bool` | `int \| float` | `Obj value{Essential = true }` | Checks whether a number is finite. | `write(is_finite(value))` |
| `pi` | `float` | `` | `` | Returns the mathematical constant pi. | `write(pi())` |
| `e` | `float` | `` | `` | Returns Euler's number. | `write(e())` |
| `tau` | `float` | `` | `` | Returns the mathematical constant tau. | `write(tau())` |

### `Os.cs` -- NativeModule: os

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `env` | `str` | `str` | `Obj name{Essential = true }` | Reads an environment variable by name. | `write(env(\"HOME\"))` |
| `setenv` | `none` | `str str` | `Obj name{Essential = true } ; Obj value{Essential = true }` | Sets an environment variable. | `setenv(\"MODE\", \"test\")` |
| `unsetenv` | `none` | `str` | `Obj name{Essential = true }` | Removes an environment variable. | `unsetenv(\"MODE\")` |
| `environ` | `list` | `` | `` | Returns all environment variables as text entries. | `values = environ()` |
| `expand_vars` | `str` | `str` | `Obj text{Essential = true }` | Expands environment-variable placeholders in text. | `path = expand_vars(\"$HOME/data\")` |
| `pid` | `int` | `` | `` | Returns the current process identifier. | `write(pid())` |
| `name` | `str` | `` | `` | Returns the current operating-system name. | `write(name())` |
| `arch` | `str` | `` | `` | Returns the operating-system architecture. | `write(arch())` |
| `version` | `str` | `` | `` | Returns the operating-system version. | `write(version())` |
| `cpuCount` | `int` | `` | `` | Returns the available processor count. | `write(cpuCount())` |
| `is64bit` | `bool` | `` | `` | Checks whether the operating system is 64-bit. | `write(is64bit())` |
| `hostname` | `str` | `` | `` | Returns the current machine host name. | `write(hostname())` |
| `username` | `str` | `` | `` | Returns the current operating-system user name. | `write(username())` |
| `args` | `list` | `` | `` | Returns the process command-line arguments. | `values = args()` |
| `sep` | `str` | `` | `` | Returns the directory separator character. | `write(sep())` |
| `pathsep` | `str` | `` | `` | Returns the path-list separator character. | `write(pathsep())` |
| `linesep` | `str` | `` | `` | Returns the environment line separator. | `write(linesep())` |
| `tickCount` | `int` | `` | `` | Returns elapsed system ticks in milliseconds. | `write(tickCount())` |
| `exit` | `list` | `int` | `Obj code = null!{Optional = true }` | Stops the current process with an optional exit code. | `exit(0)` |
| `exec` | `str` | `str` | `Obj command{Essential = true }` | Runs a shell command and returns its standard output. | `output = exec(\"echo hello\")` |

### `Pool.cs` -- NativeType: pool

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `submit` | `any` | `any any` | `Pool self{ SELF} ; Obj fn{Essential = true } ; Obj args{Positional = true }` | Returns the result of workers.submit(). | `workers.submit(fn, args)` |
| `map` | `list` | `any any` | `Pool self{ SELF} ; Obj fn{Essential = true } ; Obj vargs{Positional = true }` | Returns the result of workers.map(). | `workers.map(fn, vargs)` |
| `close` | `none` | `` | `Pool self{ SELF}` | Closes workers. | `workers.close()` |

### `Random.cs` -- NativeModule: random, NativeType: random

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `random` | `random` | `` | `` | Creates a random-number generator object. | `generator = random()` |
| `next` | `int` | `` | `` | Returns a non-negative pseudo-random integer. | `write(next())` |
| `int` | `random` | `int int` | `Obj min{Essential = true } ; Obj max{Essential = true }` | Returns a random integer in an inclusive range. | `value = int(1, 6)` |
| `float` | `float` | `float float` | `Obj min = null!{Optional = true } ; Obj max = null!{Optional = true }` | Returns a random floating-point value, optionally within a range. | `value = float(0.0, 1.0)` |
| `bool` | `bool` | `` | `` | Returns a pseudo-random boolean value. | `write(bool())` |
| `bytes` | `list` | `int` | `Obj length{Essential = true }` | Returns a list of pseudo-random byte values. | `values = bytes(16)` |
| `shuffle` | `none` | `list` | `Obj obj{Essential = true }` | Shuffles a list in place. | `shuffle(items)` |
| `choice` | `any` | `list` | `Obj obj{Essential = true }` | Returns a random value from a list. | `item = choice(items)` |
| `choices` | `list` | `list int` | `Obj obj{Essential = true } ; Obj count{Essential = true }` | Returns multiple random values from a list with replacement. | `items = choices(values, 3)` |
| `sample` | `list` | `list int` | `Obj obj{Essential = true } ; Obj count{Essential = true }` | Returns unique random values from a list. | `items = sample(values, 3)` |
| `chance` | `bool` | `float` | `Obj probability{Essential = true }` | Returns true according to a probability from zero to one. | `if chance(0.25)\n    write(\"selected\")` |
| `seed` | `none` | `int` | `Obj seed{Essential = true }` | Sets the shared pseudo-random generator seed. | `seed(42)` |
| `uuid` | `str` | `` | `` | Returns a new UUID string. | `id = uuid()` |
| `str` | `str` | `int` | `Obj length{Essential = true }` | Returns an alphanumeric pseudo-random string. | `token = string(12)` |
| `weighted` | `any` | `list list` | `Obj values{Essential = true } ; Obj weights{Essential = true }` | Returns a list value selected by matching numeric weights. | `item = weighted(values, weights)` |
| `range` | `list` | `int int` | `Obj start{Essential = true } ; Obj end = null!{Optional = true }` | Returns a shuffled list of integers in a range. | `values = range(1, 10)` |
| `value` | `enum` | `enum` | `Obj obj{Essential = true }` | Returns a random value from an enum instance. | `status = value(Status)` |
| `next` | `int` | `` | `Random self{ SELF}` | Returns a non-negative pseudo-random integer. | `write(generator.next())` |
| `int` | `int` | `int int` | `Random self{ SELF} ; Obj min{Essential = true } ; Obj max{Essential = true }` | Returns a random integer in an inclusive range. | `value = generator.int(1, 6)` |
| `float` | `float` | `float float` | `Random self{ SELF} ; Obj min = null!{Optional = true } ; Obj max = null!{Optional = true }` | Returns a random floating-point value, optionally within a range. | `value = generator.float(0.0, 1.0)` |
| `bool` | `bool` | `` | `Random self{ SELF}` | Returns a pseudo-random boolean value. | `write(generator.bool())` |
| `bytes` | `list` | `int` | `Random self{ SELF} ; Obj length{Essential = true }` | Returns a list of pseudo-random byte values. | `values = generator.bytes(16)` |
| `shuffle` | `none` | `list` | `Random self{ SELF} ; Obj obj{Essential = true }` | Shuffles a list in place. | `generator.shuffle(items)` |
| `choice` | `any` | `list` | `Random self{ SELF} ; Obj obj{Essential = true }` | Returns a random value from a list. | `item = generator.choice(items)` |
| `choices` | `list` | `list int` | `Random self{ SELF} ; Obj obj{Essential = true } ; Obj count{Essential = true }` | Returns multiple random values from a list with replacement. | `items = generator.choices(values, 3)` |
| `sample` | `list` | `list int` | `Random self{ SELF} ; Obj obj{Essential = true } ; Obj count{Essential = true }` | Returns unique random values from a list. | `items = generator.sample(values, 3)` |
| `chance` | `bool` | `float` | `Random self{ SELF} ; Obj probability{Essential = true }` | Returns true according to a probability from zero to one. | `generator.chance(0.25)` |
| `seed` | `none` | `int` | `Random self{ SELF} ; Obj seed{Essential = true }` | Sets this generator's pseudo-random seed. | `generator.seed(42)` |
| `uuid` | `str` | `` | `Random self{ SELF}` | Returns a new UUID string. | `id = generator.uuid()` |
| `str` | `str` | `int` | `Random self{ SELF} ; Obj length{Essential = true }` | Returns an alphanumeric pseudo-random string. | `token = generator.string(12)` |
| `weighted` | `any` | `list list` | `Random self{ SELF} ; Obj values{Essential = true } ; Obj weights{Essential = true }` | Returns a list value selected by matching numeric weights. | `item = generator.weighted(values, weights)` |
| `range` | `list` | `int int` | `Random self{ SELF} ; Obj start{Essential = true } ; Obj end = null!{Optional = true }` | Returns a shuffled list of integers in a range. | `values = generator.range(1, 10)` |
| `value` | `any` | `enum` | `Random self{ SELF} ; Obj obj{Essential = true }` | Returns a random value from an enum instance. | `status = generator.value(Status)` |

### `Re.cs` -- NativeModule: re

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `test` | `bool` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Checks whether text matches a pattern. | `write(test(\"\\d+\", text))` |
| `match` | `any` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Matches a pattern at the start of text. | `write(match(\"^UN\", text))` |
| `search` | `any` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Searches text for a pattern match. | `write(search(\"UN\", text))` |
| `find_all` | `list` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Finds every regular-expression match. | `matches = find_all(\"\\d+\", text)` |
| `groups` | `tuple` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Returns capture groups from a match. | `write(groups(pattern, text))` |
| `replace` | `str` | `str str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true } ; Obj replacement{Essential = true }` | Replaces the first pattern match. | `result = replace(\"cat\", text, \"dog\")` |
| `replace_all` | `str` | `str str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true } ; Obj replacement{Essential = true }` | Replaces every pattern match. | `result = replace_all(\"\\s+\", text, \" \")` |
| `split` | `list` | `str str` | `Obj pattern{Essential = true } ; Obj text{Essential = true }` | Splits text on a regular-expression pattern. | `parts = split(\",\\s*\", text)` |
| `escape` | `str` | `str` | `Obj text{Essential = true }` | Escapes text for a regular expression. | `write(escape(\"a+b\"))` |

### `Set.cs` -- BuiltinType: set

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `add` | `none` | `any` | `Set self{ SELF} ; Obj value{Essential = true }` | Adds a value to values. | `values.add(value)` |
| `remove` | `none` | `any` | `Set self{ SELF} ; Obj value{Essential = true }` | Removes a value from a set value. | `values.remove(value)` |
| `contains` | `boolean` | `any` | `Set self{ SELF} ; Obj value{Essential = true }` | Checks whether values contains a value. | `values.contains(value)` |
| `clear` | `none` | `` | `Set self{ SELF}` | Removes all values from values. | `values.clear()` |
| `clone` | `set` | `` | `Set self{ SELF}` | Returns the result of values.clone(). | `values.clone()` |
| `union` | `set` | `any` | `Set self{ SELF} ; Obj other{Essential = true }` | Returns the result of values.union(). | `values.union(other)` |
| `intersect` | `set` | `any` | `Set self{ SELF} ; Obj other{Essential = true }` | Returns the result of values.intersect(). | `values.intersect(other)` |
| `difference` | `set` | `any` | `Set self{ SELF} ; Obj other{Essential = true }` | Returns the result of values.difference(). | `values.difference(other)` |

### `Stopwatch.cs` -- NativeType: stopwatch

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `start` | `none` | `` | `Stopwatch self{ SELF}` | Starts timer. | `timer.start()` |
| `stop` | `none` | `` | `Stopwatch self{ SELF}` | Stops timer. | `timer.stop()` |
| `reset` | `none` | `` | `Stopwatch self{ SELF}` | Resets timer. | `timer.reset()` |
| `lap` | `int` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.lap(). | `timer.lap()` |
| `restart` | `none` | `` | `Stopwatch self{ SELF}` | Restarts timer. | `timer.restart()` |
| `is_running` | `bool` | `` | `Stopwatch self{ SELF}` | Checks whether a stopwatch value running. | `timer.is_running()` |
| `tick` | `int` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.tick(). | `timer.tick()` |
| `ms` | `int` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.ms(). | `timer.ms()` |
| `seconds` | `float` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.seconds(). | `timer.seconds()` |
| `minutes` | `float` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.minutes(). | `timer.minutes()` |
| `hours` | `float` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.hours(). | `timer.hours()` |
| `elapsed` | `date` | `` | `Stopwatch self{ SELF}` | Returns the result of timer.elapsed(). | `timer.elapsed()` |

### `Str.cs` -- BuiltinType: str

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `is_empty` | `bool` | `` | `Str self{ SELF}` | Checks whether a str value empty. | `text.is_empty()` |
| `is_number` | `bool` | `` | `Str self{ SELF}` | Checks whether a str value number. | `text.is_number()` |
| `is_alphabet` | `bool` | `` | `Str self{ SELF}` | Checks whether a str value alphabet. | `text.is_alphabet()` |
| `index_of` | `int` | `any` | `Str self{ SELF} ; Obj value{Essential = true }` | Finds a value index in text. | `text.index_of(value)` |
| `contains` | `bool` | `any` | `Str self{ SELF} ; Obj value{Essential = true }` | Checks whether text contains a value. | `text.contains(value)` |
| `starts_with` | `bool` | `any` | `Str self{ SELF} ; Obj value{Essential = true }` | Returns the result of text.starts with(). | `text.starts_with(value)` |
| `ends_with` | `bool` | `any` | `Str self{ SELF} ; Obj value{Essential = true }` | Returns the result of text.ends with(). | `text.ends_with(value)` |
| `to_upper` | `str` | `` | `Str self{ SELF}` | Converts a str value to upper. | `text.to_upper()` |
| `to_lower` | `str` | `` | `Str self{ SELF}` | Converts a str value to lower. | `text.to_lower()` |
| `split` | `list` | `any` | `Str self{ SELF} ; Obj sep{Essential = true }` | Returns the result of text.split(). | `text.split(sep)` |
| `trim` | `str` | `any` | `Str self{ SELF} ; Obj chars{Essential = true }` | Returns the result of text.trim(). | `text.trim(chars)` |
| `join` | `str` | `any` | `Str self{ SELF} ; Obj values{Essential = true }` | Returns the result of text.join(). | `text.join(values)` |
| `center` | `str` | `any any` | `Str self{ SELF} ; Obj width{Essential = true } ; Obj fill = null!{Optional = true }` | Returns the result of text.center(). | `text.center(width, fill)` |
| `left` | `str` | `any any` | `Str self{ SELF} ; Obj width{Essential = true } ; Obj fill = null!{Optional = true }` | Returns the result of text.left(). | `text.left(width, fill)` |
| `right` | `str` | `any any` | `Str self{ SELF} ; Obj width{Essential = true } ; Obj fill = null!{Optional = true }` | Returns the result of text.right(). | `text.right(width, fill)` |
| `replace` | `str` | `any any` | `Str self{ SELF} ; Obj oldValue{Essential = true, Name = "old_value" } ; Obj newValue{Optional = true, Name = "new_value" }` | Returns the result of text.replace(). | `text.replace(oldValue, newValue)` |
| `find` | `int` | `any` | `Str self{ SELF} ; Obj subStr{Essential = true, Name = "sub_str" }` | Returns the result of text.find(). | `text.find(subStr)` |

### `Stream.cs` -- NativeType: stream

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `read` | `str` | `` | `Stream self{ SELF}` | Reads all remaining text from a stream. | `stream.read()` |
| `read_line` | `str` | `` | `Stream self{ SELF}` | Reads the next line from a stream. | `line = stream.read_line()` |
| `write` | `none` | `any` | `Stream self{ SELF} ; Obj value{Essential = true }` | Writes a value to a stream without a trailing line break. | `stream.write(value)` |
| `write_line` | `none` | `any` | `Stream self{ SELF} ; Obj value{Essential = true }` | Writes a value and a trailing line break to a stream. | `stream.write_line(value)` |
| `flush` | `none` | `` | `Stream self{ SELF}` | Flushes buffered stream output. | `stream.flush()` |
| `close` | `none` | `` | `Stream self{ SELF}` | Closes and disposes a stream. | `stream.close()` |
| `seek` | `none` | `int` | `Stream self{ SELF} ; Int position{Essential = true }` | Seeks to an absolute byte position in a stream. | `stream.seek(0)` |
| `position` | `int` | `` | `Stream self{ SELF}` | Returns the current byte position in a stream. | `write(stream.position())` |
| `set_position` | `none` | `int` | `Stream self{ SELF} ; Int position{Essential = true }` | Sets the current byte position in a stream. | `stream.set_position(0)` |
| `length` | `int` | `` | `Stream self{ SELF}` | Returns the stream length in bytes. | `write(stream.length())` |
| `eof` | `bool` | `` | `Stream self{ SELF}` | Checks whether a readable stream reached end of file. | `write(stream.eof())` |
| `can_read` | `bool` | `` | `Stream self{ SELF}` | Checks whether a stream supports reading. | `write(stream.can_read())` |
| `can_write` | `bool` | `` | `Stream self{ SELF}` | Checks whether a stream supports writing. | `write(stream.can_write())` |
| `can_seek` | `bool` | `` | `Stream self{ SELF}` | Checks whether a stream supports seeking. | `write(stream.can_seek())` |

### `Sys.cs` -- NativeModule: sys

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `env", Description = "Reads an environment variable by name.", Example = "write(sys.env(\"HOME\"))", ReturnType = "str", ArgumentTypes = new[] { "str" }` | `str", ArgumentTypes = new[] { "str" }` | `str` | `Obj name{Essential = true }` | Reads an environment variable by name.", Example = "write(sys.env(\"HOME\"))", ReturnType = "str", ArgumentTypes = new[] { "str" } | `write(sys.env(\"HOME\"))", ReturnType = "str", ArgumentTypes = new[] { "str" }` |
| `environ", Description = "Returns all environment variables as text entries.", Example = "values = sys.environ()", ReturnType = "list` | `list` | `` | `` | Returns all environment variables as text entries.", Example = "values = sys.environ()", ReturnType = "list | `values = sys.environ()", ReturnType = "list` |
| `expand_vars", Description = "Expands environment-variable placeholders in text.", Example = "path = sys.expand_vars(\"$HOME/data\")", ReturnType = "str", ArgumentTypes = new[] { "str" }` | `str", ArgumentTypes = new[] { "str" }` | `str` | `Obj text{Essential = true }` | Expands environment-variable placeholders in text.", Example = "path = sys.expand_vars(\"$HOME/data\")", ReturnType = "str", ArgumentTypes = new[] { "str" } | `path = sys.expand_vars(\"$HOME/data\")", ReturnType = "str", ArgumentTypes = new[] { "str" }` |
| `pid", Description = "Returns the current process identifier.", Example = "write(sys.pid())", ReturnType = "int` | `int` | `` | `` | Returns the current process identifier.", Example = "write(sys.pid())", ReturnType = "int | `write(sys.pid())", ReturnType = "int` |
| `name", Description = "Returns the current operating-system name.", Example = "write(sys.name())", ReturnType = "str` | `str` | `` | `` | Returns the current operating-system name.", Example = "write(sys.name())", ReturnType = "str | `write(sys.name())", ReturnType = "str` |
| `arch", Description = "Returns the operating-system architecture.", Example = "write(sys.arch())", ReturnType = "str` | `str` | `` | `` | Returns the operating-system architecture.", Example = "write(sys.arch())", ReturnType = "str | `write(sys.arch())", ReturnType = "str` |
| `version", Description = "Returns the operating-system version.", Example = "write(sys.version())", ReturnType = "str` | `str` | `` | `` | Returns the operating-system version.", Example = "write(sys.version())", ReturnType = "str | `write(sys.version())", ReturnType = "str` |
| `cpuCount", Description = "Returns the available processor count.", Example = "write(sys.cpuCount())", ReturnType = "int` | `int` | `` | `` | Returns the available processor count.", Example = "write(sys.cpuCount())", ReturnType = "int | `write(sys.cpuCount())", ReturnType = "int` |
| `is64bit", Description = "Checks whether the operating system is 64-bit.", Example = "write(sys.is64bit())", ReturnType = "bool` | `bool` | `` | `` | Checks whether the operating system is 64-bit.", Example = "write(sys.is64bit())", ReturnType = "bool | `write(sys.is64bit())", ReturnType = "bool` |
| `hostname", Description = "Returns the current machine host name.", Example = "write(sys.hostname())", ReturnType = "str` | `str` | `` | `` | Returns the current machine host name.", Example = "write(sys.hostname())", ReturnType = "str | `write(sys.hostname())", ReturnType = "str` |
| `username", Description = "Returns the current operating-system user name.", Example = "write(sys.username())", ReturnType = "str` | `str` | `` | `` | Returns the current operating-system user name.", Example = "write(sys.username())", ReturnType = "str | `write(sys.username())", ReturnType = "str` |
| `args", Description = "Returns the process command-line arguments.", Example = "values = sys.args()", ReturnType = "list` | `list` | `` | `` | Returns the process command-line arguments.", Example = "values = sys.args()", ReturnType = "list | `values = sys.args()", ReturnType = "list` |
| `sep", Description = "Returns the directory separator character.", Example = "write(sys.sep())", ReturnType = "str` | `str` | `` | `` | Returns the directory separator character.", Example = "write(sys.sep())", ReturnType = "str | `write(sys.sep())", ReturnType = "str` |
| `pathsep", Description = "Returns the path-list separator character.", Example = "write(sys.pathsep())", ReturnType = "str` | `str` | `` | `` | Returns the path-list separator character.", Example = "write(sys.pathsep())", ReturnType = "str | `write(sys.pathsep())", ReturnType = "str` |
| `linesep", Description = "Returns the environment line separator.", Example = "write(sys.linesep())", ReturnType = "str` | `str` | `` | `` | Returns the environment line separator.", Example = "write(sys.linesep())", ReturnType = "str | `write(sys.linesep())", ReturnType = "str` |
| `tickCount", Description = "Returns elapsed system ticks in milliseconds.", Example = "write(sys.tickCount())", ReturnType = "int` | `int` | `` | `` | Returns elapsed system ticks in milliseconds.", Example = "write(sys.tickCount())", ReturnType = "int | `write(sys.tickCount())", ReturnType = "int` |
| `getcalldepth", Description = "Returns current maximum recursion depth.", Example = "write(sys.getcalldepth())", ReturnType = "int` | `int` | `` | `` | Returns current maximum recursion depth.", Example = "write(sys.getcalldepth())", ReturnType = "int | `write(sys.getcalldepth())", ReturnType = "int` |
| `setcalldepth", Description = "Sets maximum recursion depth.", Example = "sys.setcalldepth(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" }` | `none", ArgumentTypes = new[] { "int" }` | `int` | `Obj depth{Essential = true }` | Sets maximum recursion depth.", Example = "sys.setcalldepth(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" } | `sys.setcalldepth(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" }` |
| `getrecursionlimit", Description = "Returns current maximum recursion depth (alias).", Example = "write(sys.getrecursionlimit())", ReturnType = "int` | `int` | `` | `` | Returns current maximum recursion depth (alias).", Example = "write(sys.getrecursionlimit())", ReturnType = "int | `write(sys.getrecursionlimit())", ReturnType = "int` |
| `setrecursionlimit", Description = "Sets maximum recursion depth (alias).", Example = "sys.setrecursionlimit(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" }` | `none", ArgumentTypes = new[] { "int" }` | `int` | `Obj depth{Essential = true }` | Sets maximum recursion depth (alias).", Example = "sys.setrecursionlimit(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" } | `sys.setrecursionlimit(20000)", ReturnType = "none", ArgumentTypes = new[] { "int" }` |

### `Time.cs` -- NativeModule: time

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `sleep` | `none` | `int \| float` | `Obj milliseconds{}` | Waits for a number of milliseconds. | `sleep(250)` |
| `now` | `date` | `` | `` | Returns the current date and time. | `write(now())` |
| `stopwatch` | `stopwatch` | `` | `` | Creates a stopwatch object. | `timer = stopwatch()` |

### `WebSocket.cs` -- NativeType: web_socket

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `send` | `none` | `any` | `WebSocket self{ SELF} ; Obj text{Essential = true }` | Sends data through socket. | `socket.send(text)` |
| `receive` | `str` | `` | `WebSocket self{ SELF}` | Receives data from socket. | `socket.receive()` |
| `close` | `none` | `` | `WebSocket self{ SELF}` | Closes socket. | `socket.close()` |
| `is_open` | `bool` | `` | `WebSocket self{ SELF}` | Checks whether a web_socket value open. | `socket.is_open()` |

### `Ws.cs` -- NativeModule: ws

| fn | ret | arg types | C# params | desc | example |
|---|---|---|---|---|---|
| `connect` | `web_socket` | `str` | `Obj url{Essential = true }` | Connects to a WebSocket endpoint. | `socket = connect(\"wss://example.com\")` |

## 6. error string inventory

Every `new Err(...)` / `new Panic(...)` / `new Error(...)` argument in `src` is
kept verbatim. `L{n}` is the line number inside the file.

### `Builtin.cs`

- L63 `Err` : `"cannot convert argument 'message' to a str"`
- L68 `Err` : `"cannot convert argument 'name' to a str"`
- L70 `Err` : `messageValue.Value, nameValue.Value`
- L86 `Err` : `"throw_if: expected bool condition"`
- L107 `Err` : `"throw_unless: expected bool condition"`
- L125 `Err` : `"chr: expected int code"`
- L127 `Err` : `"chr: code point out of range (0..1114111)"`
- L141 `Err` : `"ord: expected str"`
- L143 `Err` : `"ord: expected a single character, got empty string"`
- L157 `Err` : `"bin: expected int"`
- L173 `Err` : `"oct: expected int"`
- L189 `Err` : `"hex: expected int"`
- L215 `Err` : `"ascii: expected str"`

### `Context.cs`

- L20 `Panic` : `"stack is empty"`

### `Counter.cs`

- L24 `Err` : `$"invalid '{Type}' initialize"`
- L29 `Err` : `"counter is infinite"`
- L31 `Err` : `"counter is infinite"`
- L33 `Err` : `"counter is infinite"`
- L35 `Err` : `"counter is infinite"`

### `Date.cs`

- L17 `Err` : `"invalid date str"`
- L18 `Err` : `$"cannot convert '{args[0].Type}' to 'date'"`
- L20 `Err` : `$"cannot convert to 'date'"`
- L27 `Err` : `$"unsupported operand type(s) for +: 'date' and '{other.Type}'"`
- L33 `Err` : `$"unsupported operand type(s) for -: 'date' and '{other.Type}'"`

### `Dict.cs`

- L22 `Err` : `$"invaild '{Type}' initialize"`
- L27 `Err` : `$"key '{Str.To(key).Value}' not found in dictionary"`

### `Enu.cs`

- L15 `Err` : `$"'{Type}' takes at most 1 argument, {args.Count} given"`
- L26 `Err` : `$"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'"`
- L33 `Err` : `$"unsupported operand type(s) for <: '{Type}' and '{other.Type}'"`

### `Evaluator.cs`

- L21 `Error` : `"invalid int object", node, context.Source`
- L22 `Error` : `"invalid bool object", node, context.Source`
- L23 `Error` : `"invalid float object", node, context.Source`
- L24 `Error` : `"invalid str object", node, context.Source`
- L73 `Error` : `$"unsupported node kind {node.Kind}", node, context.Source`
- L95 `Error` : `$"undefined variable '{name}'", node, context.Source`
- L122 `Error` : `"invalid assignment target", target, context.Source`
- L128 `Error` : `"cannot unpack non-tuple", target, context.Source`
- L131 `Error` : `$"expected {target.Children.Count} values, got {tuple.Count}", target, context.Source`
- L164 `Err` : `e.Message, e.Header`
- L173 `Error` : `err2.Message, node.Start, node.Length, context.Source, header: err2.Header`
- L187 `Error` : `$"invalid unary operator {node.Operator}", node, context.Source`
- L242 `Error` : `$"invalid binary operator {node.Operator}", node, context.Source`
- L323 `Error` : `err.Message, arg, context.Source, header: err.Header`
- L338 `Error` : `"** argument must be dict", arg, context.Source`
- L358 `Error` : `err.Message, arg, context.Source, header: err.Header`
- L409 `Err` : `"annotation error"`
- L434 `Err` : `"annotation error"`
- L459 `Error` : `"'wait' requires Future", node, context.Source`
- L494 `Error` : `err.Message, pattern, context.Source, header: err.Header`
- L517 `Error` : `$"invalid pattern {pattern.Kind}", pattern, context.Source`
- L541 `Error` : `"no match case", node, context.Source`
- L680 `Error` : `"invalid enum", node, context.Source`
- L727 `Error` : `"invalid initializer", node, context.Source`
- L749 `Error` : `$"unknown base class '{firstName}'", node, context.Source`
- L760 `Error` : `$"unknown base class '{baseName}'", node, context.Source`
- L787 `Error` : `$"duplicate getter for property '{accessorName}'", member, context.Source`
- L794 `Error` : `$"duplicate setter for property '{accessorName}'", member, context.Source`
- L811 `Error` : `$"property '{memberName}' conflicts with a method", member, context.Source`
- L877 `Error` : `$"{GetText(iterable)} is not iterable", node, context.Source`
- L1010 `Err` : `e.Message, e.Header`
- L1090 `Error` : `"f-string expression must be a string", node, context.Source`
- L1126 `Error` : `"tagged template target must be a function", node, context.Source`
- L1140 `Error` : `"annotation must evaluate to function", annotation, context.Source`
- L1180 `Error` : `$"invalid pattern {pattern.Kind}", pattern, context.Source`
- L1227 `Error` : `err.Message, pattern, context.Source, header: err.Header`
- L1230 `Error` : `"tuple unpack mismatch", pattern, context.Source`
- L1256 `Error` : `"slice assignment only supports list", pattern, context.Source`
- L1265 `Error` : `"slice start must be int", pattern, context.Source`
- L1270 `Error` : `"slice end must be int", pattern, context.Source`
- L1275 `Error` : `"slice step must be int", pattern, context.Source`
- L1276 `Error` : `"slice assignment with step != 1 not supported", pattern, context.Source`
- L1282 `Error` : `"slice assignment value must be list", pattern, context.Source`
- L1292 `Error` : `err.Message, pattern, context.Source, header: err.Header`
- L1298 `Error` : `err.Message, pattern, context.Source, header: err.Header`
- L1303 `Error` : `$"invalid pattern {pattern.Kind}", pattern, context.Source`
- L1316 `Err` : `$"cannot unpack {obj.GetType().Name}"`
- L1323 `Error` : `err.Message, node, context.Source`

### `Float.cs`

- L16 `Err` : `$"cannot convert to '{Type}'"`
- L24 `Err` : `$"unsupported operand type(s) for +: 'float' and '{other.Type}'"`
- L31 `Err` : `$"unsupported operand type(s) for -: 'float' and '{other.Type}'"`
- L38 `Err` : `$"unsupported operand type(s) for *: 'float' and '{other.Type}'"`
- L43 `Err` : `$"{Type} division by zero"`
- L44 `Err` : `$"{Type} division by zero"`
- L45 `Err` : `$"unsupported operand type(s) for /: 'float' and '{other.Type}'"`
- L52 `Err` : `$"unsupported operand type(s) for %: 'float' and '{other.Type}'"`
- L57 `Err` : `$"{Type} division by zero"`
- L58 `Err` : `$"{Type} division by zero"`
- L59 `Err` : `$"unsupported operand type(s) for //: 'float' and '{other.Type}'"`
- L66 `Err` : `$"unsupported operand type(s) for **: 'float' and '{other.Type}'"`
- L73 `Err` : `$"float pow overflow: {Value} ** {exp}"`
- L86 `Err` : `$"unsupported operand type(s) for ==: 'float' and '{other.Type}'"`
- L94 `Err` : `$"unsupported operand type(s) for !=: 'float' and '{other.Type}'"`
- L101 `Err` : `$"unsupported operand type(s) for <: 'float' and '{other.Type}'"`
- L108 `Err` : `$"unsupported operand type(s) for >: 'float' and '{other.Type}'"`
- L115 `Err` : `$"unsupported operand type(s) for <=: 'float' and '{other.Type}'"`
- L122 `Err` : `$"unsupported operand type(s) for >=: 'float' and '{other.Type}'"`

### `Flow.cs`

- L23 `Err` : `$"argument 'worker' must be of type int"`

### `Fn.cs`

- L45 `Err` : `$"multiple values for argument '{name}'"`
- L81 `Err` : `$"missing required argument '{arg.Name}'"`
- L92 `Err` : `"too many positional arguments"`
- L104 `Err` : `$"unexpected keyword argument '{keyword.Keys.First()}'"`
- L149 `Err` : `$"missing required argument '{arg.Name}'"`
- L155 `Err` : `"too many positional arguments"`
- L210 `Error` : `"invalid parameter", tuple, context.Source`
- L273 `Error` : `"invalid function parameter", node, context.Source`
- L289 `Error` : `"invalid argument name", node, context.Source`

### `Fs.cs`

- L16 `Err` : `"expected path is string"`
- L83 `Err` : `"file not found"`
- L103 `Err` : `"expected text is string"`
- L125 `Err` : `"expected text is string"`
- L211 `Err` : `"expected name is string"`
- L275 `Err` : `"expected recursive is bool"`
- L298 `Err` : `"directory not found"`
- L316 `Err` : `"directory not found"`
- L334 `Err` : `"directory not found"`
- L352 `Err` : `"directory not found"`
- L370 `Err` : `"file not found"`
- L441 `Err` : `"file not found"`
- L459 `Err` : `"path not found"`
- L474 `Err` : `"expected tuple"`
- L481 `Err` : `"expected path is string"`
- L620 `Err` : `"directory not found"`

### `Global.cs`

- L107 `Panic` : `$"module '{name}' is not allowed in this environment"`
- L162 `Panic` : `$"module '{moduleName}' has no member '{name}'"`
- L178 `Panic` : `$"'{k}' already exists in module '{moduleAlias}'"`
- L185 `Panic` : `$"module '{moduleName}' has no member '{name}'"`
- L188 `Panic` : `$"'{alias}' already exists in module '{moduleAlias}'"`
- L208 `Panic` : `$"module '{moduleName}' has no member '{name}'"`
- L235 `Panic` : `$"native module '{name}' not found"`
- L258 `Panic` : `$"circular import detected: '{fullPath}'"`
- L291 `Panic` : `$"module '{fullPath}' not found"`
- L383 `Err` : `$"class '{name}' not found"`
- L395 `Err` : `$"type '{type}' is not a class"`
- L521 `Err` : `$"'{value.Type}' object has no attribute '{name}'"`
- L524 `Err` : `$"global variable '{name}' not found"`

### `Http.cs`

- L23 `Err` : `e.Message`

### `HttpClient.cs`

- L12 `Err` : `"'http' cannot be constructed directly; use http.connect()"`
- L29 `Err` : `"expected key is string"`
- L32 `Err` : `"expected value is string"`
- L62 `Err` : `"expected url is string"`
- L70 `Err` : `e.Message`
- L85 `Err` : `"expected url is string"`
- L93 `Err` : `e.Message`
- L107 `Err` : `"expected url is string"`
- L124 `Err` : `e.Message`
- L139 `Err` : `"expected url is string"`
- L148 `Err` : `e.Message`
- L165 `Err` : `"expected url is string"`
- L168 `Err` : `"expected body is string"`
- L180 `Err` : `e.Message`
- L198 `Err` : `"expected url is string"`
- L201 `Err` : `"expected body is string"`
- L213 `Err` : `e.Message`
- L230 `Err` : `"expected url is string"`
- L233 `Err` : `"expected body is string"`
- L245 `Err` : `e.Message`
- L263 `Err` : `"expected url is string"`
- L266 `Err` : `"expected body is string"`
- L278 `Err` : `e.Message`
- L295 `Err` : `"expected url is string"`
- L298 `Err` : `"expected body is string"`
- L322 `Err` : `e.Message`
- L340 `Err` : `"expected url is string"`
- L343 `Err` : `"expected body is string"`
- L360 `Err` : `e.Message`
- L374 `Err` : `"expected url is string"`
- L385 `Err` : `e.Message`
- L400 `Err` : `"expected url is string"`
- L412 `Err` : `e.Message`
- L426 `Err` : `"expected url is string"`
- L445 `Err` : `e.Message`
- L460 `Err` : `"expected url is string"`
- L479 `Err` : `e.Message`

### `Inspect.cs`

- L49 `Err` : `"expected 'name' argument to be of type 'str'"`
- L68 `Err` : `"expected 'name' argument to be of type 'str'"`
- L85 `Err` : `"expected 'name' argument to be of type 'str'"`
- L102 `Err` : `"expected 'name' argument to be of type 'str'"`
- L104 `Err` : `$"attribute '{nameValue.Value}' not found"`
- L120 `Err` : `"expected 'name' argument to be of type 'str'"`
- L122 `Err` : `$"attribute '{nameValue.Value}' not found"`

### `Int.cs`

- L21 `Err` : `$"int: expected (str, int) for base conversion"`
- L22 `Err` : `$"cannot convert to '{Type}'"`
- L30 `Err` : `"int: base must be between 2 and 36"`
- L44 `Err` : `$"int: invalid literal for base {@base}"`
- L52 `Err` : `$"int: {e.Message}"`
- L61 `Err` : `$"unsupported operand type(s) for +: 'int' and '{other.Type}'"`
- L68 `Err` : `$"unsupported operand type(s) for -: 'int' and '{other.Type}'"`
- L75 `Err` : `$"unsupported operand type(s) for *: 'int' and '{other.Type}'"`
- L80 `Err` : `$"{Type} division by zero"`
- L81 `Err` : `$"{Type} division by zero"`
- L82 `Err` : `$"unsupported operand type(s) for /: 'int' and '{other.Type}'"`
- L89 `Err` : `$"unsupported operand type(s) for %: 'int' and '{other.Type}'"`
- L94 `Err` : `$"{Type} division by zero"`
- L95 `Err` : `$"{Type} division by zero"`
- L96 `Err` : `$"unsupported operand type(s) for //: 'int' and '{other.Type}'"`
- L103 `Err` : `$"unsupported operand type(s) for **: 'int' and '{other.Type}'"`
- L108 `Err` : `$"int pow: negative exponent {exp}"`
- L111 `Err` : `$"int pow overflow: {Value} ** {exp}"`
- L119 `Err` : `$"int pow overflow: {Value} ** {exp}"`
- L130 `Err` : `$"unsupported operand type(s) for &: 'int' and '{other.Type}'"`
- L136 `Err` : `$"unsupported operand type(s) for \|: 'int' and '{other.Type}'"`
- L142 `Err` : `$"unsupported operand type(s) for ^: 'int' and '{other.Type}'"`
- L150 `Err` : `$"unsupported operand type(s) for <<: 'int' and '{other.Type}'"`
- L156 `Err` : `$"unsupported operand type(s) for >>: 'int' and '{other.Type}'"`
- L164 `Err` : `$"unsupported operand type(s) for ==: 'int' and '{other.Type}'"`
- L171 `Err` : `$"unsupported operand type(s) for <: 'int' and '{other.Type}'"`
- L178 `Err` : `$"unsupported operand type(s) for >: 'int' and '{other.Type}'"`
- L185 `Err` : `$"unsupported operand type(s) for <=: 'int' and '{other.Type}'"`
- L192 `Err` : `$"unsupported operand type(s) for >=: 'int' and '{other.Type}'"`
- L200 `Err` : `$"unsupported operand type(s) for !=: 'int' and '{other.Type}'"`

### `IO.cs`

- L55 `Err` : `"expected 'stream' argument to be of type 'stream'"`
- L58 `Err` : `"stream is not writable"`
- L61 `Err` : `"expected 'value' argument to be a tuple"`
- L64 `Err` : `"expected 'sep' argument to be a string"`
- L67 `Err` : `"expected 'end' argument to be a string"`
- L80 `Err` : `"expected all values to be strings or representable as strings"`
- L108 `Err` : `"expected 'input' argument to be of type 'stream'"`
- L111 `Err` : `"expected 'output' argument to be of type 'stream'"`
- L114 `Err` : `"stream is not writable"`
- L117 `Err` : `"stream is not readable"`
- L127 `Err` : `"expected all values to be strings or representable as strings"`
- L161 `Err` : `"expected 'path' argument to be a string"`
- L166 `Err` : `"expected 'mode' argument to be a string"`
- L172 `Err` : `"invalid 'mode' argument, expected one of 'r', 'w', 'a', or 'rw'"`
- L177 `Err` : `"file not found"`
- L203 `Err` : `"file not found"`
- L211 `Err` : `$"invalid file mode '{modeType}'"`

### `Iter.cs`

- L17 `Err` : `"expected 'iterable' argument to be a tuple"`
- L23 `Err` : `"expected 'iterable' argument to have at least one value"`
- L28 `Err` : `"invalid argument type of 'iterable'"`
- L43 `Err` : `"expected 'iterable' argument to be a tuple"`
- L66 `Err` : `"expected 'size' argument to be a 'list'"`
- L76 `Err` : `"expected all items in 'size' list to be integers"`
- L81 `Err` : `"expected 'size' argument to have at least one dimension"`
- L112 `Err` : `"expected 'start' argument to be an integer"`
- L114 `Err` : `"expected 'end' argument to be an integer"`
- L116 `Err` : `"expected 'step' argument to be an integer"`
- L119 `Err` : `"step argument must not be zero"`
- L139 `Err` : `"expected 'start' argument to be an integer"`
- L153 `Err` : `"expected 'obj' argument to be of type 'iter'"`
- L169 `Err` : `"expected 'obj' argument to be of type 'iter'"`
- L174 `Err` : `"expected 'count' argument to be of type 'int'"`
- L189 `Err` : `"expected 'iterables' argument to be of type 'iter'"`
- L198 `Err` : `"expected all 'iterables' items to be of type 'iter'"`
- L204 `Err` : `"expected 'iterables' argument to have a non-zero length"`
- L249 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L300 `Err` : `$"{max} < {values[i]} is not a boolean"`
- L328 `Err` : `$"{min} > {values[i]} is not a boolean"`
- L349 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L361 `Err` : `"expected 'predicate' to return a boolean"`
- L382 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L411 `Err` : `"expected 'count' argument to be an integer"`
- L414 `Err` : `"expected 'count' argument to be non-negative"`
- L417 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L446 `Err` : `"expected 'count' argument to be an integer"`
- L449 `Err` : `"expected 'count' argument to be non-negative"`
- L452 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L477 `Err` : `"expected 'iterables' argument to be of type 'iter'"`
- L484 `Err` : `"expected all 'iterables' items to be of type 'iter'"`
- L503 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L535 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L545 `Err` : `"expected 'predicate' to return a boolean"`
- L566 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L576 `Err` : `"expected 'predicate' to return a boolean"`
- L597 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L609 `Err` : `"expected 'predicate' to return a boolean"`
- L628 `Err` : `"expected 'iterable' argument to be of type 'iter'"`
- L642 `Err` : `$"{items[j]} > {key} is not a boolean"`
- L17 `Err` : `$"invaild '{Type}' initialize"`
- L38 `Err` : `"iteration stopped"`
- L57 `Err` : `"invalid argument: 'n'"`
- L72 `Err` : `"invalid argument: 'n'"`
- L111 `Err` : `"invalid argument: 'fn'"`
- L136 `Err` : `"invalid argument: 'fn'"`
- L155 `Err` : `"invalid argument: 'fn'"`
- L176 `Err` : `"invalid argument: 'fn'"`

### `Json.cs`

- L26 `Err` : `$"invalid json: expected 1 argument, got {args.Count}"`
- L35 `Err` : `$"invalid json: expected string, dict, or list, got {args[0].Type}"`

### `Lexer.cs`

- L133 `Error` : `token.Message ?? "syntax error", token.Start, token.Length, source`

### `LFn.cs`

- L12 `Err` : `"maximum recursion depth exceeded"`

### `List.cs`

- L84 `Err` : `"equals operand is must be a boolean"`
- L92 `Err` : `"list index out of range"`
- L93 `Err` : `"invalid index type"`
- L102 `Err` : `"slice start must be int"`
- L107 `Err` : `"slice end must be int"`
- L112 `Err` : `"slice step must be int"`
- L113 `Err` : `"slice step cannot be zero"`
- L134 `Err` : `"invalid index type"`
- L142 `Err` : `"list index out of range"`
- L150 `Err` : `$"cannot check if '{obj.Type}' is in '{Type}'"`
- L283 `Err` : `"invalid arguments: index"`
- L361 `Err` : `"invalid arguments: index"`
- L411 `Err` : `"invalid arguments: fn"`
- L467 `Err` : `$"lower_bound requires '<' to return bool"`
- L491 `Err` : `$"upper_bound requires '>' to return bool"`
- L534 `Err` : `"list is empty"`
- L573 `Err` : `"invalid argumnets: index"`
- L579 `Err` : `"list index out of range"`

### `Lock.cs`

- L29 `Err` : `"'lock' cannot be cloned"`
- L72 `Err` : `"lock not held"`

### `Math.cs`

- L26 `Err` : `"expected number type"`
- L36 `Err` : `"expected integer type"`
- L69 `Err` : `"digit must be an integer between 0 and 15"`
- L167 `Err` : `"cannot compute sqrt of a negative number"`
- L241 `Err` : `"expected value greater than 0"`
- L265 `Err` : `"expected value greater than 0"`
- L283 `Err` : `"expected value greater than 0"`
- L440 `Err` : `"min must be less than or equal to max"`

### `NFn.cs`

- L16 `Err` : `"maximum recursion depth exceeded"`

### `Node.cs`

- L32 `Panic` : `$"unsupported const type: {value.GetType()}"`

### `Obj.cs`

- L71 `Err` : `$"unsupported operand type(s) for +: '{Type}' and '{other.Type}'"`
- L73 `Err` : `$"unsupported operand type(s) for -: '{Type}' and '{other.Type}'"`
- L75 `Err` : `$"unsupported operand type(s) for *: '{Type}' and '{other.Type}'"`
- L77 `Err` : `$"unsupported operand type(s) for /: '{Type}' and '{other.Type}'"`
- L79 `Err` : `$"unsupported operand type(s) for //: '{Type}' and '{other.Type}'"`
- L81 `Err` : `$"unsupported operand type(s) for %: '{Type}' and '{other.Type}'"`
- L83 `Err` : `$"unsupported operand type(s) for **: '{Type}' and '{other.Type}'"`
- L96 `Err` : `$"unsupported operand type(s) for ..: '{Type}' and '{other.Type}'"`
- L99 `Err` : `$"unsupported operand type(s) for &: '{Type}' and '{other.Type}'"`
- L101 `Err` : `$"unsupported operand type(s) for \|: '{Type}' and '{other.Type}'"`
- L103 `Err` : `$"unsupported operand type(s) for ^: '{Type}' and '{other.Type}'"`
- L105 `Err` : `$"unsupported operand type(s) for <<: '{Type}' and '{other.Type}'"`
- L107 `Err` : `$"unsupported operand type(s) for >>: '{Type}' and '{other.Type}'"`
- L109 `Err` : `$"unsupported operand type(s) for (): '{Type}'"`
- L111 `Err` : `$"unsupported operand type(s) for []: '{Type}'"`
- L113 `Err` : `$"unsupported operand type(s) for +: '{Type}'"`
- L115 `Err` : `$"unsupported operand type(s) for -: '{Type}'"`
- L117 `Err` : `$"unsupported operand type(s) for ~: '{Type}'"`
- L119 `Err` : `"unsupported operand type(s) for len()"`
- L121 `Err` : `"cannot hashable object"`
- L123 `Err` : `$"unsupported operand type(s) for int(): '{Type}'"`
- L125 `Err` : `$"unsupported operand type(s) for float(): '{Type}'"`
- L127 `Err` : `$"unsupported operand type(s) for bool(): '{Type}'"`
- L129 `Err` : `$"unsupported operand type(s) for list(): '{Type}'"`
- L131 `Err` : `$"unsupported operand type(s) for tuple(): '{Type}'"`
- L133 `Err` : `$"unsupported operand type(s) for str(): '{Type}'"`
- L135 `Err` : `$"unsupported operand type(s) for using entry: '{Type}'"`
- L137 `Err` : `$"unsupported operand type(s) for using exit: '{Type}'"`
- L139 `Err` : `$"unsupported operand type(s) for iter(): '{Type}'"`
- L141 `Err` : `$"unsupported operand type(s) for next(): '{Type}'"`
- L145 `Err` : `$"unsupported operand type(s) for *: '{Type}'"`
- L150 `Err` : `"left operand must be a boolean"`
- L152 `Err` : `"right operand must be a boolean"`
- L158 `Err` : `"operand must be a boolean"`
- L174 `Err` : `$"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'"`
- L177 `Err` : `"operands must be booleans"`
- L179 `Err` : `$"unsupported operand type(s) for <: '{Type}' and '{other.Type}'"`
- L184 `Err` : `$"unsupported operand type(s) for >: '{Type}' and '{other.Type}'"`
- L186 `Err` : `$"unsupported operand type(s) for >: '{Type}' and '{other.Type}'"`
- L194 `Err` : `$"unsupported operand type(s) for <=: '{Type}' and '{other.Type}'"`
- L196 `Err` : `$"unsupported operand type(s) for <=: '{Type}' and '{other.Type}'"`
- L204 `Err` : `$"unsupported operand type(s) for >=: '{Type}' and '{other.Type}'"`
- L206 `Err` : `$"unsupported operand type(s) for >=: '{Type}' and '{other.Type}'"`
- L218 `Err` : `$"unsupported slice for '{Type}'"`
- L235 `Err` : `$"'{Type}' object has no attribute '{name}'"`
- L266 `Err` : `$"'{Type}' object has no attribute '{name}'"`
- L270 `Err` : `$"'{Type}' object has no attribute '{name}'"`
- L288 `Err` : `$"unsupported operand type(s) for [] = 'value': '{Type}'"`
- L308 `Err` : `$"unsupported operand type(s) for in: '{Type}'"`
- L457 `Panic` : `"types that are not comparable to each other."`

### `Optimizer.cs`

- L26 `Panic` : `$"optimizer did not reach a fixed point within {MaxPasses} passes"`
- L186 `Panic` : `$"unsupported optimize types: {a.GetType().Name}, {b.GetType().Name}"`
- L197 `Panic` : `"division by zero"`
- L198 `Panic` : `"division by zero"`
- L199 `Panic` : `"division by zero"`
- L210 `Panic` : `$"invalid optimize operator '{op}' for long"`
- L216 `Panic` : `"negative exponent for int pow"`
- L219 `Panic` : `$"integer pow overflow: {a} ** {b}"`
- L231 `Panic` : `"division by zero"`
- L232 `Panic` : `"division by zero"`
- L243 `Panic` : `$"invalid optimize operator '{op}' for double"`
- L251 `Panic` : `$"float pow overflow: {a} ** {b}"`

### `Os.cs`

- L17 `Err` : `"expected value is string"`
- L41 `Err` : `"environment variable not found"`
- L247 `Err` : `"expected code is int"`
- L270 `Err` : `"command is empty"`
- L273 `Err` : `"command too long"`
- L292 `Err` : `"failed to start process"`
- L300 `Err` : `"command timed out after 10s"`
- L304 `Err` : `string.IsNullOrWhiteSpace(error) ? $"command failed with exit code {process.ExitCode}" : error.Trim()`
- L310 `Err` : `e.Message`

### `Parser.cs`

- L40 `Error` : `msg, Current.Start, Current.Length, source`
- L109 `Error` : `$"unexpected token {Current.Type}", Current.Start, Current.Length, source`
- L244 `Error` : `"using requires an assignment", expr, source`
- L566 `Error` : `"invalid assignment target", left.Start, left.Length, source`
- L717 `Error` : `"assignment chaining is not supported", Current.Start, Current.Length, source`
- L940 `Error` : `$"unexpected primary token {Current.Type}", Current.Start, Current.Length, source`
- L1011 `Error` : `"unclosed interpolation", textStart + exprStart - 1, text.Length - exprStart + 1, source`
- L1044 `Error` : `error.Message, start + error.Start, error.Lenght, this.source, error.Header, error.InnerException`
- L1074 `Error` : `$"invalid literal {kind}", token.Start, token.Length, source`
- L1084 `Error` : `"integer literal is too large", token.Start, token.Length, source`
- L1088 `Error` : `"integer literal is too large", token.Start, token.Length, source`
- L1092 `Error` : `"integer literal is too large", token.Start, token.Length, source`
- L1097 `Error` : `"integer literal is too large", token.Start, token.Length, source`
- L1105 `Error` : `"invalid floating-point literal", token.Start, token.Length, source`
- L1218 `Error` : `"set cannot contain key-value pairs", Current.Start, Current.Length, context.Source`
- L1441 `Error` : `"annotations are not allowed here", annotations[0].Start, annotations[^1].Start + annotations[^1].Length - annotations[0].Start, source`

### `Pool.cs`

- L35 `Err` : `"expected a worker count as an integer"`
- L36 `Err` : `"pool takes at most one argument"`
- L51 `Err` : `"'lock' cannot be cloned"`
- L66 `Err` : `"expected 'fn' argument to be of type 'func'"`
- L69 `Err` : `"expected 'args' argument to be of type 'collection'"`
- L89 `Err` : `"expected 'fn' argument to be of type 'func'"`
- L92 `Err` : `"expected 'iterable' argument to be of type 'iterable'"`

### `Random.cs`

- L41 `Err` : `"expected 'random.int' argument 'min' is int"`
- L43 `Err` : `"expected 'random.int' argument 'max' is int"`
- L45 `Err` : `"'min' cannot be greater than 'max'"`
- L65 `Err` : `"'min' and 'max' must be specified together"`
- L68 `Err` : `"expected 'random.float' argument 'min' is float"`
- L70 `Err` : `"expected 'random.float' argument 'max' is float"`
- L72 `Err` : `"'min' cannot be greater than 'max'"`
- L95 `Err` : `"expected 'random.bytes' argument 'length' is int"`
- L97 `Err` : `"'length' cannot be negative"`
- L115 `Err` : `"expected 'random.shuffle' argument to be a list"`
- L132 `Err` : `"expected 'random.choice' argument to be a list"`
- L147 `Err` : `"expected 'random.choices' argument 'obj' is list"`
- L149 `Err` : `"expected 'random.choices' argument 'count' is int"`
- L171 `Err` : `"expected 'random.sample' argument 'obj' is list"`
- L173 `Err` : `"expected 'random.sample' argument 'count' is int"`
- L175 `Err` : `"'random.sample' argument 'count' cannot be greater than the length of 'obj'"`
- L193 `Err` : `"expected 'random.chance' argument 'probability' is float"`
- L195 `Err` : `"'probability' must be between 0 and 1"`
- L210 `Err` : `"expected 'random.seed' argument 'seed' is int"`
- L236 `Err` : `"expected 'random.string' argument 'length' is int"`
- L238 `Err` : `"'length' cannot be negative"`
- L255 `Err` : `"expected 'values' is list"`
- L258 `Err` : `"expected 'weights' is list"`
- L261 `Err` : `"'values' and 'weights' must have the same length"`
- L269 `Err` : `"weights must be float"`
- L272 `Err` : `"weights must be non-negative"`
- L279 `Err` : `"sum of weights must be greater than zero"`
- L305 `Err` : `"expected 'random.range' argument 'start' is int"`
- L317 `Err` : `"expected 'random.range' argument 'end' is int"`
- L324 `Err` : `"'start' cannot be greater than 'end'"`
- L343 `Err` : `"expected 'random.value' argument is enum"`
- L346 `Err` : `"expected 'random.value' argument is enum"`
- L36 `Err` : `"expected 'random.int' argument 'min' is int"`
- L38 `Err` : `"expected 'random.int' argument 'max' is int"`
- L40 `Err` : `"'min' cannot be greater than 'max'"`
- L61 `Err` : `"'min' and 'max' must be specified together"`
- L64 `Err` : `"expected 'random.float' argument 'min' is float"`
- L66 `Err` : `"expected 'random.float' argument 'max' is float"`
- L68 `Err` : `"'min' cannot be greater than 'max'"`
- L93 `Err` : `"expected 'random.bytes' argument 'length' is int"`
- L95 `Err` : `"'length' cannot be negative"`
- L115 `Err` : `"expected 'random.shuffle' argument to be a list"`
- L134 `Err` : `"expected 'random.choice' argument to be a list"`
- L150 `Err` : `"expected 'random.choices' argument 'obj' is list"`
- L152 `Err` : `"expected 'random.choices' argument 'count' is int"`
- L175 `Err` : `"expected 'random.sample' argument 'obj' is list"`
- L177 `Err` : `"expected 'random.sample' argument 'count' is int"`
- L179 `Err` : `"'random.sample' argument 'count' cannot be greater than the length of 'obj'"`
- L199 `Err` : `"expected 'random.chance' argument 'probability' is float"`
- L201 `Err` : `"'probability' must be between 0 and 1"`
- L218 `Err` : `"expected 'random.seed' argument 'seed' is int"`
- L255 `Err` : `"expected 'random.string' argument 'length' is int"`
- L257 `Err` : `"'length' cannot be negative"`
- L275 `Err` : `"expected 'values' is list"`
- L278 `Err` : `"expected 'weights' is list"`
- L281 `Err` : `"'values' and 'weights' must have the same length"`
- L289 `Err` : `"weights must be float"`
- L292 `Err` : `"weights must be non-negative"`
- L299 `Err` : `"sum of weights must be greater than zero"`
- L326 `Err` : `"expected 'random.range' argument 'start' is int"`
- L338 `Err` : `"expected 'random.range' argument 'end' is int"`
- L345 `Err` : `"'start' cannot be greater than 'end'"`
- L366 `Err` : `"expected 'random.value' argument is enum"`
- L369 `Err` : `"expected 'random.value' argument is enum"`

### `Range.cs`

- L55 `Panic` : `"step cannot be zero"`

### `Re.cs`

- L22 `Err` : `"expected value is string"`
- L56 `Err` : `$"invalid regex pattern: {e.Message}"`

### `Repeat.cs`

- L27 `Err` : `$"invalid '{Type}' initialize"`
- L32 `Err` : `"repeat is infinite"`
- L34 `Err` : `"repeat is infinite"`
- L38 `Err` : `"repeat is infinite"`

### `Reverse.cs`

- L23 `Err` : `$"invalid '{Type}' initialization"`

### `Runner.cs`

- L43 `Error` : `err?.Message ?? "propagated error", pf.Start, pf.Length, Context.Source, header: err?.Header ?? "error"`
- L47 `Error` : `"'break' outside loop", bf.Start, bf.Length, Context.Source`
- L51 `Error` : `"'skip' outside loop", sf.Start, sf.Length, Context.Source`
- L55 `Error` : `"'->' outside function", rf.Start, rf.Length, Context.Source`
- L107 `Panic` : `$"file '{path}' not found"`

### `Scope.cs`

- L46 `Err` : `$"variable '{key}' not found"`

### `Set.cs`

- L21 `Err` : `$"unsupported operand type(s) for +: 'set' and '{other.Type}'"`
- L27 `Err` : `$"unsupported operand type(s) for -: 'set' and '{other.Type}'"`
- L33 `Err` : `$"unsupported operand type(s) for ^: 'set' and '{other.Type}'"`
- L38 `Err` : `$"key {Str.To(key).Value} not found in set"`
- L105 `Err` : `"invalid argument: other"`
- L119 `Err` : `"invalid argument: other"`
- L133 `Err` : `"invalid argument: other"`

### `Str.cs`

- L28 `Err` : `$"too many arguments"`
- L41 `Err` : `$"cannot add '{other.Type}' to '{Type}'"`
- L54 `Err` : `$"cannot add '{other.Type}' to '{Type}'"`
- L63 `Err` : `$"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'"`
- L70 `Err` : `$"unsupported operand type(s) for ==: '{Type}' and '{other.Type}'"`
- L76 `Err` : `$"unsupported operand type(s) for <: '{Type}' and '{other.Type}'"`
- L81 `Err` : `"list index out of range"`
- L82 `Err` : `"invalid index type"`
- L91 `Err` : `"slice start must be int"`
- L96 `Err` : `"slice end must be int"`
- L101 `Err` : `"slice step must be int"`
- L102 `Err` : `"slice step cannot be zero"`
- L118 `Err` : `$"cannot convert '{Value}' to 'int'"`
- L120 `Err` : `$"cannot convert '{Value}' to 'float'"`
- L236 `Err` : `"invalid argument: value"`
- L251 `Err` : `"invalid argument: value"`
- L266 `Err` : `"invalid argument: value"`
- L281 `Err` : `"invalid argument: value"`
- L312 `Err` : `"invalid argument: value"`
- L328 `Err` : `"invalid argument: value"`
- L343 `Err` : `"invalid argument: values"`
- L354 `Err` : `$"cannot join '{item.Type}' with '{self.Type}'"`
- L375 `Err` : `"invalid argument: width"`
- L380 `Err` : `"invalid argument: fill"`
- L402 `Err` : `"invalid argument: width"`
- L407 `Err` : `"invalid argument: fill"`
- L427 `Err` : `"invalid argument: width"`
- L432 `Err` : `"invalid argument: fill"`
- L452 `Err` : `"invalid argument: old_value"`
- L454 `Err` : `"invalid argument: new_value"`
- L469 `Err` : `"invalid argument: sub_str"`

### `Stream.cs`

- L58 `Err` : `"stream is not readable"`
- L72 `Err` : `"stream is not readable"`
- L88 `Err` : `"stream is not writable"`
- L106 `Err` : `"stream is not writable"`
- L121 `Err` : `"stream is not writable"`
- L151 `Err` : `"stream is not seekable"`
- L166 `Err` : `"stream is not seekable"`
- L183 `Err` : `"stream is not seekable"`
- L198 `Err` : `"stream is not seekable"`
- L212 `Err` : `"stream is not readable"`

### `Stru.cs`

- L17 `Err` : `"invalid initialize"`
- L39 `Err` : `$"unsupported operand type(s) for +: '{Type}' and '{other.Type}'"`
- L88 `Err` : `$"equals operand is must be a boolean"`

### `Sys.cs`

- L65 `Err` : `"expected 'depth' to be int"`
- L66 `Err` : `"depth must be >= 1"`

### `Time.cs`

- L29 `Err` : `"expected 'time' argument to be of type 'int' or 'float' or convertible to int"`

### `Tup.cs`

- L97 `Err` : `$"equals operand is must be a boolean"`
- L105 `Err` : `"tuple index out of range"`
- L106 `Err` : `$"tuple has no attribute '{s.Value}'"`
- L107 `Err` : `"invalid index type"`
- L116 `Err` : `"slice start must be int"`
- L121 `Err` : `"slice end must be int"`
- L126 `Err` : `"slice step must be int"`
- L127 `Err` : `"slice step cannot be zero"`
- L149 `Err` : `$"equals operand is must be a boolean"`

### `UnType.cs`

- L64 `Panic` : `$"type not found: {name}"`

### `WebSocket.cs`

- L14 `Err` : `"'socket' cannot be constructed directly; use websocket.connect(url)"`
- L38 `Err` : `"expected text is string"`
- L49 `Err` : `e.Message`
- L81 `Err` : `e.Message`
- L101 `Err` : `e.Message`

### `Ws.cs`

- L16 `Err` : `"expected value is string"`
- L45 `Err` : `e.Message`

## generation summary

- registrations: 36
- native functions: 345
- error-site strings: 553
- parsed files: 85
