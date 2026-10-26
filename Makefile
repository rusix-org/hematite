.PHONY: image build test run

image:
	docker build --tag hematite-build:phase0 .

build: image
	bash tools/dev.sh cargo build --locked --offline -p hm-core --release --target-dir target/monitor

test: image
	bash tools/dev.sh bash -eu -c '\
		evidence_dir="target/evidence/$$(date -u +%Y%m%dT%H%M%S-%N)"; \
		mkdir -p "$$evidence_dir"; \
		{ rustc --version; cargo --version; qemu-system-x86_64 --version; sha256sum /usr/share/qemu/bios-microvm.bin; } > "$$evidence_dir/environment.log"; \
		cargo fmt --all -- --check; \
		cargo clippy --workspace --all-features --locked --offline -- -D warnings; \
		python3 tools/test_test_kernel.py; \
		python3 tools/check_repository.py; \
		python3 tools/check_safety.py; \
		cargo build --locked --offline -p hm-core --release --features boot-tests --target-dir target/boot-test; \
		cargo build --locked --offline -p hm-core --release --target-dir target/monitor; \
		python3 tools/test_kernel.py \
			--kernel target/boot-test/x86_64-unknown-none/release/hematite \
			--output "$$evidence_dir/boot" --accel tcg; \
		python3 tools/test_boot_profiles.py \
			--test-kernel target/boot-test/x86_64-unknown-none/release/hematite \
			--monitor-kernel target/monitor/x86_64-unknown-none/release/hematite \
			--output "$$evidence_dir/profiles"'

run: build
	bash tools/dev.sh --interactive bash tools/run.sh
