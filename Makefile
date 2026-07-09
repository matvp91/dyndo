# dyndo — build & dev tasks
CARGO ?= cargo
BIN   := dyndo

.PHONY: build build-debug run test lint fmt fmt-check check install clean

## build: release build of the CLI -> target/release/dyndo
build:
	$(CARGO) build --release -p dyndo-cli

## build-debug: debug build of the CLI -> target/debug/dyndo
build-debug:
	$(CARGO) build -p dyndo-cli

## run: build, then package the sample assets -> assets/asset.json
run: build
	./target/release/$(BIN) \
		-i assets/index_video_avc_1080.mp4 \
		-i assets/index_video_avc_720.mp4 \
		-i assets/index_audio_aac_nl_2.mp4 \
		-o assets/asset.json

## test: run the whole workspace test suite
test:
	$(CARGO) test

## lint: clippy across all targets, warnings as errors
lint:
	$(CARGO) clippy --all-targets -- -D warnings

## fmt: format all crates
fmt:
	$(CARGO) fmt --all

## fmt-check: verify formatting without modifying
fmt-check:
	$(CARGO) fmt --all --check

## check: fast type-check of the workspace
check:
	$(CARGO) check --workspace

## install: install the dyndo CLI into ~/.cargo/bin
install:
	$(CARGO) install --path crates/dyndo-cli

## clean: remove build artifacts
clean:
	$(CARGO) clean
