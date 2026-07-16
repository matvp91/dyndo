#!/usr/bin/env bash
# Install the prebuilt dyndo CLI from a GitHub release.
#
# Served at https://matvp91.github.io/dyndo/install.sh (the Docs workflow
# copies it into the Pages artifact), so users can run:
#
#   curl -fsSL https://matvp91.github.io/dyndo/install.sh | bash
#
# Installs the latest release into ~/.dyndo/bin and adds that directory to
# PATH via the shell's rc file. Pin a version with `bash -s <version>` or
# DYNDO_VERSION=<version>.
set -euo pipefail

REPO="matvp91/dyndo"
INSTALL_DIR="${HOME}/.dyndo/bin"
# Overridable so tests can serve tarballs from a local HTTP server.
DOWNLOAD_BASE="${DYNDO_DOWNLOAD_BASE:-https://github.com/${REPO}/releases/download}"

error() {
  echo "error: $*" >&2
  exit 1
}

# Map uname output to a release target triple; fail on anything we don't ship.
detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}-${arch}" in
    Darwin-arm64) echo "aarch64-apple-darwin" ;;
    Darwin-x86_64)
      # Apple Silicon under Rosetta reports x86_64; prefer the native build.
      # The sysctl OID doesn't exist on Intel Macs, so a failed read means
      # "not translated".
      if [[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" == "1" ]]; then
        echo "aarch64-apple-darwin"
      else
        echo "x86_64-apple-darwin"
      fi
      ;;
    Linux-x86_64) echo "x86_64-unknown-linux-gnu" ;;
    *) error "unsupported platform ${os}/${arch}; prebuilt binaries cover macOS (arm64, x86_64) and Linux (x86_64). Build from source: https://github.com/${REPO}" ;;
  esac
}

# Resolve the release tag: argument or DYNDO_VERSION (leading v optional),
# else whatever tag the GitHub releases/latest redirect lands on.
resolve_tag() {
  local requested="${1:-${DYNDO_VERSION:-}}"
  if [[ -n "$requested" ]]; then
    [[ "$requested" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]] \
      || error "requested version '${requested}' is not X.Y.Z"
    echo "v${requested#v}"
    return
  fi
  local url
  url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
    "https://github.com/${REPO}/releases/latest")" \
    || error "could not reach github.com to resolve the latest release"
  # https://github.com/<repo>/releases/tag/vX.Y.Z -> vX.Y.Z
  local tag="${url##*/}"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] \
    || error "could not parse a release tag from ${url}"
  echo "$tag"
}

# Put INSTALL_DIR on PATH via the shell's rc file, unless it already is.
# Idempotent: an rc file that mentions .dyndo/bin is left alone.
setup_path() {
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) return ;;
  esac

  local shell_name rc line
  shell_name="$(basename "${SHELL:-}")"
  # The rc lines are single-quoted on purpose: $HOME must land literally in
  # the rc file and expand when it is sourced, not now.
  # shellcheck disable=SC2016
  case "$shell_name" in
    zsh)  rc="${HOME}/.zshrc";  line='export PATH="$HOME/.dyndo/bin:$PATH"' ;;
    bash) rc="${HOME}/.bashrc"; line='export PATH="$HOME/.dyndo/bin:$PATH"' ;;
    fish) rc="${HOME}/.config/fish/config.fish"; line='set -gx PATH "$HOME/.dyndo/bin" $PATH' ;;
    *)
      echo "Add ${INSTALL_DIR} to your PATH to run dyndo by name."
      return
      ;;
  esac

  if [[ -f "$rc" ]] && grep -q '\.dyndo/bin' "$rc"; then
    echo "PATH entry already present in ${rc}; open a new shell if dyndo isn't found."
    return
  fi

  mkdir -p "$(dirname "$rc")"
  printf '\n# dyndo\n%s\n' "$line" >>"$rc"
  echo "Added ${INSTALL_DIR} to PATH in ${rc}; open a new shell to pick it up."
}

main() {
  command -v curl >/dev/null || error "curl is required"
  command -v tar >/dev/null || error "tar is required"

  # macOS ships shasum, Linux ships sha256sum; accept either.
  local sha256
  if command -v sha256sum >/dev/null; then
    sha256=(sha256sum)
  elif command -v shasum >/dev/null; then
    sha256=(shasum -a 256)
  else
    error "sha256sum or shasum is required"
  fi

  local target tag archive
  target="$(detect_target)"
  tag="$(resolve_tag "${1:-}")"
  archive="dyndo-${tag}-${target}.tar.gz"

  # Not `local`: the EXIT trap runs after main returns, when locals are gone
  # (and set -u would abort the trap).
  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  echo "Downloading dyndo ${tag} for ${target}..."
  curl -fsSL -o "${tmpdir}/${archive}" "${DOWNLOAD_BASE}/${tag}/${archive}" \
    || error "download failed: ${DOWNLOAD_BASE}/${tag}/${archive}"
  curl -fsSL -o "${tmpdir}/SHA256SUMS.txt" \
    "${DOWNLOAD_BASE}/${tag}/dyndo-${tag}-SHA256SUMS.txt" \
    || error "download failed: ${DOWNLOAD_BASE}/${tag}/dyndo-${tag}-SHA256SUMS.txt"

  # The sums file lists every tarball as `<hash>  ./<name>`; keep only our
  # line and let the sha tool verify it from inside the temp dir.
  grep " \./${archive}\$" "${tmpdir}/SHA256SUMS.txt" >"${tmpdir}/expected.sum" \
    || error "no checksum entry for ${archive} in the release's SHA256SUMS"
  (cd "$tmpdir" && "${sha256[@]}" -c expected.sum >/dev/null 2>&1) \
    || error "checksum verification failed for ${archive}"

  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir" dyndo
  mkdir -p "$INSTALL_DIR"
  install -m 755 "${tmpdir}/dyndo" "${INSTALL_DIR}/dyndo"
  echo "Installed ${INSTALL_DIR}/dyndo"

  setup_path

  echo
  "${INSTALL_DIR}/dyndo" --version
}

main "$@"
