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
| `region` | 3.0.2 | MIT | `rustc-dev-guide` / region-rs — upstream sha `fc89b0c76d51bd13a591dca5fc9b9c8a0224aa18` (recorded in `region/.cargo_vcs_info.json`) |
| `cranelift-jit` | 0.137.1 | Apache-2.0 WITH LLVM-exception | Bytecode Alliance Cranelift (`bytecodealliance/wasmtime`, `cranelift-jit` crate) |
| `icache-coherence` (package `wasmtime-internal-jit-icache-coherence`) | 50.0.1 | Apache-2.0 WITH LLVM-exception | Bytecode Alliance wasmtime (`wasmtime-jit-icache-coherence`) |

Attribution summary: `region`'s upstream copyright and licence text are in
`region/LICENSE` (and `region/Cargo.toml.orig`). The two wasmtime crates
ship in this directory **without their upstream `LICENSE` files**; the
upstream texts are Apache-2.0 WITH LLVM-exception, available from the
Bytecode Alliance repositories and from every version published on
crates.io, and no license grant is removed by the raw-FFI patches.
`THIRD_PARTY.md` in this directory summarises the notices.
