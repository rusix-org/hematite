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
		python3 tools/check_repository.py; \
		python3 tools/check_safety.py; \
		cargo build --locked --offline -p hm-core --release --features boot-tests --target $(TARGET) --target-dir target/$(ARCH)/boot-test; \
		cargo build --locked --offline -p hm-core --release --target $(TARGET) --target-dir target/$(ARCH)/monitor; \
		if [ "$(ARCH)" = aarch64 ]; then \
			cargo build --locked --offline -p hm-core --release --features unhandled-fault-test --target $(TARGET) --target-dir target/$(ARCH)/fault-test; \
			cargo build --locked --offline -p hm-core --release --features nested-fault-test --target $(TARGET) --target-dir target/$(ARCH)/nested-test; \
		fi; \
		cargo build --locked --offline -p hm-kernel --release --target $(TARGET) --target-dir target/$(ARCH)/boot-test; \
		python3 tools/check_capability_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/type-boundary"; \
		python3 tools/check_frame_types.py \
			--library target/$(ARCH)/boot-test/$(TARGET)/release/libhm_kernel.rlib \
			--target $(TARGET) --output "$$evidence_dir/frame-types"; \
		python3 tools/test_kernel.py \
			--kernel target/$(ARCH)/boot-test/$(TARGET)/release/hematite \
			--output "$$evidence_dir/boot" --accel tcg --arch $(ARCH); \
		python3 tools/test_boot_profiles.py \
			--test-kernel target/$(ARCH)/boot-test/$(TARGET)/release/hematite \
			--monitor-kernel target/$(ARCH)/monitor/$(TARGET)/release/hematite \
			--fault-kernel target/$(ARCH)/fault-test/$(TARGET)/release/hematite \
			--nested-kernel target/$(ARCH)/nested-test/$(TARGET)/release/hematite \
			--output "$$evidence_dir/profiles" --arch $(ARCH)'

test-all:
	$(MAKE) test ARCH=aarch64
	$(MAKE) test ARCH=x86_64

run: build
	bash tools/dev.sh --interactive bash tools/run.sh $(ARCH)
