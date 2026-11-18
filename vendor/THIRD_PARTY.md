# Third-party notices

The `vendor/` directory redistributes patched copies of upstream crates.
Each patch only removes the `windows-sys` dependency and replaces its usage
with raw kernel32 FFI; no upstream license grant is removed.

| crate | version | license | upstream |
|---|---|---|---|
| `region` | 3.0.2 | MIT | <https://github.com/Amanieu/region> (crate `region` on crates.io); upstream revision `fc89b0c76d51bd13a591dca5fc9b9c8a0224aa18` (from `region/.cargo_vcs_info.json`) |
| `cranelift-jit` | 0.137.1 | Apache-2.0 WITH LLVM-exception | <https://github.com/bytecodealliance/wasmtime> (`cranelift-jit`) |
| `wasmtime-internal-jit-icache-coherence` | 50.0.1 | Apache-2.0 WITH LLVM-exception | <https://github.com/bytecodealliance/wasmtime> (`wasmtime-jit-icache-coherence`) |

`vendor/region/LICENSE` carries the MIT text. The two wasmtime crates
carry their upstream `LICENSE` files (the Apache-2.0 text with the LLVM
exception, verbatim as published on crates.io and in the bytecodealliance
repositories); their raw-FFI patches only remove the `windows-sys`
dependency and do not affect the license grant.
