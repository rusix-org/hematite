ARCH ?= aarch64
ifeq ($(ARCH),aarch64)
QEMU := qemu-system-aarch64
else ifeq ($(ARCH),x86_64)
QEMU := qemu-system-x86_64
else
$(error Unsupported ARCH '$(ARCH)'; use aarch64 or x86_64)
endif
TARGET := $(ARCH)-unknown-none

.PHONY: image build test test-all run

image:
	docker build --tag hematite-build:phase0 .

build: image
	bash tools/dev.sh cargo build --locked --offline -p hm-core --release --target $(TARGET) --target-dir target/$(ARCH)/monitor

test: image
	bash tools/dev.sh bash -eu -c '\
		evidence_dir="target/evidence/$$(date -u +%Y%m%dT%H%M%S-%N)-$(ARCH)"; \
		mkdir -p "$$evidence_dir"; \
		{ rustc --version; cargo --version; $(QEMU) --version; \
			if [ "$(ARCH)" = x86_64 ]; then sha256sum /usr/share/qemu/bios-microvm.bin; \
			else printf "Boot: direct ELF; no external firmware.\n"; fi; \
		} > "$$evidence_dir/environment.log"; \
		cargo fmt --all -- --check; \
		cargo clippy --workspace --all-features --locked --offline --target $(TARGET) --target-dir target/$(ARCH)/clippy -- -D warnings; \
		python3 tools/test_test_kernel.py; \
		python3 tools/test_timer_trace.py; \
		python3 tools/test_test_preemption.py; \
		python3 tools/test_test_preemption_negative.py; \
		python3 tools/test_preemption_boundaries.py; \
		python3 tools/test_test_wait_blocked.py; \
		python3 tools/test_test_secondary.py; \
		python3 tools/test_test_smp.py; \
		python3 tools/test_test_elf.py; \
		python3 tools/test_test_space.py; \
		python3 tools/check_repository.py; \
		python3 tools/check_safety.py; \
		cargo build --locked --offline -p hm-core --release --features boot-tests --target $(TARGET) --target-dir target/$(ARCH)/boot-test; \
		cargo build --locked --offline -p hm-core --release --target $(TARGET) --target-dir target/$(ARCH)/monitor; \
		if [ "$(ARCH)" = aarch64 ]; then \
			cargo build --locked --offline -p hm-core --release --features unhandled-fault-test --target $(TARGET) --target-dir target/$(ARCH)/fault-test; \
			cargo build --locked --offline -p hm-core --release --features nested-fault-test --target $(TARGET) --target-dir target/$(ARCH)/nested-test; \
			cargo build --locked --offline -p hm-core --release --features thread-overflow-test --target $(TARGET) --target-dir target/$(ARCH)/thread-overflow-test; \
			cargo build --locked --offline -p hm-core --release --features preemption-tests --target $(TARGET) --target-dir target/$(ARCH)/preemption-test; \
			cargo build --locked --offline -p hm-core --release --features preemption-boundary-test --target $(TARGET) --target-dir target/$(ARCH)/preemption-boundary-test; \
			cargo build --locked --offline -p hm-core --release --features preemption-no-timer-test --target $(TARGET) --target-dir target/$(ARCH)/preemption-no-timer-test; \
			cargo build --locked --offline -p hm-core --release --features preemption-overflow-test --target $(TARGET) --target-dir target/$(ARCH)/preemption-overflow-test; \
			cargo build --locked --offline -p hm-core --release --features wait-all-blocked-test --target $(TARGET) --target-dir target/$(ARCH)/wait-all-blocked-test; \
			cargo build --locked --offline -p hm-core --release --features space-tests --target $(TARGET) --target-dir target/$(ARCH)/space-test; \
			cargo build --locked --offline -p hm-core --release --features secondary-tests --target $(TARGET) --target-dir target/$(ARCH)/secondary-test; \
			cargo build --locked --offline -p hm-core --release --features secondary-stale-generation-test --target $(TARGET) --target-dir target/$(ARCH)/secondary-stale-test; \
			cargo build --locked --offline -p hm-core --release --features secondary-cpu-off-return-test --target $(TARGET) --target-dir target/$(ARCH)/secondary-off-return-test; \
			cargo build --locked --offline -p hm-core --release --features smp-tests --target $(TARGET) --target-dir target/$(ARCH)/smp-test; \
			cargo build --locked --offline -p hm-core --release --features smp-no-peer-timer-test --target $(TARGET) --target-dir target/$(ARCH)/smp-no-peer-timer-test; \
			cargo build --locked --offline -p hm-core --release --features smp-peer-overflow-test --target $(TARGET) --target-dir target/$(ARCH)/smp-peer-overflow-test; \
			cargo build --locked --offline -p hm-core --release --features smp-all-blocked-test --target $(TARGET) --target-dir target/$(ARCH)/smp-all-blocked-test; \
		fi; \
		cargo build --locked --offline -p hm-kernel --release --target $(TARGET) --target-dir target/$(ARCH)/boot-test; \
		python3 tools/check_capability_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/type-boundary"; \
		python3 tools/check_elf_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/elf-types"; \
		python3 tools/check_frame_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/frame-types"; \
		mutex_host_args=(); \
		if [ "$(ARCH)" = aarch64 ]; then \
			cargo build --locked --offline -p hm-kernel --release --target x86_64-unknown-linux-gnu --target-dir target/host-mutex; \
			python3 tools/check_elf.py --library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib --output "$$evidence_dir/elf-host"; \
			python3 tools/check_cpu_topology.py --library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib --output "$$evidence_dir/cpu-topology"; \
			mutex_host_args=(--host-library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib); \
		fi; \
		python3 tools/check_mutex_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/mutex-types" "$${mutex_host_args[@]}"; \
		if [ "$(ARCH)" = aarch64 ]; then \
			python3 tools/check_space_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/space-types"; \
			python3 tools/check_mapping_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/mapping-types"; \
			python3 tools/check_thread_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/thread-types"; \
		fi; \
		python3 tools/test_kernel.py \
			--kernel target/$(ARCH)/boot-test/$(TARGET)/release/hematite \
			--output "$$evidence_dir/boot" --accel tcg --arch $(ARCH); \
		python3 tools/test_boot_profiles.py \
			--test-kernel target/$(ARCH)/boot-test/$(TARGET)/release/hematite \
			--monitor-kernel target/$(ARCH)/monitor/$(TARGET)/release/hematite \
			--fault-kernel target/$(ARCH)/fault-test/$(TARGET)/release/hematite \
			--nested-kernel target/$(ARCH)/nested-test/$(TARGET)/release/hematite \
			--thread-overflow-kernel target/$(ARCH)/thread-overflow-test/$(TARGET)/release/hematite \
			--output "$$evidence_dir/profiles" --arch $(ARCH); \
		if [ "$(ARCH)" = aarch64 ]; then \
			python3 tools/test_preemption.py --kernel target/$(ARCH)/preemption-test/$(TARGET)/release/hematite --output "$$evidence_dir/preemption-128m"; \
			python3 tools/test_preemption.py --kernel target/$(ARCH)/preemption-test/$(TARGET)/release/hematite --memory 64 --output "$$evidence_dir/preemption-64m"; \
			python3 tools/test_preemption.py --kernel target/$(ARCH)/preemption-boundary-test/$(TARGET)/release/hematite --boundaries --output "$$evidence_dir/preemption-boundaries"; \
			python3 tools/test_preemption_negative.py --no-timer-kernel target/$(ARCH)/preemption-no-timer-test/$(TARGET)/release/hematite --overflow-kernel target/$(ARCH)/preemption-overflow-test/$(TARGET)/release/hematite --output "$$evidence_dir/preemption-negative"; \
			python3 tools/test_wait_blocked.py --kernel target/$(ARCH)/wait-all-blocked-test/$(TARGET)/release/hematite --output "$$evidence_dir/wait-all-blocked"; \
			python3 tools/test_space.py --kernel target/$(ARCH)/space-test/$(TARGET)/release/hematite --output "$$evidence_dir/space"; \
			python3 tools/test_secondary.py --kernel target/$(ARCH)/secondary-test/$(TARGET)/release/hematite --stale-generation-kernel target/$(ARCH)/secondary-stale-test/$(TARGET)/release/hematite --cpu-off-return-kernel target/$(ARCH)/secondary-off-return-test/$(TARGET)/release/hematite --output "$$evidence_dir/secondary"; \
			python3 tools/test_smp.py --kernel target/$(ARCH)/smp-test/$(TARGET)/release/hematite --no-peer-timer-kernel target/$(ARCH)/smp-no-peer-timer-test/$(TARGET)/release/hematite --peer-overflow-kernel target/$(ARCH)/smp-peer-overflow-test/$(TARGET)/release/hematite --all-blocked-kernel target/$(ARCH)/smp-all-blocked-test/$(TARGET)/release/hematite --output "$$evidence_dir/smp"; \
			python3 tools/check_timer_trace.py --trace "$$evidence_dir/boot/exceptions.log" \
				--expected 16 --output "$$evidence_dir/boot/timer-trace.json"; \
			python3 tools/check_timer_trace.py --trace "$$evidence_dir/profiles/monitor/exceptions.log" \
				--expected 32 --output "$$evidence_dir/profiles/monitor/timer-trace.json"; \
			python3 tools/check_timer_trace.py --trace "$$evidence_dir/profiles/memory-64m/exceptions.log" \
				--expected 32 --output "$$evidence_dir/profiles/memory-64m/timer-trace.json"; \
		fi'

test-all:
	$(MAKE) test ARCH=aarch64
	$(MAKE) test ARCH=x86_64

run: build
	bash tools/dev.sh --interactive bash tools/run.sh $(ARCH)
