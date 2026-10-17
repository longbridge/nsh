#!/usr/bin/env sh
# Installs NSH (the `nsh` CLI and its prebuilt runtimes) from a GitHub release.
#
#   curl -fsSL https://nsh.dev/install.sh | sh
#
# NSH_VERSION      a release tag such as v0.1.0 (default: the latest release)
# NSH_INSTALL_DIR  where the distribution goes (default: ~/.nsh); nsh finds
#                  its runtimes next to bin/nsh, so the directory stays whole
#
# `nsh` is linked into ~/.local/bin. Running the script again updates it.
set -eu

repository="longbridge/nsh"
dir="${NSH_INSTALL_DIR:-$HOME/.nsh}"
bin="$HOME/.local/bin"
version="${NSH_VERSION:-latest}"

fail() {
  echo "nsh: $*" >&2
  exit 1
}

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) platform="macos-arm64" ;;
  Linux/x86_64 | Linux/amd64) platform="linux-x86_64" ;;
  Darwin/*) fail "no prebuilt NSH for Intel Macs; build it from source: https://nsh.dev/docs/installation" ;;
  *) fail "no prebuilt NSH for $(uname -s) $(uname -m); see https://nsh.dev/docs/installation" ;;
esac

command -v curl > /dev/null 2>&1 || fail "curl is required"

asset="nsh-$platform.tar.gz"
if [ "$version" = "latest" ]; then
  url="https://github.com/$repository/releases/latest/download/$asset"
else
  url="https://github.com/$repository/releases/download/$version/$asset"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

echo "Downloading $url"
curl -fsSL "$url" -o "$work/$asset"
curl -fsSL "$url.sha256" -o "$work/$asset.sha256"

(
  cd "$work"
  if command -v shasum > /dev/null 2>&1; then
    shasum -a 256 -c "$asset.sha256" > /dev/null
  else
    sha256sum -c "$asset.sha256" > /dev/null
  fi
) || fail "the checksum of $asset does not match"

tar -xzf "$work/$asset" -C "$work"
[ -x "$work/nsh/bin/nsh" ] || fail "$asset holds no nsh/bin/nsh"

# Replace the whole directory: runtimes of an older release must not stay.
mkdir -p "$(dirname "$dir")" "$bin"
rm -rf "$dir.old"
[ ! -e "$dir" ] || mv "$dir" "$dir.old"
mv "$work/nsh" "$dir"
rm -rf "$dir.old"
ln -sf "$dir/bin/nsh" "$bin/nsh"

echo "Installed $("$dir/bin/nsh" --version) to $dir"
case ":$PATH:" in
  *":$bin:"*) ;;
  *) echo "Add $bin to your PATH: export PATH=\"$bin:\$PATH\"" ;;
esac
if [ "$platform" = "linux-x86_64" ]; then
  echo "The Linux runtime needs GTK 4.14 or newer (Ubuntu 24.04 and later)."
fi
