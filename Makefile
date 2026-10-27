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
	bash tools/dev.sh bash -eu -c 'if [ "$(ARCH)" = aarch64 ]; then python3 tools/build_apps.py --output "target/apps-build/$$(date -u +%Y%m%dT%H%M%S-%N)"; fi; cargo build --locked --offline -p hm-core --release --target $(TARGET) --target-dir target/$(ARCH)/monitor'

test: image
	bash tools/dev.sh bash -eu -c '\
		unset HM_TASK_NEGATIVE HM_IDLE_PROFILE; \
		evidence_dir="target/evidence/$$(date -u +%Y%m%dT%H%M%S-%N)-$(ARCH)"; \
		mkdir -p "$$evidence_dir"; \
		task_negative_cases="$$(python3 tools/build_task_negative_cases.py --list-profiles)"; \
		test -n "$$task_negative_cases"; \
		printf "%s\n" "$$task_negative_cases" > "$$evidence_dir/task-negative-profiles.txt"; \
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
		python3 tools/test_test_rw_copy.py; \
		python3 tools/test_test_idle.py; \
		python3 tools/test_test_native.py; \
		python3 tools/test_test_interactive_wire.py; \
		python3 tools/test_test_input_policy.py; \
		python3 tools/test_test_startup_input.py; \
		python3 tools/test_test_task_wire.py; \
		python3 tools/test_test_task_admission.py; \
		python3 tools/test_test_process_budget.py; \
		python3 tools/test_test_process.py; \
		python3 tools/test_test_initramfs.py; \
		python3 tools/test_test_boot_image.py; \
		python3 tools/test_test_launcher.py; \
		python3 tools/test_test_launch_input.py; \
		python3 tools/test_test_startup_delivery.py; \
		python3 tools/test_test_group_launcher.py; \
		python3 tools/test_test_task_launcher.py; \
		python3 tools/test_test_task_cases.py; \
		python3 tools/test_test_task_negative.py; \
		python3 tools/test_test_runtime.py; \
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
			cargo build --locked --offline -p hm-core --release --features rw-copy-tests --target $(TARGET) --target-dir target/$(ARCH)/rw-copy-test; \
			HM_IDLE_PROFILE=short cargo build --locked --offline -p hm-core --release --features idle-tests --target $(TARGET) --target-dir target/$(ARCH)/idle-short-test; \
			HM_IDLE_PROFILE=wall cargo build --locked --offline -p hm-core --release --features idle-tests --target $(TARGET) --target-dir target/$(ARCH)/idle-wall-test; \
			cargo build --locked --offline -p hm-core --release --features process-tests --target $(TARGET) --target-dir target/$(ARCH)/process-test; \
			cargo build --locked --offline -p hm-core --release --features startup-input-tests --target $(TARGET) --target-dir target/$(ARCH)/startup-input-test; \
			cargo build --locked --offline -p hm-core --release --features runtime-tests --target $(TARGET) --target-dir target/$(ARCH)/runtime-test; \
			cargo build --locked --offline -p hm-core --release --features task-tests --target $(TARGET) --target-dir target/$(ARCH)/task-test; \
			for task_case in $$task_negative_cases; do \
				HM_TASK_NEGATIVE="$$task_case" cargo build --locked --offline -p hm-core --release --features task-negative-tests --target $(TARGET) --target-dir "target/$(ARCH)/task-negative-$$task_case"; \
			done; \
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
		python3 tools/check_native_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/native-types"; \
		python3 tools/check_startup_input_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/startup-input-types"; \
		python3 tools/check_input_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/input-types"; \
		python3 tools/check_task_wire_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/task-wire-types"; \
		python3 tools/check_task_admission_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/task-admission-types"; \
		python3 tools/check_process_budget_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/process-budget-types"; \
		python3 tools/check_boot_image_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/boot-image-types"; \
		python3 tools/check_initramfs_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/initramfs-types"; \
		python3 tools/check_elf_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/elf-types"; \
		python3 tools/check_frame_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/frame-types"; \
		mutex_host_args=(); \
		if [ "$(ARCH)" = aarch64 ]; then \
			cargo build --locked --offline -p hm-kernel --release --target x86_64-unknown-linux-gnu --target-dir target/host-mutex; \
			python3 tools/check_native.py --output "$$evidence_dir/native-host"; \
			cargo clippy -p hm-kernel --test startup_input --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-startup-input -- -D warnings; \
			python3 tools/check_startup_input.py --output "$$evidence_dir/startup-input-host"; \
			cargo clippy -p hm-kernel --test interactive_wire --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-interactive-wire -- -D warnings; \
			python3 tools/check_interactive_wire.py --output "$$evidence_dir/interactive-wire-host"; \
			cargo clippy -p hm-kernel --test input_policy --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-input-policy -- -D warnings; \
			python3 tools/check_input_policy.py --output "$$evidence_dir/input-policy-host"; \
			cargo clippy -p hm-kernel --test task_wire --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-task-wire -- -D warnings; \
			python3 tools/check_task_wire.py --output "$$evidence_dir/task-wire-host"; \
			cargo clippy -p hm-kernel --test task_admission --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-task-admission -- -D warnings; \
			python3 tools/check_task_admission.py --output "$$evidence_dir/task-admission-host"; \
			cargo clippy -p hm-kernel --test process_budget --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-process-budget -- -D warnings; \
			python3 tools/check_process_budget.py --output "$$evidence_dir/process-budget-host"; \
			python3 tools/build_apps.py --output "$$evidence_dir/apps-fixtures"; \
			python3 tools/build_startup_probes.py --output "$$evidence_dir/startup-input-fixtures"; \
			python3 tools/build_runtime.py --output "$$evidence_dir/runtime-fixtures"; \
			cargo clippy -p hm-kernel --test launch_input --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-launch-input -- -D warnings; \
			python3 tools/check_launch_input.py --output "$$evidence_dir/launch-input-host"; \
			python3 tools/build_initramfs.py --output "$$evidence_dir/initramfs-fixture"; \
			python3 tools/build_initramfs.py --process --output "$$evidence_dir/process-archive"; \
			cargo clippy -p hm-kernel --test boot_image --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-boot-image -- -D warnings; \
			python3 tools/check_boot_image.py --output "$$evidence_dir/boot-image-host"; \
			cargo clippy -p hm-kernel --test initramfs --locked --offline --target x86_64-unknown-linux-gnu --target-dir target/host-initramfs -- -D warnings; \
			python3 tools/check_initramfs.py --reference "$$evidence_dir/initramfs-fixture/gnu-reference.cpio" --output "$$evidence_dir/initramfs-host"; \
			python3 tools/build_process_fixtures.py --output "$$evidence_dir/process-fixtures"; \
			python3 tools/build_rw_copy.py --output "$$evidence_dir/rw-copy-fixture"; \
			python3 tools/check_elf.py --library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib --output "$$evidence_dir/elf-host"; \
			python3 tools/check_cpu_topology.py --library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib --output "$$evidence_dir/cpu-topology"; \
			mutex_host_args=(--host-library target/host-mutex/x86_64-unknown-linux-gnu/release/libhm_kernel.rlib); \
		fi; \
		python3 tools/check_mutex_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/mutex-types" "$${mutex_host_args[@]}"; \
		if [ "$(ARCH)" = aarch64 ]; then \
			python3 tools/check_process_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/process-types"; \
			python3 tools/check_runtime_layout.py --library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib --output "$$evidence_dir/runtime-layout"; \
			python3 tools/build_task_cases.py --layout "$$evidence_dir/runtime-layout/result.json" --output "$$evidence_dir/task-case-fixtures"; \
			python3 tools/check_runtime_layout.py --task-negative --library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib --output "$$evidence_dir/task-negative-layout"; \
			python3 tools/build_task_negative_cases.py --layout "$$evidence_dir/task-negative-layout/result.json" --output "$$evidence_dir/task-negative-fixtures"; \
			python3 tools/check_space_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/space-types"; \
			python3 tools/check_rw_copy_types.py \
				--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
				--target $(TARGET) --output "$$evidence_dir/rw-copy-types"; \
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
			python3 tools/test_process.py --kernel target/$(ARCH)/process-test/$(TARGET)/release/hematite --fixtures crates/hm-core/fixtures/process/manifest.json --archive crates/hm-core/fixtures/process-initramfs.cpio --output "$$evidence_dir/process"; \
			python3 tools/test_startup_delivery.py --kernel target/$(ARCH)/startup-input-test/$(TARGET)/release/hematite --fixtures crates/hm-core/fixtures/startup-input/manifest.json --archive crates/hm-core/fixtures/startup-input-initramfs.cpio --output "$$evidence_dir/startup-delivery"; \
			python3 tools/test_launcher.py --kernel target/$(ARCH)/monitor/$(TARGET)/release/hematite --apps crates/hm-core/fixtures/apps/manifest.json --archive crates/hm-core/fixtures/apps-initramfs.cpio --output "$$evidence_dir/launcher"; \
			python3 tools/test_group_launcher.py --kernel target/$(ARCH)/monitor/$(TARGET)/release/hematite --apps crates/hm-core/fixtures/apps/manifest.json --archive crates/hm-core/fixtures/apps-initramfs.cpio --output "$$evidence_dir/group-launcher"; \
			python3 tools/test_task_launcher.py --kernel target/$(ARCH)/monitor/$(TARGET)/release/hematite --apps crates/hm-core/fixtures/apps/manifest.json --archive crates/hm-core/fixtures/apps-initramfs.cpio --output "$$evidence_dir/task-launcher"; \
			python3 tools/test_runtime.py --kernel target/$(ARCH)/runtime-test/$(TARGET)/release/hematite --fixtures crates/hm-core/fixtures/runtime/manifest.json --archive crates/hm-core/fixtures/runtime-initramfs.cpio --layout "$$evidence_dir/runtime-layout/result.json" --output "$$evidence_dir/runtime"; \
			python3 tools/test_task_cases.py --kernel target/$(ARCH)/task-test/$(TARGET)/release/hematite --fixtures crates/hm-core/fixtures/task-cases/manifest.json --archive crates/hm-core/fixtures/task-cases-initramfs.cpio --large-archive crates/hm-core/fixtures/task-cases-large-initramfs.cpio --layout "$$evidence_dir/runtime-layout/result.json" --output "$$evidence_dir/task-cases"; \
			for task_case in $$task_negative_cases; do \
				python3 tools/test_task_negative.py --case "$$task_case" --kernel "target/$(ARCH)/task-negative-$$task_case/$(TARGET)/release/hematite" --fixtures crates/hm-core/fixtures/task-negative-cases/manifest.json --archive crates/hm-core/fixtures/task-negative-initramfs.cpio --layout "$$evidence_dir/task-negative-layout/result.json" --output "$$evidence_dir/task-negative-$$task_case"; \
			done; \
			python3 tools/test_space.py --kernel target/$(ARCH)/space-test/$(TARGET)/release/hematite --output "$$evidence_dir/space"; \
			python3 tools/test_rw_copy.py --kernel target/$(ARCH)/rw-copy-test/$(TARGET)/release/hematite --output "$$evidence_dir/rw-copy"; \
			python3 tools/test_idle.py --kernel target/$(ARCH)/idle-short-test/$(TARGET)/release/hematite --wall-kernel target/$(ARCH)/idle-wall-test/$(TARGET)/release/hematite --apps crates/hm-core/fixtures/apps/manifest.json --archive crates/hm-core/fixtures/apps-initramfs.cpio --output "$$evidence_dir/idle"; \
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
