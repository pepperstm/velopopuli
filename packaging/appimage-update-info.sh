#!/usr/bin/env bash
# Embeds update information in the AppImage and writes its .zsync file next to it, so
# AppImageUpdate and similar tools can update it from the latest GitHub release.
#
#   packaging/appimage-update-info.sh target/release/bundle/appimage/VeloPopuli_X.Y.Z_amd64.AppImage
#
# Needs appimagetool (APPIMAGETOOL=path, default `appimagetool` on the PATH). The AppImage is
# unpacked and packed again with the same contents; only the update information is added.
set -euo pipefail

appimage=$(realpath "$1")
tool=${APPIMAGETOOL:-appimagetool}
# Only full releases: "latest" skips pre-releases such as 0.4.0-beta.1.
info="gh-releases-zsync|pepperstm|velopopuli|latest|VeloPopuli_*_amd64.AppImage.zsync"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
"$appimage" --appimage-extract >/dev/null
mkdir out
# The .zsync names the AppImage it describes, so it's built under the final name.
out="out/$(basename "$appimage")"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream -u "$info" squashfs-root "$out"

test -s "$out.zsync" || { echo "appimagetool made no .zsync file" >&2; exit 1; }
embedded=$("$out" --appimage-updateinformation)
if [ "$embedded" != "$info" ]; then
  echo "update information not embedded: got '$embedded'" >&2
  exit 1
fi

mv -f "$out" "$appimage"
mv -f "$out.zsync" "$appimage.zsync"
chmod +x "$appimage"
echo "Update information: $info"
ls -la "$appimage" "$appimage.zsync"
