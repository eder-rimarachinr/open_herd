#!/usr/bin/env bash
# install.sh — Descarga e instala Open Herd desde GitHub Releases
#
# Uso:
#   curl -fsSL https://raw.githubusercontent.com/TU_USUARIO/open_herd/main/installer/install.sh | bash
#
# Con versión específica:
#   OPENHERD_VERSION=0.2.0 curl -fsSL ...install.sh | bash
#
# Con .deb (Debian/Ubuntu) en vez de AppImage:
#   OPENHERD_FORMAT=deb curl -fsSL ...install.sh | bash

set -euo pipefail

REPO_OWNER="TU_USUARIO"          # ← cambia esto por tu usuario de GitHub
REPO_NAME="open_herd"
API_BASE="https://api.github.com/repos/$REPO_OWNER/$REPO_NAME"
VERSION="${OPENHERD_VERSION:-}"
FORMAT="${OPENHERD_FORMAT:-appimage}"   # appimage | deb

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

info()    { echo -e "${CYAN}[open-herd]${NC} $*"; }
success() { echo -e "${GREEN}[open-herd]${NC} $*"; }
warn()    { echo -e "${YELLOW}[open-herd]${NC} $*"; }
die()     { echo -e "${RED}[open-herd] ERROR:${NC} $*" >&2; exit 1; }
require() { command -v "$1" &>/dev/null || die "$1 es necesario pero no está instalado."; }

require curl

# ── Resolución de versión ─────────────────────────────────────────────────────

if [[ -z "$VERSION" ]]; then
    info "Buscando la última versión..."
    VERSION=$(curl -fsSL "$API_BASE/releases/latest" | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')
    [[ -n "$VERSION" ]] || die "No se pudo obtener la versión de GitHub."
    info "Última versión: $VERSION"
fi

# ── Arquitectura ──────────────────────────────────────────────────────────────

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  DEB_ARCH="amd64"  ; APPIMAGE_ARCH="x86_64"  ;;
    aarch64) DEB_ARCH="arm64"  ; APPIMAGE_ARCH="aarch64"  ;;
    *)       die "Arquitectura no soportada: $ARCH" ;;
esac

# ── Instalar según formato ────────────────────────────────────────────────────

case "$FORMAT" in

    deb)
        require dpkg
        FILE="open-herd_${VERSION}_${DEB_ARCH}.deb"
        URL="https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/v$VERSION/$FILE"
        TMP="/tmp/$FILE"

        info "Descargando $FILE..."
        curl -fsSL "$URL" -o "$TMP"

        info "Instalando paquete .deb..."
        if command -v apt &>/dev/null; then
            sudo apt install -y "$TMP"
        else
            sudo dpkg -i "$TMP"
        fi
        rm -f "$TMP"

        success "Open Herd v$VERSION instalado."
        echo ""
        echo "  Ejecuta: open-herd"
        ;;

    appimage)
        INSTALL_DIR="${HOME}/.local/bin"
        FILE="Open_Herd_${VERSION}_${APPIMAGE_ARCH}.AppImage"
        URL="https://github.com/$REPO_OWNER/$REPO_NAME/releases/download/v$VERSION/$FILE"
        DEST="$INSTALL_DIR/open-herd"

        mkdir -p "$INSTALL_DIR"

        info "Descargando $FILE..."
        curl -fsSL "$URL" -o "$DEST"
        chmod +x "$DEST"

        # Crear entrada en el menú de aplicaciones
        DESKTOP_DIR="${HOME}/.local/share/applications"
        mkdir -p "$DESKTOP_DIR"
        cat > "$DESKTOP_DIR/open-herd.desktop" << EOF
[Desktop Entry]
Name=Open Herd
Comment=Local PHP development environment manager
Exec=$DEST %U
Icon=open-herd
Terminal=false
Type=Application
Categories=Development;
StartupWMClass=open-herd
EOF

        success "Open Herd v$VERSION instalado en $DEST"
        echo ""

        # Advertir si ~/.local/bin no está en PATH
        if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
            warn "$INSTALL_DIR no está en tu PATH."
            warn "Añade esta línea a tu ~/.bashrc o ~/.zshrc:"
            echo ""
            echo '  export PATH="$HOME/.local/bin:$PATH"'
            echo ""
        else
            echo "  Ejecuta: open-herd"
        fi
        ;;

    *)
        die "Formato desconocido: $FORMAT. Usa 'appimage' o 'deb'."
        ;;
esac
