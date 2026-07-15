# dyndo — build & dev tasks
CARGO   ?= cargo
BIN     := dyndo
# Pinned nightly for rustfmt (rustfmt.toml uses the nightly-only `group_imports`).
# Dated so formatting is reproducible across machines and CI; bump deliberately.
NIGHTLY ?= nightly-2026-07-09

.PHONY: build build-debug run test lint fmt fmt-check check doc install clean

## build: release build of the CLI -> target/release/dyndo
build:
	$(CARGO) build --release -p dyndo-cli

## build-debug: debug build of the CLI -> target/debug/dyndo
build-debug:
	$(CARGO) build -p dyndo-cli

## run: run the dyndo-server
run:
	$(CARGO) run -p dyndo-server

## test: run the whole workspace test suite
test:
	$(CARGO) test

## lint: clippy across all targets, warnings as errors
lint:
	$(CARGO) clippy --all-targets -- -D warnings

## fmt: format all crates (pinned nightly rustfmt — required for import grouping)
fmt:
	$(CARGO) +$(NIGHTLY) fmt --all

## fmt-check: verify formatting without modifying (pinned nightly rustfmt)
fmt-check:
	$(CARGO) +$(NIGHTLY) fmt --all --check

## check: fast type-check of the workspace
check:
	$(CARGO) check --workspace

## doc: build workspace docs, warnings as errors
doc:
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps --workspace

## install: install the dyndo CLI into ~/.cargo/bin
install:
	$(CARGO) install --path crates/dyndo-cli

## clean: remove build artifacts
clean:
	$(CARGO) clean
