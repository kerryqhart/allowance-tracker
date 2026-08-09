#!/usr/bin/env bash
#
# Build the Allowance Tracker macOS .app bundle and install it with
# permissions that let *every* user account on the Mac (and any
# iCloud-synced copy) read and run it.
#
# Why this script exists
# ----------------------
# The finished bundle can end up owner-only, so other user accounts on
# the Mac (or an iCloud-synced copy on another machine) can't read or
# launch it: the .app directory ends up 700 (others can't even `cd` into
# it) and the executable ends up 751 = `-rwxr-x--x` (others may execute
# but cannot READ it, and macOS must read a Mach-O to load it). iCloud
# Drive is especially prone to leaving these owner-only perms. There is
# no native `cargo bundle` hook to fix this, so we normalize permissions
# here.
#
# `chmod -R a+rX` makes everything world-readable and adds the execute
# bit only to directories and files that are already executable (i.e. the
# binary), yielding the standard 755 dirs / 644 files / 755 binary.
#
# Usage
# -----
#   scripts/install.sh [DEST_DIR]
#
#   DEST_DIR  Directory to install "Allowance Tracker.app" into.
#             Precedence: $1  >  $ALLOWANCE_INSTALL_DIR  >  /Applications
#             (the default; shared with every user account on the Mac).
#             Pass "-" to build and fix perms only, without installing
#             (useful for testing; leaves the bundle under target/).
#
# Examples
#   scripts/install.sh                       # build + install to /Applications
#   scripts/install.sh "/path/to/dir"        # install into a chosen dir
#   ALLOWANCE_INSTALL_DIR="…" scripts/install.sh
#   scripts/install.sh -                      # build + fix perms, no install
#
# Note: installing to /Applications may require an admin account.

set -euo pipefail

APP_NAME="Allowance Tracker.app"
EXECUTABLE="allowance-tracker-egui"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILT_APP="$REPO_ROOT/target/release/bundle/osx/$APP_NAME"

# Where to install. /Applications makes the app available to every user
# account on the Mac. Edit DEFAULT_DEST or pass an argument to change it.
DEFAULT_DEST="/Applications"
DEST_DIR="${1:-${ALLOWANCE_INSTALL_DIR:-$DEFAULT_DEST}}"

echo "==> Building release bundle (cargo bundle --release)"
cargo bundle --release --package "$EXECUTABLE" --format osx

if [ ! -d "$BUILT_APP" ]; then
  echo "error: expected bundle not found at:" >&2
  echo "       $BUILT_APP" >&2
  exit 1
fi

echo "==> Embedding app icon (.icns) into the bundle"
# cargo-bundle 0.9 does not populate Contents/Resources (it drops neither the
# icon nor other resources), so the .app would show a generic Finder icon —
# most visibly when copied to another machine or a cloud drive. Embed the
# pre-generated icon (see scripts/make-icon.sh) and point Info.plist at it.
# (This is separate from the running app's Dock tile, which eframe sets at
# runtime from egui-frontend/assets/app-icon.png.)
ICNS_SRC="$REPO_ROOT/egui-frontend/assets/AppIcon.icns"
if [ -f "$ICNS_SRC" ]; then
  mkdir -p "$BUILT_APP/Contents/Resources"
  cp "$ICNS_SRC" "$BUILT_APP/Contents/Resources/AppIcon.icns"
  PLIST="$BUILT_APP/Contents/Info.plist"
  if /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile AppIcon" "$PLIST" 2>/dev/null; then
    :
  else
    /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string AppIcon" "$PLIST"
  fi
else
  echo "warning: $ICNS_SRC not found — run scripts/make-icon.sh." >&2
  echo "         The bundle will fall back to a generic Finder icon." >&2
fi

echo "==> Normalizing permissions on the built bundle"
chmod -R a+rX "$BUILT_APP"

if [ "$DEST_DIR" = "-" ]; then
  echo "==> Build-only mode: skipping install."
  echo "    Bundle ready at: $BUILT_APP"
  ls -ld "$BUILT_APP"
  ls -l "$BUILT_APP/Contents/MacOS/$EXECUTABLE"
  exit 0
fi

DEST_APP="$DEST_DIR/$APP_NAME"
echo "==> Installing to: $DEST_APP"
mkdir -p "$DEST_DIR"
# Replace any existing copy so no stale files linger, then use ditto
# (Apple's bundle-aware copy) to lay down the fresh .app.
rm -rf "$DEST_APP"
ditto "$BUILT_APP" "$DEST_APP"

echo "==> Normalizing permissions on the installed bundle"
# Re-apply at the destination: this is the copy other users run, and it's
# the copy iCloud can leave owner-only.
chmod -R a+rX "$DEST_APP"

# Nudge Launch Services so Finder/Dock refresh the icon instead of showing a
# stale cached one. Harmless if it fails.
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [ -x "$LSREGISTER" ]; then
  echo "==> Refreshing Launch Services (icon cache)"
  "$LSREGISTER" -f "$DEST_APP" || true
fi

echo "==> Done."
echo "    Installed: $DEST_APP"
ls -ld "$DEST_APP"
ls -l "$DEST_APP/Contents/MacOS/$EXECUTABLE"
