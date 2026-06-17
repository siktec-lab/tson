#!/usr/bin/env bash
# bump-version.sh — bump TSON version across all manifests
#
# Usage:  ./scripts/bump-version.sh 0.2.0
#   This updates: Cargo.toml, pyproject.toml, js/package.json

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "  e.g. $0 0.2.0"
    exit 1
fi

NEW_VER="$1"

# ── Cargo.toml ──────────────────────────────────────────────────────
sed -i "s/^version = \".*\"/version = \"$NEW_VER\"/" Cargo.toml
echo "  ✓ Cargo.toml → $NEW_VER"

# ── pyproject.toml ──────────────────────────────────────────────────
sed -i "s/^version = \".*\"/version = \"$NEW_VER\"/" pyproject.toml
echo "  ✓ pyproject.toml → $NEW_VER"

# ── js/package.json ─────────────────────────────────────────────────
sed -i "s/\"version\": \".*\"/\"version\": \"$NEW_VER\"/" js/package.json
echo "  ✓ js/package.json → $NEW_VER"

echo
echo "Done. Review the changes, then:"
echo "  git add Cargo.toml pyproject.toml js/package.json"
echo "  git commit -m \"Bump version to $NEW_VER\""
echo "  git push"
