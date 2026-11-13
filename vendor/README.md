# vendor/ — patched sources for Windows/GNU

These copies are the `region`, `cranelift-jit` and `wasmtime-internal-jit-icache-coherence`
crates, rewritten with raw kernel32 FFI instead of the published versions. They exist so
`windows-sys` is **never** on the dependency list, because `windows-sys 0.52.0`
(pulled in by `region`) needs `dlltool.exe`, which the local GNU toolchain does not ship.

On a fully provisioned toolchain (MSVC Build Tools, or GNU binutils with `dlltool`) you
would be able to delete this directory and use `region`/`cranelift-jit` from crates.io
unchanged.

| crate | version | reason |
|---|---|---|
| `region` | 3.0.2 | VirtualAlloc/VirtualProtect on Windows and the `linux`/`macos`/bsd mmap paths on Unix. Removed the `windows-sys` dependency. |
| `cranelift-jit` | 0.137.1 | JIT runtime support. Replaced just the `windows_sys::GetModuleHandleA` / `GetProcAddress` usage with raw FFI. |
| `icache-coherence` (package `wasmtime-internal-jit-icache-coherence`) | 50.0.1 | `FlushInstructionCache` on Windows. Raw FFI. |

Licenses of the upstream crates remain in their directories.
