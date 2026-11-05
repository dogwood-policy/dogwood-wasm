#!/usr/bin/env bash
# Check the wasm-pack output in pkg/ against the release tag and finish it for
# npm. Run by .github/workflows/release.yml after the release build and tests,
# immediately before `npm publish`; safe to run locally on a release build.
#
# Fails (non-zero) unless:
#   - the package is named @dogwood-policy/dogwood-wasm
#   - its version equals the tag (without the leading `v`) and Cargo.toml's
#   - repository.url is this GitHub repository (npm provenance requires it)
#   - the build has no panic hook (BSC15: release builds must not log panics)
# Then copies NOTICE into the package, as Apache-2.0 section 4(d) requires.
#
# Usage: scripts/prepare-release.sh vX.Y.Z
set -euo pipefail

TAG="${1:?usage: $0 vX.Y.Z}"
NAME="@dogwood-policy/dogwood-wasm"
REPO="https://github.com/dogwood-policy/dogwood-wasm"

root="$(cd "$(dirname "$0")/.." && pwd)"
pkg="$root/pkg"
[[ -f "$pkg/package.json" ]] || { echo "error: $pkg/package.json not found; build first" >&2; exit 1; }

fail=0
err() { echo "error: $*" >&2; fail=1; }

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || err "tag $TAG is not vMAJOR.MINOR.PATCH[-PRERELEASE]"
want="${TAG#v}"

field() { node -p "const p=require('$pkg/package.json'); String(($1) ?? '')"; }
name="$(field p.name)"
version="$(field p.version)"
repo="$(field 'p.repository && p.repository.url')"
cargo_version="$(awk -F'"' '/^version = /{print $2; exit}' "$root/Cargo.toml")"

[[ "$name" == "$NAME" ]] || err "package name is '$name', expected '$NAME' (build with --scope dogwood-policy)"
[[ "$version" == "$want" ]] || err "package version $version does not match tag $TAG"
[[ "$cargo_version" == "$want" ]] || err "Cargo.toml version $cargo_version does not match tag $TAG"
repo_norm="${repo#git+}"; repo_norm="${repo_norm%.git}"
[[ "$repo_norm" == "$REPO" ]] || err "repository.url is '$repo', expected '$REPO'"

# console_error_panic_hook imports console.error and Error.prototype.stack;
# nothing else in the bindings does.
if grep -qE '__wbg_(error|stack)_' "$pkg/dogwood_wasm.js"; then
  err "pkg/ includes the panic hook; release builds must not enable the 'dev' feature"
fi

(( fail == 0 )) || exit 1

cp "$root/NOTICE" "$pkg/NOTICE"
node -e "
const fs = require('fs');
const f = '$pkg/package.json';
const p = JSON.parse(fs.readFileSync(f, 'utf8'));
if (!p.files.includes('NOTICE')) p.files.push('NOTICE');
fs.writeFileSync(f, JSON.stringify(p, null, 2) + '\n');
"

echo "ok: $NAME@$version from $REPO"
(cd "$pkg" && npm pack --dry-run 2>&1 | sed -n '/Tarball Contents/,/Tarball Details/p')
