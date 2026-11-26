# Working on Stencilator

Notes for anyone changing this repository, people and coding agents alike.

- Run `make check` before every commit. It is what CI runs, and it must pass.
- `internal/` is ignored by git and holds the maintainer's working notes, including the
  development plan. If `internal/plan.md` exists, read it before starting larger work.
- The engine, `crates/stencilator-core`, only computes. Its `clippy.toml` forbids file,
  network, environment and clock access, hash-ordered collections, and the standard
  library's transcendental float functions (use `libm`, which gives the same bits
  natively and in WebAssembly). The shells around it do all input and output.
- `crates/stencilator-core/tests/reference.rs` checks layouts, colour conversion,
  palettes, charts, resampling, PDFs and stencils against reference results in
  `fixtures/reference`, computed independently with NumPy and Pillow. A change that
  alters any of them on purpose says so in its commit message.
- Error messages are written for the person using the tool, in British English.
