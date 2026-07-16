# Install the CLI

In this short lesson you'll install the prebuilt `dyndo` CLI and confirm it
runs — no Rust toolchain needed. Set aside two minutes.

Prebuilt binaries cover macOS (Apple Silicon and Intel) and Linux (x86_64).
On any other platform, build from source instead — [Getting
started](./getting-started.md) shows how.

## Step 1: Run the installer

```bash
curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash
```

The script detects your platform, downloads the latest release from GitHub,
verifies its checksum, and installs the `dyndo` binary into `~/.dyndo/bin`.
If that directory isn't on your `PATH` yet, it appends one line (marked
`# dyndo`) to your shell's rc file — `~/.zshrc`, `~/.bashrc`, or fish's
`config.fish` — and says so.

## Step 2: Verify it runs

Open a new terminal, then:

```bash
dyndo --version
```

You should see the name and version, e.g. `dyndo x.y.z`. That's it — the CLI
is installed.

## Pinning a version

The installer takes an optional version, with or without the leading `v`:

```bash
curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash -s <version>
```

Setting `DYNDO_VERSION=<version>` does the same. Available versions are
listed on the [releases page](https://github.com/matvp91/dyndo/releases).

## Uninstalling

Remove the install directory, and the `# dyndo` line from your shell's rc
file if you like:

```bash
rm -rf ~/.dyndo
```

## Where to next?

Follow [Getting started](./getting-started.md) to create CMAF sources, index
them, and play a stream. That tutorial builds from source because it also
uses `dyndo-server`, which this installer doesn't ship — for the server, see
[Deploy with Docker](../how-to/deploy-with-docker.md).
