#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="Liebert Dynamic Island"
BIN_NAME="liebert-dynamic-island"
VERSION="0.1.0"
ARCH="x86_64"
APPDIR="$ROOT_DIR/target/appimage/AppDir"
RUNTIME_PATH="$ROOT_DIR/target/appimage/runtime-$ARCH"
OUT_PATH="$ROOT_DIR/dist/Liebert_Dynamic_Island-${VERSION}-${ARCH}.AppImage"
RUNTIME_URL="https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-${ARCH}"

if [[ "${1:-}" != "--skip-cargo" ]]; then
    cargo build --release
fi

BIN_PATH="$ROOT_DIR/target/release/$BIN_NAME"
if [[ ! -x "$BIN_PATH" ]]; then
    echo "Error: release binary not found at $BIN_PATH" >&2
    exit 1
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$ROOT_DIR/dist" "$ROOT_DIR/target/appimage"
cp "$BIN_PATH" "$APPDIR/usr/bin/$BIN_NAME"
chmod 755 "$APPDIR/usr/bin/$BIN_NAME"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/liebert-dynamic-island" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

cat > "$APPDIR/${BIN_NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=Lightweight native Rust Dynamic Island notification overlay
Exec=${BIN_NAME}
Icon=${BIN_NAME}
Categories=Utility;
Terminal=false
StartupNotify=false
EOF

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$APPDIR/${BIN_NAME}.desktop"
fi

# Generate a clean 256x256 RGBA PNG icon (dark rounded square + Dynamic Island pill).
python3 - "$APPDIR/${BIN_NAME}.png" <<'PYEOF'
import math, struct, sys, zlib

out_path = sys.argv[1]
w = h = 256
raw = bytearray()

def clamp01(v):
    return 0.0 if v < 0.0 else (1.0 if v > 1.0 else v)

def sd_rounded_rect(px, py, cx, cy, hw, hh, r):
    dx = abs(px - cx) - (hw - r)
    dy = abs(py - cy) - (hh - r)
    return math.hypot(max(dx, 0.0), max(dy, 0.0)) + min(max(dx, dy), 0.0) - r

for y in range(h):
    raw.append(0)
    for x in range(w):
        d_bg = sd_rounded_rect(x + 0.5, y + 0.5, 128.0, 128.0, 112.0, 112.0, 48.0)
        bg_a = clamp01(0.5 - d_bg)
        if bg_a <= 0.0:
            raw.extend((0, 0, 0, 0))
            continue

        # Subtle dark slate gradient background
        t = y / (h - 1)
        r = 18.0 + 10.0 * t
        g = 20.0 + 10.0 * t
        b = 28.0 + 14.0 * t

        # Dynamic Island pill near top-center
        d_pill = sd_rounded_rect(x + 0.5, y + 0.5, 128.0, 74.0, 72.0, 22.0, 22.0)
        pill_cov = clamp01(0.5 - d_pill)
        if pill_cov > 0.0:
            r = r * (1.0 - pill_cov) + 6.0 * pill_cov
            g = g * (1.0 - pill_cov) + 6.0 * pill_cov
            b = b * (1.0 - pill_cov) + 9.0 * pill_cov

        # Accent indicator dot inside left side of pill
        d_dot = math.hypot((x + 0.5) - 76.0, (y + 0.5) - 74.0) - 9.0
        dot_cov = clamp01(0.5 - d_dot)
        if dot_cov > 0.0:
            r = r * (1.0 - dot_cov) + 110.0 * dot_cov
            g = g * (1.0 - dot_cov) + 195.0 * dot_cov
            b = b * (1.0 - dot_cov) + 255.0 * dot_cov

        raw.extend((int(round(r)), int(round(g)), int(round(b)), int(round(bg_a * 255.0))))

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xffffffff)

ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")
with open(out_path, "wb") as f:
    f.write(png)
PYEOF
cp "$APPDIR/${BIN_NAME}.png" "$APPDIR/.DirIcon"

if command -v appimagetool >/dev/null 2>&1; then
    ARCH="$ARCH" appimagetool "$APPDIR" "$OUT_PATH"
elif command -v mksquashfs >/dev/null 2>&1; then
    if [[ ! -f "$RUNTIME_PATH" ]]; then
        echo "Downloading AppImage type2-runtime..."
        curl -fL --retry 3 -o "$RUNTIME_PATH" "$RUNTIME_URL"
    fi
    SQFS_PATH="$ROOT_DIR/target/appimage/app.squashfs"
    rm -f "$SQFS_PATH"
    mksquashfs "$APPDIR" "$SQFS_PATH" -root-owned -noappend -comp zstd -Xcompression-level 19 -quiet
    cat "$RUNTIME_PATH" "$SQFS_PATH" > "$OUT_PATH"
    chmod 755 "$OUT_PATH"
else
    echo "Error: neither appimagetool nor mksquashfs found." >&2
    exit 1
fi

echo "AppImage built: $OUT_PATH"
