RUST_LOG ?= tern=debug
LOG_FILE ?= /tmp/tern.log
JOBS ?= 2
BIN := target/dev-fast/tern
export RUSTFLAGS ?= -C link-arg=-fuse-ld=lld

.PHONY: zst run clean

zst:
	@mkdir -p target
	CARGO_BUILD_JOBS="$(JOBS)" makepkg --force

run:
	@cargo build --profile dev-fast && RUST_LOG="$(RUST_LOG)" "$(BIN)" 2>&1 | tee "$(LOG_FILE)"

# NOTE: do NOT remove the source `src/` directory here; it is Rust source, not a makepkg build dir.
clean:
	rm -rf pkg *.tar.zst
	cargo clean
