# rust_inventory.ps1 — generate docs/rust-migration/NATIVE_INVENTORY.md
#
# reads-only analysis: parse the C# reference and write the native module/type
# registrations, function signatures, and error strings into one doc.
# the C# source is never modified.
#
# usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File rust\scripts\rust_inventory.ps1

$ErrorActionPreference = 'Stop'
$repo = 'C:\dev\un'

function Get-Balance($text, $start) {
    $depth = 0
    $i = $start
    $inStr = $false
    while ($i -lt $text.Length) {
        $c = $text[$i]
        if ($inStr) {
            if ($c -eq '\') { $i += 2; continue }
            if ($c -eq '"') { $inStr = $false }
            $i++; continue
        }
        if ($c -eq '"') { $inStr = $true; $i++; continue }
        if ($c -eq '(' -or $c -eq '[' -or $c -eq '{') { $depth++; $i++; continue }
        if ($c -eq ')' -or $c -eq ']' -or $c -eq '}') {
            $depth--
            if ($depth -eq 0) { return $i }
            $i++; continue
        }
        $i++
    }
    return -1
}

function Clean($v) {
    $v = $v.Trim().TrimStart('"').TrimEnd('"').TrimEnd(',').Trim().Trim('"')
    return $v
}

function Split-TopLevel($s) {
    $parts = @()
    $cur = ''
    $depth = 0
    $inStr = $false
    for ($i = 0; $i -lt $s.Length; $i++) {
        $c = $s[$i]
        if ($inStr) { $cur += $c; if ($c -eq '"') { $inStr = $false }; continue }
        if ($c -eq '"') { $inStr = $true; $cur += $c; continue }
        if ($c -eq '(' -or $c -eq '[' -or $c -eq '{') { $depth++; $cur += $c; continue }
        if ($c -eq ')' -or $c -eq ']' -or $c -eq '}') { $depth--; $cur += $c; continue }
        if ($c -eq ',' -and $depth -eq 0) { $parts += $cur; $cur = ''; continue }
        $cur += $c
    }
    if ($cur.Trim().Length -gt 0) { $parts += $cur }
    return $parts
}

function Esc($v) { return [regex]::Replace($v, '\|', '\|') }

# the doc commentary lives here so regeneration keeps it.
$head = @(
    '# NATIVE_INVENTORY.md - native standard library inventory',
    '',
    '`rust/scripts/rust_inventory.ps1` generates this file by parsing `src/**/*.cs`',
    '(the C# reference, read only). Regenerating refreshes the tables and the string',
    'inventory; this commentary block is kept in the script so it stays in sync.',
    '',
    '## 1. registrations',
    '',
    'Reference registration uses four attributes and `Global.Init` collects them by assembly',
    'reflection. Rust must register the same names with the same meaning, because the',
    'module cache, circular import rules, module aliases, and the `inspect` module',
    '(`globals`, `members`, `attribute`) all read this table.',
    '',
    '- `BuiltinModule` : global builtin functions (not a module)',
    '- `NativeModule`  : modules loaded with `use <name>`',
    '- `BuiltinType`   : methods of the language builtin types',
    '- `NativeType`    : methods of native types',
    '',
    '`NativeModule("x", typeof(T))` attaches a type to a module: `flow` gives `pool`/`lock`,',
    '`iter` gives `range`/`counter`/`reverse`/`repeat`, `time` gives `stopwatch`,',
    '`random` gives `random`, `http` gives `http_client`, `ws` gives `web_socket`.',
    '',
    '## 2. call conventions',
    '',
    '`[ArgInfo]` fields (reproduce verbatim):',
    '',
    '| field | meaning |',
    '|---|---|',
    '| `Essential = true` | required argument; the missing-argument path reports `invalid argument: {name}` |',
    '| `Optional = true`  | default `= null!`; the argument arrives as `none` and the native checks it itself |',
    '| `Positional = true`| positional spread argument (`*args`) |',
    '| `Keyword = true`   | keyword argument collection (`**kw`) |',
    '| `Name = "x"`       | can differ from the C# parameter name; `inspect` and the error strings use this name |',
    '| `[Self]`           | injects the receiver, a different path from bound `fn` methods |',
    '',
    'Natives take `Obj` and return `Obj`. Failure returns `new Err(msg)`.',
    '`src/Core/Native/` has no `Panic`: `throw` appears only in `Global.cs`, `Runner.cs`,',
    '`Context.cs`, `Optimizer.cs`, `Evaluator.cs`, `UnType.cs` (Rust: `Payload::Panic`).',
    '',
    'Sites that lift a host exception into `Err` through `e.Message` (`http`, `os`, `ws`,',
    '`re`, `HttpClient`, `WebSocket`) must reproduce the .NET exception text. `Re` wraps it',
    'as `invalid regex pattern: {e.Message}`.',
    '',
    '## 3. function tables',
    '',
    '`arg types` is the `ArgumentTypes = new[] { ... }` list and `C# params` is the signature',
    'parameter declarations with their `[ArgInfo]`/`[Self]` attributes. `ret` is the',
    '`ReturnType` string: it is display metadata for `inspect`, not a VM type check.',
    '',
    '## 4. typos are verbatim targets',
    '',
    'Reference string typos reach stdout/stderr, so copy them exactly:',
    '',
    '- `List.cs` L573: `invalid argumnets: index`',
    '- `Dict.cs` L22 / `Iter.cs` L17: `invaild ''{Type}'' initialize`',
    '',
    '## 5. sandbox',
    '',
    '`PlaygroundRunner.DefaultAllowed` is the module allow-list for the memory filesystem',
    'sandbox: `io`, `math`, `iter`, `re`, `random`, `time`, `inspect`, `builtin`, `sys`.',
    'The Rust playground runner must make the same allow/deny decision and fail with',
    '`module ''{name}'' is not allowed in this environment` for a denied module.'
)

$files = Get-ChildItem -Path "$repo\src" -Recurse -Filter '*.cs' | Sort-Object FullName
$registrations = @()
$functions = @()
$strings = @()

foreach ($f in $files) {
    $text = [System.IO.File]::ReadAllText($f.FullName)
    $file = $f.Name

    foreach ($m in [regex]::Matches($text, '\[(BuiltinModule|NativeModule|BuiltinType|NativeType)\s*\(\s*(?:Name\s*=\s*)?"([^"]*)"' )) {
        $registrations += ,@($file, $m.Groups[1].Value, $m.Groups[2].Value)
    }

    foreach ($m in [regex]::Matches($text, '\[Native\s*\(' )) {
        $open = $m.Index + $m.Value.Length - 1
        $close = Get-Balance $text $open
        if ($close -lt 0) { continue }
        $body = $text.Substring($open + 1, $close - $open - 1)

        $name = ''; $ret = ''; $argt = ''; $desc = ''; $ex = ''
        foreach ($field in @('Name', 'ReturnType', 'Description', 'Example')) {
            $mm = [regex]::Match($body, "$field\s*=\s*([^\r\n]*)", 'Singleline')
            if ($mm.Success) {
                $v = Clean $mm.Groups[1].Value
                if ($field -eq 'Name') { $name = $v }
                if ($field -eq 'ReturnType') { $ret = $v }
                if ($field -eq 'Description') { $desc = $v }
                if ($field -eq 'Example') { $ex = $v }
            }
        }
        $am = [regex]::Match($body, 'ArgumentTypes\s*=\s*new\[\]\s*\{([^}]*)\}', 'Singleline')
        if ($am.Success) { $argt = ($am.Groups[1].Value -split ',' | ForEach-Object { Clean $_ }) -join ' ' }

        $rest = $text.Substring($close + 1)
        $sig = ''; $params = ''
        $sigMatch = [regex]::Match($rest, '^\s*\]\s*(?:\[[^\]]*\]\s*)*(public|internal|private)\s+(static\s+)?(async\s+)?([A-Za-z0-9_<>,\?\[\]]+(?:\s+[A-Za-z0-9_<>,\?\[\]]+)*?)\s+(\w+)\s*\(', 'Singleline')
        if ($sigMatch.Success) {
            $sig = $sigMatch.Groups[5].Value
            $parenStart = $sigMatch.Index + $sigMatch.Value.LastIndexOf('(')
            $parenClose = Get-Balance $rest $parenStart
            $raw = $rest.Substring($parenStart + 1, $parenClose - $parenStart - 1)
            $parts = @()
            foreach ($p in (Split-TopLevel $raw)) {
                $r = $p.Trim()
                if ($r.Length -eq 0) { continue }
                $info = ''; $self = ''
                foreach ($a in [regex]::Matches($r, '\[(ArgInfo|Self)(?:\s*\(([^()]*)\))?\]')) {
                    if ($a.Groups[1].Value -eq 'Self') { $self = ' SELF' }
                    # keep the ArgInfo text verbatim: `Clean` would eat the closing
                    # quote of `Name = "old_value"`
                    else { $info += ($a.Groups[2].Value -replace '\s+', ' ').Trim() + ' ' }
                }
                $pn = ([regex]::Replace($r, '\[(ArgInfo|Self)(\s*\([^()]*\))?\]', '') -replace '\s+', ' ').Trim()
                $parts += "$pn{$info$self}"
            }
            $params = ($parts -join ' ; ')
        }

        $functions += ,@($file, $name, $ret, $argt, $sig, $params, $desc, $ex)
    }

    foreach ($m in [regex]::Matches($text, 'new\s+(Err|Panic|Error)\s*\(')) {
        $open = $m.Index + $m.Value.Length - 1
        $close = Get-Balance $text $open
        if ($close -lt 0) { continue }
        $body = ($text.Substring($open + 1, $close - $open - 1) -replace '\s+', ' ').Trim()
        $line = ($text.Substring(0, $m.Index) -split "`n").Count
        $strings += ,@($file, $line, $m.Groups[1].Value, $body)
    }
}

# --------------------------------------------------------------------------
# group by file
# --------------------------------------------------------------------------

$regByFile = @{}
$fnByFile = @{}
$errByFile = @{}

foreach ($r in $registrations) {
    $file = $r[0]
    if (-not $regByFile[$file]) { $regByFile[$file] = @() }
    $regByFile[$file] += ,$r
}

foreach ($fn in $functions) {
    $file = $fn[0]
    if (-not $fnByFile[$file]) { $fnByFile[$file] = @() }
    $fnByFile[$file] += ,$fn
}

foreach ($s in $strings) {
    $file = $s[0]
    if (-not $errByFile[$file]) { $errByFile[$file] = @() }
    $errByFile[$file] += ,$s
}

$sb = New-Object System.Text.StringBuilder
foreach ($l in $head) { [void]$sb.AppendLine($l) }
[void]$sb.AppendLine('')

# --------------------------------------------------------------------------
# registration table
# --------------------------------------------------------------------------

[void]$sb.AppendLine('| kind | name | file |')
[void]$sb.AppendLine('|---|---|---|')

$regObjs = $registrations | ForEach-Object {
    [pscustomobject]@{ kind = $_[1]; name = $_[2]; file = $_[0] }
}

foreach ($r in ($regObjs | Sort-Object kind, name, file)) {
    [void]$sb.AppendLine("| ``$($r.kind)`` | ``$($r.name)`` | ``$($r.file)`` |")
}

[void]$sb.AppendLine('')
[void]$sb.AppendLine((($registrations | Group-Object { $_[1] } | Sort-Object Name |
    ForEach-Object { "$($_.Name) x$($_.Count)" }) -join ', '))
[void]$sb.AppendLine('')

# --------------------------------------------------------------------------
# function tables, grouped by source file
# --------------------------------------------------------------------------

foreach ($file in ($fnByFile.Keys | Sort-Object)) {
    $label = 'no registration attribute'
    if ($regByFile[$file]) {
        $parts = @()
        foreach ($r in $regByFile[$file]) { $parts += ($r[1] + ': ' + $r[2]) }
        $label = ($parts -join ', ')
    }
    [void]$sb.AppendLine("### ``$file`` -- $label")
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('| fn | ret | arg types | C# params | desc | example |')
    [void]$sb.AppendLine('|---|---|---|---|---|---|')
    foreach ($fn in $fnByFile[$file]) {
        $name = Esc $fn[1]
        $ret = Esc $fn[2]
        $argt = Esc $fn[3]
        $params = Esc $fn[5]
        $desc = Esc $fn[6]
        $ex = Esc $fn[7]
        [void]$sb.AppendLine("| ``$name`` | ``$ret`` | ``$argt`` | ``$params`` | $desc | ``$ex`` |")
    }
    [void]$sb.AppendLine('')
}

# --------------------------------------------------------------------------
# error strings, grouped by source file, in source order
# --------------------------------------------------------------------------

[void]$sb.AppendLine('## 6. error string inventory')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('Every `new Err(...)` / `new Panic(...)` / `new Error(...)` argument in `src` is')
[void]$sb.AppendLine('kept verbatim. `L{n}` is the line number inside the file.')
[void]$sb.AppendLine('')

foreach ($file in ($errByFile.Keys | Sort-Object)) {
    [void]$sb.AppendLine("### ``$file``")
    [void]$sb.AppendLine('')
    foreach ($s in $errByFile[$file]) {
        [void]$sb.AppendLine("- L$($s[1]) ``$($s[2])`` : ``$(Esc $s[3])``")
    }
    [void]$sb.AppendLine('')
}

[void]$sb.AppendLine('## generation summary')
[void]$sb.AppendLine('')
[void]$sb.AppendLine("- registrations: $($registrations.Count)")
[void]$sb.AppendLine("- native functions: $($functions.Count)")
[void]$sb.AppendLine("- error-site strings: $($strings.Count)")
[void]$sb.AppendLine("- parsed files: $($files.Count)")

$out = "$repo\docs\rust-migration\NATIVE_INVENTORY.md"
[System.IO.File]::WriteAllText($out, $sb.ToString(), (New-Object System.Text.UTF8Encoding $false))

Write-Output ("registrations=" + $registrations.Count)
Write-Output ("functions=" + $functions.Count)
Write-Output ("strings=" + $strings.Count)
Write-Output ("files=" + $files.Count)
Write-Output ("lines=" + (Get-Content $out).Count)
