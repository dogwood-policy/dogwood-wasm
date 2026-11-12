#!/usr/bin/env bash
# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Check that src/ops.rs and src/error.rs still match dogwood-cli upstream.
#
# Both files are copies of dogwood-cli/src/{ops,error}.rs from
# dogwood-policy/dogwood, plus wasm-only additions. Those additions are
# stripped before diffing:
#   - `, tsify::Tsify` in derive lists
#   - `#[tsify(...)]` attribute lines
#   - blocks between `// wasm-only: begin` and `// wasm-only: end`
#   - this repository's two-line license header (and the blank line after it)
#     at the top of the file
# Anything else that differs is drift.
#
# Usage: scripts/check-cli-sync.sh [REF]
#   REF is an upstream tag, branch, or commit. Defaults to PINNED_REF below,
#   the release that Cargo.toml's dogwood-language dependency tracks. Bump
#   both together.
set -euo pipefail

PINNED_REF=v1.1
REF="${1:-$PINNED_REF}"
UPSTREAM="https://raw.githubusercontent.com/dogwood-policy/dogwood/${REF}/dogwood-cli/src"
FILES=(ops.rs error.rs)

root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

LICENSE_HEADER_1='// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.'
LICENSE_HEADER_2='// SPDX-License-Identifier: Apache-2.0'

strip_wasm_only() {
  awk -v h1="$LICENSE_HEADER_1" -v h2="$LICENSE_HEADER_2" '
    NR == 1 && $0 == h1 { header = 1; next }
    header && NR == 2 && $0 == h2 { next }
    header && NR == 3 && $0 == "" { next }
    { print }
  ' "$1" |
  sed -e '/^[[:space:]]*\/\/ wasm-only: begin$/,/^[[:space:]]*\/\/ wasm-only: end$/d' \
      -e '/^[[:space:]]*#\[tsify(.*)\][[:space:]]*$/d' \
      -e 's/, tsify::Tsify//'
}

# The pin must track the dogwood-language release actually locked, or the check
# compares against the wrong CLI.
if [[ "$REF" == "$PINNED_REF" ]]; then
  locked="$(awk '/^name = "dogwood-language"$/{getline; gsub(/version = |"/, ""); print}' "$root/Cargo.lock")"
  if [[ "$locked" != "${PINNED_REF#v}".* ]]; then
    echo "error: Cargo.lock has dogwood-language $locked but PINNED_REF is $PINNED_REF; bump them together" >&2
    exit 1
  fi
fi

status=0
for f in "${FILES[@]}"; do
  if ! curl -fsSL "$UPSTREAM/$f" -o "$tmp/upstream-$f"; then
    echo "error: could not fetch $UPSTREAM/$f" >&2
    exit 2
  fi
  strip_wasm_only "$root/src/$f" > "$tmp/local-$f"
  if diff -u --label "upstream@$REF/dogwood-cli/src/$f" --label "src/$f (wasm-only stripped)" \
      "$tmp/upstream-$f" "$tmp/local-$f"; then
    echo "ok: src/$f matches dogwood-cli@$REF"
  else
    echo "DRIFT: src/$f differs from dogwood-cli@$REF (diff above)" >&2
    status=1
  fi
done
exit $status
