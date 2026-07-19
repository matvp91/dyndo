# Build from source

The [installer](../tutorial/install-cli.md) covers the `dyndo` CLI on macOS and
Linux, and [Docker](./deploy-with-docker.md) covers the server. Building from
source is the fallback for when neither fits — an unsupported platform, or
running the server without Docker.

## Prerequisites

- A recent stable [Rust toolchain](https://www.rust-lang.org/tools/install) with
  `cargo`. The minimum supported version is pinned in the repository's
  `rust-toolchain.toml`; any current stable release satisfies it.
- [`git`](https://git-scm.com/), to clone the repository.

## Build

Clone the repository and build in release mode:

```bash
git clone https://github.com/matvp91/dyndo.git
cd dyndo
cargo build --release
```

This produces both binaries under `target/release/`:

- `target/release/dyndo` — the CLI.
- `target/release/dyndo-server` — the server.

Run them straight from there, or put them on your `PATH`.

## Install the CLI on your PATH

`cargo install` copies the `dyndo` binary into `~/.cargo/bin`, which the Rust
installer already adds to your `PATH`:

```bash
cargo install --path crates/dyndo-cli
dyndo --version
```

You can now use every [CLI command](../reference/cli.md) — `index`, `dash`,
`hls` — exactly as the rest of this book describes.

## Run the server

The `dyndo-server` binary reads the same configuration as the Docker image
(built-in defaults, then `config.yaml`, then `DYNDO_*` environment variables).
Point it at a storage root and run it:

```bash
DYNDO_FS__ROOT=./assets ./target/release/dyndo-server
```

```text
dyndo-server listening on http://0.0.0.0:8080
```

Everything in [Run and configure the server](./run-the-server.md) applies — the
same config keys, request URLs, and `/health` endpoint. Only the way you launch
the process differs.

## Next steps

- Configure and operate the server:
  [Run and configure the server](./run-the-server.md).
- The full command-line surface: [dyndo CLI reference](../reference/cli.md).
- Prefer prebuilt binaries after all?
  [Install the CLI](../tutorial/install-cli.md).
