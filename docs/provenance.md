# Bootstrap provenance

Consulted 2026-10-25. All kernel sources here were written for this project;
no Linux implementation or Asterinas kernel code was copied or translated.
Substantial AI assistance and independent AI review are disclosed in commits.

| Area | Sources and use |
| --- | --- |
| PVH entry and ELF note | [PVH public ABI](https://xenbits.xen.org/docs/unstable/misc/pvh.html), [Xen public ELF-note definitions](https://github.com/xen-project/xen/blob/master/xen/include/public/elfnote.h): entry register/mode contract and note number 18; no implementation copied |
| x86 bootstrap/paging | [Intel SDM index](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html), Volume 3 architecture constraints; applied register/descriptor design independently reviewed in [review 0001](reviews/0001-bootstrap.md) |
| Virtual machine | [QEMU microvm documentation](https://www.qemu.org/docs/master/system/i386/microvm.html); actual command, firmware hash and package version preserved in local evidence |
| UART | [TI TL16C550C datasheet, revision I](https://www.ti.com/lit/ds/symlink/tl16c550c.pdf), standard compatible register layout and polling; COM1 address is part of the fixed QEMU bring-up profile |
| Rust safety | [Rust Reference extern statics](https://doc.rust-lang.org/reference/items/external-blocks.html#statics): mutable hardware-owned page tables; compiler `forbid(unsafe_code)` on policy crate |
| Host tooling | Python standard library, Cargo, GNU Make, Docker, QEMU; no third-party Python packages |

## ARM64 extension

[Design 0002](design/0002-arm64-bootstrap.md) links the consulted Arm register,
translation, PL011 and semihosting specifications, and the QEMU virt platform
contract. [Review 0002](reviews/0002-arm64-bootstrap.md) records the independent
source review and fixes. `boot-aarch64.S` and `aarch64.rs` are original code;
no Linux or third-party kernel implementation was copied. Fixed semihosting
is used only to exit the guest test, never to expose host-file operations.

## Dependency inventory

`Cargo.lock` contains only `hm-core` and `hm-kernel`; there are no external
kernel crates. Stable Rust `1.99.0` supplies `core` and compiler-builtins for
`aarch64-unknown-none` and `x86_64-unknown-none`; no nightly features, custom target or build-std patch.
Those compiler/runtime components remain trusted tooling, not audited original
kernel code. `hm-core` is the only project crate permitted to use unsafe code.

The Dockerfile pins the official Rust image digest and an immutable Debian
package snapshot. Rustup verifies the target/component downloads against the
pinned toolchain distribution manifests. QEMU and its firmware are host test
inputs, not linked kernel code. The original kernel license remains pending
ADR 0002; this project does not relicense its compiler, emulator, or firmware.
No public release is authorized. Name checks and restrictions remain in ADR 0003.
