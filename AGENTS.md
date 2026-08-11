# AGENTS.md

Repository-specific guidance for contributors and coding agents. Keep this file
short and actionable; put user-facing architecture and usage documentation in
`README.md` or `docs/` instead.

## Repository map

- `crates/dyndo-core`: CMAF parsing, indexing, packaging, and image extraction.
- `crates/dyndo-dash` and `crates/dyndo-hls`: manifest generation.
- `crates/dyndo-cli`: the `dyndo` CLI.
- `crates/dyndo-server`: the Axum HTTP server.
- `docs/`: mdBook source.

Make behavior changes in the owning crate and add or update its tests.

## Validation

Run the narrowest relevant check while iterating. Before handoff, run the
applicable checks below:

```sh
make fmt-check  # required; uses the pinned nightly formatter
make lint       # Clippy on all targets; warnings are errors
make test       # workspace tests
make doc        # rustdoc; warnings are errors
```

Use `make check` for a fast workspace type-check. For Dockerfile changes, also
run `docker build -t dyndo-server .` when a local Docker daemon is available.

## Invariants

- Rust is pinned in `rust-toolchain.toml`. When upgrading it, update the
  `rust-version` in `Cargo.toml` and the Rust builder image in `Dockerfile` in
  the same change.
- `rustfmt.toml` requires the nightly pinned by `Makefile` (`NIGHTLY`); do not
  substitute the stable formatter.
- `rsmpeg` links to system FFmpeg. CI and Docker must use the same FFmpeg
  version and configure options: update `Dockerfile` and
  `.github/actions/setup-ffmpeg/action.yml` together.
- FFmpeg is built as shared libraries only. The Docker runtime stage must copy
  those libraries, run `ldconfig`, and exclude FFmpeg programs and build tools.
- Update `Cargo.lock` whenever dependency resolution changes.

## Change rules

- Preserve unrelated working-tree changes.
- Keep CMAF fixtures minimal. CI rejects `.mp4` fixtures over 4 KiB in
  `crates/dyndo-core/tests/fixtures`.
- When changing user-facing behavior, update the relevant `README.md` or
  `docs/` page. Build the book with `make book` when modifying `docs/`; use the
  mdBook version pinned in `.github/workflows/docs.yml` when CI parity matters.
