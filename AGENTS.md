# Working on Stencilator

Notes for anyone changing this repository, people and coding agents alike.

- Work on a branch and open a pull request for review. Check CodeRabbit's findings
  before merging; merging, releasing and deploying need separate authorisation.
- Run `make check` before every commit. CI runs it, together with `make parity` and the
  app's checks, and all of them must pass.
- `internal/` is ignored by git and holds the maintainer's working notes, including the
  development plan. If `internal/plan.md` exists, read it before starting larger work.
- The engine, `crates/stencilator-core`, only computes. Its `clippy.toml` forbids file,
  network, environment and clock access, hash-ordered collections, and the standard
  library's transcendental float functions (use `libm`, which gives the same bits
  natively and in WebAssembly). The shells around it do all input and output.
- `crates/stencilator-core/tests/reference.rs` checks layouts, colour conversion,
  palettes, charts, resampling, PDFs, stencils and free-form plates against reference
  results in `fixtures/reference`, computed independently with NumPy, Pillow and trimesh. A change that
  alters any of them on purpose says so in its commit message.
- The app in `app/` reaches the engine only through `src/engine/client.ts`, and only
  the Web Worker loads the WebAssembly build; dependency-cruiser enforces both. Run
  `make wasm` before working on it, and `pnpm check` and `pnpm e2e` in `app/` before
  committing changes there. Every word the interface shows lives in
  `app/src/lib/text.ts`, so that a translation can follow.
- Error messages are written for the person using the tool, in British English.
