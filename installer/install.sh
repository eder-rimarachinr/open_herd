#!/usr/bin/env bash
set -euo pipefail

PHPENV_DIR="${HOME}/.phpenv"
BIN_DIR="${HOME}/.local/bin"
REPO_URL="https://github.com/open-herd/phpenv"
VERSION="${PHPENV_VERSION:-latest}"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

info()    { echo -e "${CYAN}[phpenv]${NC} $*"; }
success() { echo -e "${GREEN}[phpenv]${NC} $*"; }
warn()    { echo -e "${YELLOW}[phpenv]${NC} $*"; }
die()     { echo -e "${RED}[phpenv] ERROR:${NC} $*" >&2; exit 1; }

require() { command -v "$1" &>/dev/null || die "$1 is required but not installed."; }

# ── Pre-flight ────────────────────────────────────────────────────────────────

info "Checking dependencies..."
require curl
require nginx
require mkcert

# ── Directories ───────────────────────────────────────────────────────────────

info "Creating directories under ${PHPENV_DIR}..."
mkdir -p \
    "${PHPENV_DIR}/nginx/sites" \
    "${PHPENV_DIR}/php" \
    "${PHPENV_DIR}/certs" \
    "${PHPENV_DIR}/logs" \
    "${BIN_DIR}"

# ── mkcert CA ─────────────────────────────────────────────────────────────────

info "Installing local CA with mkcert..."
mkcert -install

# ── Download daemon binary ────────────────────────────────────────────────────

ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64)  ARCH_SLUG="amd64" ;;
    aarch64) ARCH_SLUG="arm64" ;;
    *)       die "Unsupported architecture: ${ARCH}" ;;
esac

BINARY_URL="${REPO_URL}/releases/download/${VERSION}/phpenv-daemon-linux-${ARCH_SLUG}"
info "Downloading daemon (${VERSION} ${ARCH_SLUG})..."
curl -fsSL "${BINARY_URL}" -o "${BIN_DIR}/phpenv-daemon"
chmod +x "${BIN_DIR}/phpenv-daemon"

CLI_URL="${REPO_URL}/releases/download/${VERSION}/phpenv-linux-${ARCH_SLUG}"
curl -fsSL "${CLI_URL}" -o "${BIN_DIR}/phpenv"
chmod +x "${BIN_DIR}/phpenv"

# ── systemd service ───────────────────────────────────────────────────────────

SERVICE_FILE="${HOME}/.config/systemd/user/phpenv-daemon.service"
mkdir -p "$(dirname "${SERVICE_FILE}")"

cat > "${SERVICE_FILE}" << EOF
[Unit]
Description=phpenv daemon
After=network.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/phpenv-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now phpenv-daemon

# ── Shell PATH ────────────────────────────────────────────────────────────────

SHELL_RC=""
case "${SHELL}" in
    */bash) SHELL_RC="${HOME}/.bashrc" ;;
    */zsh)  SHELL_RC="${HOME}/.zshrc"  ;;
esac

if [[ -n "${SHELL_RC}" ]]; then
    if ! grep -q 'phpenv' "${SHELL_RC}"; then
        echo '' >> "${SHELL_RC}"
        echo '# phpenv' >> "${SHELL_RC}"
        echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> "${SHELL_RC}"
    fi
fi

success "phpenv installed successfully!"
echo ""
echo "  Daemon: systemctl --user status phpenv-daemon"
echo "  CLI:    phpenv status"
echo ""
warn "Restart your shell or run: source ${SHELL_RC:-~/.bashrc}"
