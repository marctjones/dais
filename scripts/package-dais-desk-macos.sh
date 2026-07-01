#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
PROFILE="${PROFILE:-debug}"
OUTPUT_DIR="${OUTPUT_DIR:-${ROOT_DIR}/apps/dais-desk/target/macos}"
APP_NAME="${APP_NAME:-Dais Desk}"
BUNDLE_ID="${BUNDLE_ID:-social.dais.desk.debug}"
APP_PATH="${OUTPUT_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_PATH}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
EXECUTABLE_NAME="dais-desk"
REAL_EXECUTABLE_NAME="dais-desk-bin"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Dais Desk macOS packaging requires macOS." >&2
  exit 1
fi

case "${PROFILE}" in
  debug)
    BINARY_PATH="${ROOT_DIR}/apps/dais-desk/target/debug/dais-desk"
    BUNDLE_VERSION="debug"
    ;;
  release)
    BINARY_PATH="${ROOT_DIR}/apps/dais-desk/target/release/dais-desk"
    BUNDLE_VERSION="release"
    ;;
  *)
    echo "PROFILE must be debug or release, got: ${PROFILE}" >&2
    exit 1
    ;;
esac

echo "Building Dais Desk (${PROFILE})"
(
  cd "${ROOT_DIR}"
  if [[ "${PROFILE}" == "release" ]]; then
    cargo build --manifest-path apps/dais-desk/Cargo.toml --release
  else
    cargo build --manifest-path apps/dais-desk/Cargo.toml
  fi
)

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"
find "${MACOS_DIR}" -maxdepth 1 -type f ! -name "${EXECUTABLE_NAME}" -delete
ditto "${BINARY_PATH}" "${MACOS_DIR}/${REAL_EXECUTABLE_NAME}"
cat >"${MACOS_DIR}/${EXECUTABLE_NAME}" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SLINT_BACKEND="${SLINT_BACKEND:-winit}"
exec "${SCRIPT_DIR}/dais-desk-bin" "$@"
LAUNCHER
chmod +x "${MACOS_DIR}/${EXECUTABLE_NAME}"
chmod +x "${MACOS_DIR}/${REAL_EXECUTABLE_NAME}"

cat >"${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>${EXECUTABLE_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>${BUNDLE_VERSION}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.social-networking</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticTermination</key>
  <false/>
  <key>NSSupportsSuddenTermination</key>
  <false/>
</dict>
</plist>
PLIST

plutil -lint "${CONTENTS_DIR}/Info.plist" >/dev/null
printf 'APPL????' >"${CONTENTS_DIR}/PkgInfo"
codesign --force --deep --sign - "${APP_PATH}" >/dev/null
echo "Packaged ${APP_PATH}"
echo "Launch with: open -n \"${APP_PATH}\""
