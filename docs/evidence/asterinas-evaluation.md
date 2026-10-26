# Asterinas local evaluation

Status: **unmodified upstream build and boot smoke verified locally**.
This is not hematite implementation, a full compatibility suite, or a Phase 0 exit pass.
Session date: 2026-10-25 America/Vancouver (2026-10-26 UTC).

## Inputs and provenance

- Source: <https://github.com/asterinas/asterinas>, shallow clone into
  `.work/asterinas` (ignored, disk-backed).
- Pinned observed HEAD: `26fffae2b7360809cd1a7883cf7e825c8290a056`.
- Commit timestamp: `2026-10-26T11:55:28+08:00`.
- Upstream toolchain: `nightly-2026-07-21`, from its `rust-toolchain.toml`.
- Upstream README development image: `asterinas/dev:0.19.0-20261012`.
  Download succeeded (exit 0); immutable image:
  `asterinas/dev@sha256:6a62f4eb57337e23733f3b40036bbbea5b022c972f21ed9557ad9a4437848306`.
  Docker reports 16,983,520,843 bytes uncompressed.
- Docker server observed: `29.9.1`. KVM device exists outside the sandbox.

Only upstream README, build configuration, and helper scripts were inspected
for evaluation setup. No Linux kernel implementation source was consulted;
no Asterinas implementation was copied into hematite.

## Commands and observations

Working directory: `/home/aaron/code/hematite`.

```sh
git ls-remote https://github.com/asterinas/asterinas.git HEAD
docker info --format '{{.ServerVersion}}'
ls -l /dev/kvm
git clone --depth 1 https://github.com/asterinas/asterinas.git .work/asterinas
git -C .work/asterinas show -s --format='%H%n%cI%n%s'
git -C .work/asterinas status --porcelain
docker pull asterinas/dev:0.19.0-20261012 > docs/evidence/asterinas-image-pull.log 2>&1
```

Initial sandbox checks failed: Git DNS resolution denied, Docker socket
access denied, `/dev/kvm` absent. Retrying with approved external execution
confirmed upstream access, Docker, and KVM; these were environment visibility
failures, not Asterinas failures. Clone succeeded and checkout was clean.
Image pull session `48065` completed with exit 0. The read-only disk-usage
query `27961` also completed with exit 0; no other project was cleaned up.
Image download raw output: [asterinas-image-pull.log](asterinas-image-pull.log).

## Planned bounded execution

After the image is available, inspect and record its immutable digest. Run the
unmodified checkout in a Docker container with 2 CPUs, 4 GiB memory,
`CARGO_BUILD_JOBS=2`, `make -j2`, and a 30-minute build timeout. Mount only
the evaluation checkout and its disk-backed scratch directory; do not use
upstream README's privileged/host-network/full-device mounts. Boot separately
with only `/dev/kvm`, 1 vCPU, 1 GiB guest RAM and a 120-second timeout.
Disable graphical display, network and optional host-sharing devices for the
initial smoke boot. A later networking evaluation needs an explicit isolated
configuration. Retain build failures and serial logs. These are planned bounds,
not executed commands or results.

## Measurement scope

Build and boot-smoke results are recorded below. No server workload, syscall
suite, filesystem durability, safety audit, or Linux differential result exists yet. Upstream
capability reports remain distinct from local observations in ADR 0001.

## Build attempt 01

Started container `hematite-asterinas-eval-20261025`; exec session `93650`.
Raw output: [asterinas-build-01.log](asterinas-build-01.log). Status: completed
with exit 0; exec session `93650` is terminal.
This attempt uses upstream defaults for GRUB/Multiboot2, not the proposed
hematite PVH/microvm profile. Its result will not prove that profile works.

Exact command (from repository root):

```sh
docker run --name hematite-asterinas-eval-20261025 \
  --cpus=2 --memory=4g --memory-swap=4g \
  --mount type=bind,src=/home/aaron/code/hematite/.work/asterinas,dst=/root/asterinas \
  --mount type=bind,src=/home/aaron/code/hematite/.work/tmp,dst=/scratch \
  -w /root/asterinas -e CARGO_BUILD_JOBS=2 -e TMPDIR=/scratch \
  -e 'NIX_CONFIG=max-jobs = 2
cores = 2' \
  asterinas/dev@sha256:6a62f4eb57337e23733f3b40036bbbea5b022c972f21ed9557ad9a4437848306 \
  timeout --kill-after=15s 1800s bash -lc \
  'set -eux; rustc --version; cargo --version; qemu-system-x86_64 --version; make -j2 kernel AUTO_TEST=boot MEM=1G SMP=1 NETDEV=none QEMU_DISPLAY=none' \
  > docs/evidence/asterinas-build-01.log 2>&1
```

