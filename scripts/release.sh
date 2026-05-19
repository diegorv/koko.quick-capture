#!/usr/bin/env bash
set -euo pipefail

# ─── Config ───────────────────────────────────────────────────────────
# quick-capture tags are `vX.Y.Z` with no pre-release suffix (the brain
# fork ships `-alpha`; this fork ships stable point releases).
SUFFIX=""
TAG_PREFIX="v"
CARGO_PKG_NAME="quick-capture"

# ─── Helpers ──────────────────────────────────────────────────────────
usage() {
  echo "Usage: $0 [patch|minor|major]"
  echo ""
  echo "Bump types:"
  echo "  patch  (default)  Correções e ajustes pequenos"
  echo "                    0.3.0 → 0.3.1 → 0.3.2 → 0.3.3 ..."
  echo ""
  echo "  minor             Features novas (reseta patch pra 0)"
  echo "                    0.3.2 → 0.4.0 → 0.5.0 → 0.6.0 ..."
  echo ""
  echo "  major             Breaking changes (reseta minor e patch pra 0)"
  echo "                    0.6.0 → 1.0.0 → 2.0.0 → 3.0.0 ..."
  echo ""
  echo "O prefixo '${TAG_PREFIX}' é adicionado automaticamente."
  echo "Todas as tags são annotated com changelog dos commits desde a última tag."
  echo ""
  echo "Exemplos:"
  echo "  $0              # v0.8.0 → v0.8.1"
  echo "  $0 minor        # v0.8.1 → v0.9.0"
  echo "  $0 major        # v0.9.0 → v1.0.0"
  exit 1
}

# ─── Parse bump type ─────────────────────────────────────────────────
BUMP="${1:-patch}"
case "$BUMP" in
  patch|minor|major) ;;
  -h|--help) usage ;;
  *) echo "Error: unknown bump type '$BUMP'"; usage ;;
esac

# ─── Get latest tag ──────────────────────────────────────────────────
LATEST_TAG=$(git tag --sort=-v:refname | grep -v '^nightly$' | head -1)

if [ -z "$LATEST_TAG" ]; then
  echo "No tags found. Starting from ${TAG_PREFIX}0.1.0${SUFFIX}"
  LATEST_TAG="${TAG_PREFIX}0.0.0${SUFFIX}"
fi

echo "Latest tag: $LATEST_TAG"

# ─── Strip prefix + suffix, split version ────────────────────────────
VERSION=$(echo "$LATEST_TAG" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
IFS='.' read -r MAJOR MINOR PATCH <<< "$VERSION"

# ─── Bump version ────────────────────────────────────────────────────
case "$BUMP" in
  patch) PATCH=$((PATCH + 1)) ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
esac

# `NEW_VERSION` is the value written into package.json / Cargo.toml /
# tauri.conf.json. `NEW_TAG` is what gets pushed to GitHub.
NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}${SUFFIX}"
NEW_TAG="${TAG_PREFIX}${NEW_VERSION}"

# ─── Build changelog from commits since last tag ─────────────────────
echo ""
echo "──────────────────────────────────────"
echo "  $LATEST_TAG → $NEW_TAG ($BUMP)"
echo "──────────────────────────────────────"
echo ""

if [ "$LATEST_TAG" = "${TAG_PREFIX}0.0.0${SUFFIX}" ]; then
  COMMITS=$(git log --oneline --no-decorate)
else
  COMMITS=$(git log "${LATEST_TAG}..HEAD" --oneline --no-decorate)
fi

if [ -z "$COMMITS" ]; then
  echo "No new commits since $LATEST_TAG. Aborting."
  exit 1
fi

# ─── Format changelog ────────────────────────────────────────────────
CHANGELOG=$(echo "$COMMITS" | while IFS= read -r line; do
  # Strip the short hash, keep only the message
  MSG="${line#* }"
  echo "- $MSG"
done)

TAG_BODY="Release ${NEW_TAG}

Changes since ${LATEST_TAG}:

${CHANGELOG}
"

echo "$TAG_BODY"
echo "──────────────────────────────────────"
echo ""

# ─── Confirm ─────────────────────────────────────────────────────────
read -rp "Create tag $NEW_TAG? [y/N] " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

# ─── Bump version in project files ───────────────────────────────────
echo "Updating version in project files..."

sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${NEW_VERSION}\"/" package.json
sed -i '' "s/^version = \"[^\"]*\"/version = \"${NEW_VERSION}\"/" src-tauri/Cargo.toml
sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"${NEW_VERSION}\"/" src-tauri/tauri.conf.json

sed -i '' '/^name = "'"${CARGO_PKG_NAME}"'"$/{n; s/^version = "[^"]*"/version = "'"${NEW_VERSION}"'"/;}' src-tauri/Cargo.lock

git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore(release): bump to ${NEW_TAG}"

echo "Version bumped and committed."
echo ""

# ─── Create annotated tag ────────────────────────────────────────────
git tag -a "$NEW_TAG" -m "$TAG_BODY"

echo ""
echo "Tag $NEW_TAG created. Pushing to origin..."
echo ""

git push origin main
git push origin "$NEW_TAG"

echo ""
echo "Tag $NEW_TAG pushed to GitHub."

# ─── Prune old tags (keep last 4, never touch 'nightly') ─────────────
KEEP_COUNT=4
echo ""
echo "Pruning old tags (keeping last ${KEEP_COUNT})..."

git fetch --tags --prune --prune-tags origin >/dev/null 2>&1 || true

ALL_TAGS=$(git tag -l --sort=-v:refname | grep -v '^nightly$' || true)
OLD_TAGS=$(echo "$ALL_TAGS" | tail -n +$((KEEP_COUNT + 1)))

if [ -z "$OLD_TAGS" ]; then
  echo "Nothing to prune."
else
  HAVE_GH=0
  command -v gh >/dev/null 2>&1 && HAVE_GH=1
  echo "$OLD_TAGS" | while IFS= read -r tag; do
    [ -z "$tag" ] && continue
    echo "  deleting $tag"
    if [ "$HAVE_GH" = "1" ]; then
      gh release delete "$tag" --cleanup-tag -y >/dev/null 2>&1 || true
    fi
    git push origin --delete "$tag" >/dev/null 2>&1 || true
    git tag -d "$tag" >/dev/null 2>&1 || true
  done
  echo "Pruned."
fi
