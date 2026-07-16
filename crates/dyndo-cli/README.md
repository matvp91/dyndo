# dyndo-cli

The `dyndo` command-line tool — the indexing, subtitle-packing, and
offline-manifest entry point for [`dyndo`](../../README.md). It is thin wiring:
argument parsing plus calls into [`dyndo-core`](../dyndo-core/README.md).

Full documentation lives in the book: the
**[dyndo CLI reference](https://matvp91.github.io/dyndo/reference/cli.html)**
covers every command (`index`, `dash`, `hls`, `pack`), option, and default, and
the [how-to guides](https://matvp91.github.io/dyndo/how-to/index-sources.html)
walk through the tasks they serve.

## Install

```bash
cargo install --path .        # or, from the repo root: make install
```