Observed attempt-01 tool versions: Rust `1.99.0-nightly (87e5904f5 2026-07-20)`,
Cargo `1.99.0-nightly (3efb1f477 2026-07-17)`, QEMU `10.2.1`.
Upstream initramfs generation, OSDK installation and kernel compilation completed
successfully; GRUB rescue ISO generation also completed. The original checkout remains clean in Git.

Reproducibility limitation: upstream `make install_osdk` uses `cargo install
--path osdk` without `--locked`. The build log records fresh dependency
resolution. The checked-in OSDK lockfile is unchanged; no generated resolved
lockfile was located. Preserve the raw log and installed build artifacts;
do not claim source commit plus container digest alone reproduces this exact
OSDK dependency graph. A subsequent reproducibility run should use the native
Cargo `--locked` interface, with that command difference recorded explicitly.

Boot preparation: the build container has no KVM device. Run the resulting
GRUB ISO in a separate bounded container with `/dev/kvm`, using the upstream
`tools/qemu_args.sh normal` arguments and the ISO drive options from OSDK.
The ISO embeds the generated initramfs, avoiding dependence on the build
container's generated Nix-store paths. Preserve the expanded QEMU command.

## Boot attempt 01 — verified scope

The harness exited 0. QEMU exited **33**, upstream's debug-exit success code;
exactly one complete `Successfully booted.` line was observed and no timeout
occurred. The native `/init` script ran its shell workload
`sh -l /test/boot_hello.sh`; that script only prints the boot marker. This proves
that this initramfs can reach and execute that shell script, not general shell,
server or syscall compatibility. Init's setup commands are not independently
asserted by this smoke test.

Evidence: [result.json](asterinas-boot-01/result.json),
[console.log](asterinas-boot-01/console.log),
[serial log](asterinas-boot-01/qemu-serial.log), and
[expanded QEMU command](asterinas-boot-01/command.json).
QEMU emitted warnings about unsupported requested host CPU features; GRUB
printed `WARNING: no console will be available to OS`. The guest subsequently
produced serial/virtio-console output and completed the smoke test. Preserve
those warnings; do not infer that every requested Icelake feature was active.

ISO SHA-256: `d1c0e8cf8593bf741bf6edf6568c17b455b4501cca97c9477f59a41cc74b020f`.
Embedded initramfs SHA-256:
`521150414fdb9d9bce4918c18bbc1ef4b64626b351d9099167f5ecd9ff2ae330`.

Exact host invocation:

```sh
docker run --name hematite-asterinas-boot-01 --network=none \
  --cpus=2 --memory=2g --memory-swap=2g --device=/dev/kvm \
  --mount type=bind,src=/home/aaron/code/hematite/.work/asterinas,dst=/root/asterinas \
  --mount type=bind,src=/home/aaron/code/hematite/tools,dst=/evaluation,readonly \
  --mount type=bind,src=/home/aaron/code/hematite/docs/evidence,dst=/evidence \
  -w /root/asterinas \
  asterinas/dev@sha256:6a62f4eb57337e23733f3b40036bbbea5b022c972f21ed9557ad9a4437848306 \
  timeout --kill-after=10s 150s python3 /evaluation/run_asterinas_smoke.py \
  --checkout /root/asterinas \
  --iso /root/asterinas/target/osdk/asterinas-osdk-bin.iso \
  --output /evidence/asterinas-boot-01 \
  > docs/evidence/asterinas-boot-01-host.log 2>&1
```

The harness uses upstream's QEMU argument generator and documented OSDK ISO
attachment/exit conventions. It does not modify upstream kernel code. Both
build and boot containers have stopped; the checkout and artifacts remain on
disk for further evaluation. No background build or VM is required to keep
these results valid.
