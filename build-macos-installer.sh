#!/bin/sh
# Script para criar instalador macOS para CPU Limiter

set -e

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$PROJECT_DIR/target/release"
APP_NAME="CPU Limiter"
BUNDLE_NAME="cpu_limiter"
VERSION="0.1.0"
DMG_NAME="CPULimiter-${VERSION}.dmg"
DIST_DIR="$PROJECT_DIR/dist"

echo "🔨 Compilando para macOS (release)..."
cargo build --release

echo "📦 Criando app bundle..."
mkdir -p "$DIST_DIR"
rm -rf "$DIST_DIR/$APP_NAME.app"

APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Copiar executável
cp "$BUILD_DIR/$BUNDLE_NAME" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# Copiar ícone (se existir)
if [ -f "$PROJECT_DIR/src/icon.png" ]; then
    cp "$PROJECT_DIR/src/icon.png" "$RESOURCES_DIR/AppIcon.png"
fi

# Criar Info.plist
cat > "$CONTENTS_DIR/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>CPU Limiter</string>
    <key>CFBundleIdentifier</key>
    <string>com.alexkads.cpu-limiter</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>CPU Limiter</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2025 Alex Fonseca. All rights reserved.</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSRequiresIPhoneOS</key>
    <false/>
</dict>
</plist>
EOF

echo "✅ App bundle criado: $APP_BUNDLE"

echo "💿 Criando DMG..."
rm -f "$DIST_DIR/$DMG_NAME"

# Criar pasta temporária para o DMG
DMG_TMP="$DIST_DIR/dmg_tmp"
rm -rf "$DMG_TMP"
mkdir -p "$DMG_TMP"

# Copiar app bundle
cp -r "$APP_BUNDLE" "$DMG_TMP/"

# Criar link simbólico para Applications
ln -s /Applications "$DMG_TMP/Applications"

# Criar DMG temporário (read-write)
DMG_TMP_FILE="$DIST_DIR/tmp.dmg"
rm -f "$DMG_TMP_FILE"

hdiutil create -volname "$APP_NAME" \
    -srcfolder "$DMG_TMP" \
    -ov -format UDRW \
    "$DMG_TMP_FILE"

# Montar o DMG para personalizar
MOUNT_DIR="/Volumes/$APP_NAME"
hdiutil attach "$DMG_TMP_FILE" -mountpoint "$MOUNT_DIR"

# Aguardar um pouco para garantir que está montado
sleep 2

# Configurar aparência da janela do Finder (opcional, via AppleScript)
echo '
   tell application "Finder"
     tell disk "'$APP_NAME'"
           open
           set current view of container window to icon view
           set toolbar visible of container window to false
           set statusbar visible of container window to false
           set the bounds of container window to {400, 100, 900, 450}
           set viewOptions to the icon view options of container window
           set arrangement of viewOptions to not arranged
           set icon size of viewOptions to 72
           set position of item "'$APP_NAME'.app" of container window to {125, 175}
           set position of item "Applications" of container window to {375, 175}
           close
           open
           update without registering applications
           delay 2
     end tell
   end tell
' | osascript || true

# Desmontar
hdiutil detach "$MOUNT_DIR"

# Converter para DMG final compactado (somente leitura)
hdiutil convert "$DMG_TMP_FILE" \
    -format UDZO \
    -o "$DIST_DIR/$DMG_NAME"

# Limpar arquivos temporários
rm -f "$DMG_TMP_FILE"
rm -rf "$DMG_TMP"

echo "✅ DMG criado: $DIST_DIR/$DMG_NAME"

echo ""
echo "================================"
echo "✨ Build concluído com sucesso!"
echo "================================"
echo ""
echo "Arquivos disponíveis em: $DIST_DIR"
echo ""
echo "Para instalar:"
echo "  1. Abra o DMG: open '$DIST_DIR/$DMG_NAME'"
echo "  2. Arraste 'CPU Limiter.app' para a pasta 'Applications'"
echo ""
