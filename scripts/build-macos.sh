#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
[ "$(uname -s)" = Darwin ] || { echo 'Run this script on a Mac.' >&2; exit 1; }
cargo test --locked
cargo build --release --locked
bundle='dist/AULA L99.app'
mkdir -p "$bundle/Contents/MacOS"
cp target/release/aula_l99_linux "$bundle/Contents/MacOS/aula-l99"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>aula-l99</string>
<key>CFBundleIdentifier</key><string>com.craftanything.aula-l99</string>
<key>CFBundleName</key><string>AULA L99</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
echo "Created $bundle for this Mac's architecture (unsigned desktop preview)."
