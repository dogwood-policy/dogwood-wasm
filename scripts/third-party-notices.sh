#!/usr/bin/env bash
# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Write THIRD-PARTY-NOTICES: attribution for every third-party crate compiled
# into the wasm module.
#
#   1. cargo-about renders, from Cargo.lock, the crate list and the full text
#      of each license, for the wasm32-unknown-unknown target (about.toml,
#      about.hbs). It runs offline and fails if any crate's license cannot be
#      read or is not in about.toml's accepted list.
#   2. Every NOTICE file those crates ship (NOTICE, NOTICE.md, NOTICES.md, ...)
#      is appended verbatim, as Apache-2.0 section 4(d) requires.
#
# Requires cargo-about (the version pinned in .github/workflows) and node.
#
# Usage: scripts/third-party-notices.sh [OUTPUT]   (default: THIRD-PARTY-NOTICES)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/THIRD-PARTY-NOTICES}"
cd "$root"

command -v cargo-about >/dev/null || { echo "error: cargo-about is not installed" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cargo about generate --locked --offline --fail about.hbs -o "$tmp/licenses.txt"

# The crates cargo-about listed, as "name version" lines.
awk '/^Crates included$/ { getline; inlist = 1; next }
     inlist && /^=+$/ { exit }
     inlist && NF >= 2 { print $1, $2 }' "$tmp/licenses.txt" > "$tmp/crates.txt"
[[ -s "$tmp/crates.txt" ]] || { echo "error: no crates found in cargo-about output" >&2; exit 1; }

cargo metadata --locked --offline --format-version 1 \
  --filter-platform wasm32-unknown-unknown > "$tmp/metadata.json"

# Append each listed crate's own NOTICE files, in crate order.
node - "$tmp/metadata.json" "$tmp/crates.txt" "$tmp/notices.txt" <<'EOF'
const fs = require("fs");
const path = require("path");
const [metadataFile, cratesFile, outFile] = process.argv.slice(2);
const packages = JSON.parse(fs.readFileSync(metadataFile, "utf8")).packages;
const byId = new Map(packages.map((p) => [`${p.name} ${p.version}`, p]));
let text = "";
for (const line of fs.readFileSync(cratesFile, "utf8").split("\n").filter(Boolean)) {
  const pkg = byId.get(line);
  if (!pkg) throw new Error(`crate ${line} listed by cargo-about is not in cargo metadata`);
  const dir = path.dirname(pkg.manifest_path);
  for (const file of fs.readdirSync(dir).sort()) {
    if (!/^notices?(\.[a-z]+)?$/i.test(file)) continue;
    text += "\n------------------------------------------------------------------------\n";
    text += `NOTICE file of ${line} (${file})\n`;
    text += "------------------------------------------------------------------------\n\n";
    text += fs.readFileSync(path.join(dir, file), "utf8").trimEnd() + "\n";
  }
}
fs.writeFileSync(outFile, text);
EOF

{
  cat "$tmp/licenses.txt"
  if [[ -s "$tmp/notices.txt" ]]; then
    printf '\n========================================================================\n'
    printf 'NOTICE files from third-party crates\n'
    printf '========================================================================\n'
    cat "$tmp/notices.txt"
  fi
} > "$out"

echo "ok: $(wc -l < "$tmp/crates.txt" | tr -d ' ') crates, $(grep -c '^NOTICE file of ' "$out" || true) NOTICE file(s) -> $out"
