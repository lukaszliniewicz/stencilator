# Everyday commands. `make check` is what CI runs.

WASM_CRATES := -p stencilator-core -p stencilator-wasm -p phase0 -p phase0-wasm

.PHONY: check fmt lint test deny unused fix spike-wasm

check: fmt lint test deny unused

fmt:
	cargo fmt --all --check

lint:
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo clippy $(WASM_CRATES) --target wasm32-unknown-unknown --locked -- -D warnings

test:
	cargo test --workspace --locked

deny:
	cargo deny check

unused:
	cargo machete

fix:
	cargo fmt --all
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Spike 1: the browser build, its size, and the comparison with native.
# Needs wasm-bindgen-cli 0.2.130, binaryen (wasm-opt), brotli and Node.
TARGET_DIR ?= $(or $(CARGO_TARGET_DIR),target)
WASM_OUT ?= $(TARGET_DIR)/spike-wasm
WASM_FILE := $(WASM_OUT)/phase0_wasm_bg.wasm

spike-wasm:
	cargo build -p phase0-wasm --target wasm32-unknown-unknown --profile wasm --locked
	wasm-bindgen --target web --out-dir $(WASM_OUT) $(TARGET_DIR)/wasm32-unknown-unknown/wasm/phase0_wasm.wasm
	wasm-opt -Oz --strip-debug --strip-producers -o $(WASM_FILE) $(WASM_FILE)
	@echo "raw $$(wc -c < $(WASM_FILE)) bytes, gzip $$(gzip -9c $(WASM_FILE) | wc -c), brotli $$(brotli -cq 11 $(WASM_FILE) | wc -c)"
	node spikes/phase0-wasm/check.mjs $(WASM_OUT) .
