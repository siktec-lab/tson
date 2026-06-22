#!/usr/bin/env bash
# bump-version.sh — bump TSON version across every manifest.
#
# Usage:  ./scripts/bump-version.sh 0.2.0
#
# Updates:
#   - Cargo.toml                       (crates.io)
#   - pyproject.toml                   (PyPI)
#   - js/package.json                  (npm: version + optionalDependencies)
#   - js/npm/<platform>/package.json   (npm per-platform packages, via `napi version`)
#
# After running, commit the changes and tag `v<version>` to trigger the
# Release workflow (.github/workflows/release.yml).

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "  e.g. $0 0.2.0"
    exit 1
fi

NEW_VER="$1"

# ── Cargo.toml (first `version =` only — the [package] one) ─────────
sed -i "0,/^version = \".*\"/s//version = \"$NEW_VER\"/" Cargo.toml
echo "  ✓ Cargo.toml → $NEW_VER"

# ── pyproject.toml ──────────────────────────────────────────────────
sed -i "0,/^version = \".*\"/s//version = \"$NEW_VER\"/" pyproject.toml
echo "  ✓ pyproject.toml → $NEW_VER"

# ── js/package.json: top-level version + optionalDependencies pins ──
# Only the FIRST "version" (top-level) — not the "version": "napi version"
# entry in the scripts block.
sed -i "0,/\"version\": \".*\"/s//\"version\": \"$NEW_VER\"/" js/package.json
# Pin each per-platform optionalDependency (scoped @siktec-lab/tson-*) to the
# new version.
sed -i "s/\(\"@siktec-lab\/tson-[a-z0-9-]*\": \)\"[^\"]*\"/\1\"$NEW_VER\"/g" js/package.json
echo "  ✓ js/package.json → $NEW_VER"

# ── js/npm/<platform>/package.json (regenerate version via napi) ────
if [ -d js/npm ]; then
    ( cd js && npx --no-install napi version 2>/dev/null ) \
        && echo "  ✓ js/npm/* → $NEW_VER (napi version)" \
        || echo "  ⚠ js/npm/* not updated (run 'cd js && npm install' then re-run, or 'napi version')"
fi

echo
echo "Done. Review the changes, then:"
echo "  git add -A && git commit -m \"Release v$NEW_VER\""
echo "  git tag v$NEW_VER && git push --follow-tags"
