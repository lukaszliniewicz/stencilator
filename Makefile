# Everyday commands. `make check` is what CI runs.

TARGET_DIR ?= $(or $(CARGO_TARGET_DIR),target)
WASM_CRATES := -p stencilator-core -p stencilator-wasm -p phase0 -p phase0-wasm

.PHONY: check fmt lint test deny unused fix wasm parity timing spike-wasm

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

# The browser build of the engine and its built-in fonts, into the app's source tree
# (ignored by git).
# Needs wasm-bindgen-cli at the same version as the wasm-bindgen crate, and binaryen.
WASM_PKG ?= app/src/engine/wasm

wasm:
	cargo build -p stencilator-wasm --target wasm32-unknown-unknown --profile wasm --locked
	wasm-bindgen --target web --out-dir $(WASM_PKG) $(TARGET_DIR)/wasm32-unknown-unknown/wasm/stencilator_wasm.wasm
	wasm-opt -Oz --strip-debug --strip-producers -o $(WASM_PKG)/stencilator_wasm_bg.wasm $(WASM_PKG)/stencilator_wasm_bg.wasm
	mkdir -p $(WASM_PKG)/fonts
	cp fonts/*.ttf $(WASM_PKG)/fonts/

# The browser build and the command line must write identical files.
parity: wasm
	cargo build -p stencilator-cli --locked
	node crates/stencilator-wasm/parity.mjs $(WASM_PKG) $(TARGET_DIR)/debug/stencilator .

# How long the browser build takes over free-form work.
timing: wasm
	node crates/stencilator-wasm/timing.mjs $(WASM_PKG) .

# Spike 1: the browser build, its size, and the comparison with native.
# Needs wasm-bindgen-cli 0.2.130, binaryen (wasm-opt), brotli and Node.
WASM_OUT ?= $(TARGET_DIR)/spike-wasm
WASM_FILE := $(WASM_OUT)/phase0_wasm_bg.wasm

spike-wasm:
	cargo build -p phase0-wasm --target wasm32-unknown-unknown --profile wasm --locked
	wasm-bindgen --target web --out-dir $(WASM_OUT) $(TARGET_DIR)/wasm32-unknown-unknown/wasm/phase0_wasm.wasm
	wasm-opt -Oz --strip-debug --strip-producers -o $(WASM_FILE) $(WASM_FILE)
	@echo "raw $$(wc -c < $(WASM_FILE)) bytes, gzip $$(gzip -9c $(WASM_FILE) | wc -c), brotli $$(brotli -cq 11 $(WASM_FILE) | wc -c)"
	node spikes/phase0-wasm/check.mjs $(WASM_OUT) .
