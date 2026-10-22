#!/bin/sh
set -eu

cd "$(dirname "$0")"
version="${1:-0.0.0}"
app=.build/Warlogger.app

swift build -c release --arch arm64 --arch x86_64
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$(swift build -c release --arch arm64 --arch x86_64 --show-bin-path)/Warlogger" "$app/Contents/MacOS/"
cp ../host/host.js ../assets/warlogger.icns "$app/Contents/Resources/"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>Warlogger</string>
  <key>CFBundleIconFile</key><string>warlogger</string>
  <key>CFBundleIdentifier</key><string>com.patvuk.warlogger</string>
  <key>CFBundleName</key><string>Warlogger</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
EOF

codesign --force --deep --sign - --options runtime --entitlements Warlogger.entitlements "$app"
ditto -c -k --keepParent "$app" ".build/Warlogger-macos.zip"
echo ".build/Warlogger-macos.zip"
