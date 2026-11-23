#!/usr/bin/env bash
set -euo pipefail

version=v2026.11.10
case "$(uname -m)" in
  x86_64)
    arch=x64
    sha256=70a3cf7776ea97cfdae0b4c49ef348782c454b43b0bb77823cf029840c332946
    ;;
  aarch64 | arm64)
    arch=arm64
    sha256=af305e1efc58433a79b893738bfbf11b4b1662bdea9f0ea71f2ecab990058ad6
    ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/mise.tar.gz" \
  "https://github.com/jdx/mise/releases/download/${version}/mise-${version}-linux-${arch}.tar.gz"
echo "${sha256}  ${tmp}/mise.tar.gz" | sha256sum -c -
tar -xzf "$tmp/mise.tar.gz" -C "$tmp"
install -D "$tmp/mise/bin/mise" "${MISE_INSTALL_DIR:-$HOME/.local/bin}/mise"
