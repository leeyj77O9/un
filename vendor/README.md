# vendor/ — patched sources for Windows/GNU

These copies are the `region`, `cranelift-jit` and `wasmtime-internal-jit-icache-coherence`
crates, rewritten with raw kernel32 FFI instead of the published versions. They exist so
`windows-sys` is **never** on the dependency list, because `windows-sys 0.52.0`
(pulled in by `region`) needs `dlltool.exe`, which the local GNU toolchain does not ship.

On a fully provisioned toolchain (MSVC Build Tools, or GNU binutils with `dlltool`) you
would be able to delete this directory and use `region`/`cranelift-jit` from crates.io
unchanged.

| crate | version | license | upstream |
|---|---|---|---|
| `region` | 3.0.2 | MIT | `Amanieu/region` — upstream sha `fc89b0c76d51bd13a591dca5fc9b9c8a0224aa18` (recorded in `region/.cargo_vcs_info.json`); `vendor/THIRD_PARTY.md` links the canonical repository |
| `cranelift-jit` | 0.137.1 | Apache-2.0 WITH LLVM-exception | Bytecode Alliance Cranelift (`bytecodealliance/wasmtime`, `cranelift-jit` crate) |
| `icache-coherence` (package `wasmtime-internal-jit-icache-coherence`) | 50.0.1 | Apache-2.0 WITH LLVM-exception | Bytecode Alliance wasmtime (`wasmtime-jit-icache-coherence`) |

Attribution summary: `region`'s upstream copyright and licence text are in
`region/LICENSE` (and `region/Cargo.toml.orig`). The two wasmtime crates ship in this directory with their upstream `LICENSE` files (the Apache-2.0 text with the LLVM exception, verbatim as published on crates.io and in the bytecodealliance repositories). The raw-FFI patches only remove the `windows-sys` dependency; no license grant is removed by the patches.
`THIRD_PARTY.md` in this directory summarises the notices.
