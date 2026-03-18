#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# App Beholder - Build & Deploy Script
# Target: Ubuntu aarch64 on AWS EC2
# ============================================================

# --- Configuration ---
EC2_HOST="63.183.192.131"
EC2_USER="ubuntu"
SSH_KEY="$HOME/.ssh/aws.pem"
REMOTE_DIR="/opt/appbeholder"
SERVICE_NAME="appbeholder"
DB_NAME="appbeholder"
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"

SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=10"
SSH_CMD="ssh $SSH_OPTS -i $SSH_KEY ${EC2_USER}@${EC2_HOST}"
SCP_CMD="scp $SSH_OPTS -i $SSH_KEY"

# --- Colors ---
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[+]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
error() { echo -e "${RED}[x]${NC} $1"; exit 1; }

# ============================================================
# Step 1: Cross-compile for aarch64-unknown-linux-gnu
# ============================================================
info "Building appbeholder for aarch64-linux-gnu using Docker..."

cd "$PROJECT_DIR"

# Build using a Docker container with the Rust toolchain
docker run --rm \
    -v "$PROJECT_DIR":/app \
    -v cargo-cache:/usr/local/cargo/registry \
    -w /app \
    rust:latest \
    bash -c "
        apt-get update -qq && apt-get install -y -qq libssl-dev pkg-config > /dev/null 2>&1
        rustup target add aarch64-unknown-linux-gnu
        cargo build --release --target aarch64-unknown-linux-gnu -p appbeholder 2>&1
    "

BINARY="target/aarch64-unknown-linux-gnu/release/appbeholder"

if [ ! -f "$BINARY" ]; then
    error "Build failed - binary not found at $BINARY"
fi

info "Build complete: $(ls -lh "$BINARY" | awk '{print $5}')"

# ============================================================
# Step 2: Setup remote server (PostgreSQL + directories)
# ============================================================
info "Setting up remote server..."

$SSH_CMD << 'SETUP_EOF'
set -euo pipefail

# Install PostgreSQL if not present
if ! command -v psql &> /dev/null; then
    echo "[+] Installing PostgreSQL..."
    sudo apt-get update -qq
    sudo apt-get install -y -qq postgresql postgresql-client > /dev/null 2>&1
    sudo systemctl enable postgresql
    sudo systemctl start postgresql
    echo "[+] PostgreSQL installed and started"
else
    echo "[+] PostgreSQL already installed"
    if ! systemctl is-active --quiet postgresql; then
        sudo systemctl start postgresql
    fi
fi

# Create database and user if not exist
sudo -u postgres psql -tc "SELECT 1 FROM pg_database WHERE datname = 'appbeholder'" | grep -q 1 || {
    sudo -u postgres createdb appbeholder
    echo "[+] Database 'appbeholder' created"
}

# Allow local connections via peer auth (default on Ubuntu)
echo "[+] PostgreSQL ready"

# Create app directory
sudo mkdir -p /opt/appbeholder
sudo chown ubuntu:ubuntu /opt/appbeholder

echo "[+] Remote setup complete"
SETUP_EOF

# ============================================================
# Step 3: Deploy binary and config
# ============================================================
info "Stopping service before deploy..."
$SSH_CMD "sudo systemctl stop appbeholder 2>/dev/null || true"

info "Deploying binary to ${EC2_HOST}..."

$SCP_CMD "$BINARY" "${EC2_USER}@${EC2_HOST}:${REMOTE_DIR}/appbeholder"
$SSH_CMD "chmod +x ${REMOTE_DIR}/appbeholder && sudo setcap 'cap_net_bind_service=+ep' ${REMOTE_DIR}/appbeholder"

info "Deploying static files..."
$SSH_CMD "mkdir -p ${REMOTE_DIR}/static"
$SCP_CMD -r "$PROJECT_DIR/static/" "${EC2_USER}@${EC2_HOST}:${REMOTE_DIR}/static/"

info "Deploying configuration..."

# Create config.toml on remote if it doesn't exist
$SSH_CMD << CONF_EOF
cat > ${REMOTE_DIR}/config.toml << 'TOML'
[server]
host = "0.0.0.0"
port = 8080

[database]
url = "postgres:///appbeholder?host=/var/run/postgresql&user=ubuntu"

[retention]
logs_days = 7
traces_days = 30
metrics_days = 90
errors_days = 30
TOML
echo "[+] config.toml written"
CONF_EOF

# ============================================================
# Step 4: Create systemd service
# ============================================================
info "Setting up systemd service..."

$SSH_CMD << 'SERVICE_EOF'
sudo tee /etc/systemd/system/appbeholder.service > /dev/null << 'UNIT'
[Unit]
Description=App Beholder - Observability Platform
After=network.target postgresql.service
Requires=postgresql.service

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/opt/appbeholder
ExecStart=/opt/appbeholder/appbeholder
Restart=always
RestartSec=5
Environment=RUST_LOG=appbeholder=info,tower_http=info

[Install]
WantedBy=multi-user.target
UNIT

sudo systemctl daemon-reload
sudo systemctl enable appbeholder
echo "[+] Systemd service configured"
SERVICE_EOF

# ============================================================
# Step 5: Grant DB access to ubuntu user and restart
# ============================================================
info "Granting database access..."

$SSH_CMD << 'DB_EOF'
# Create ubuntu role in PostgreSQL if not exists
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname = 'ubuntu'" | grep -q 1 || {
    sudo -u postgres createuser ubuntu
    echo "[+] PostgreSQL user 'ubuntu' created"
}
sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE appbeholder TO ubuntu;" 2>/dev/null || true
# On PostgreSQL 15+, also grant schema privileges
sudo -u postgres psql -d appbeholder -c "GRANT ALL ON SCHEMA public TO ubuntu;" 2>/dev/null || true
echo "[+] Database access granted"
DB_EOF

# ============================================================
# Step 6: Restart service
# ============================================================
info "Restarting App Beholder..."

$SSH_CMD << 'RESTART_EOF'
sudo systemctl restart appbeholder
sleep 2
if systemctl is-active --quiet appbeholder; then
    echo "[+] App Beholder is running!"
else
    echo "[x] Service failed to start. Checking logs..."
    journalctl -u appbeholder --no-pager -n 20
    exit 1
fi
RESTART_EOF

# ============================================================
# Done!
# ============================================================
echo ""
info "============================================"
info "  App Beholder deployed successfully!"
info "  URL: http://beholder.lipinski.work"
info "============================================"
echo ""
info "Send a test log:"
echo "  curl -X POST http://beholder.lipinski.work/api/v1/logs \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -H 'X-Project-Slug: my-app' \\"
echo "    -d '{\"level\":\"info\",\"message\":\"Hello from App Beholder!\"}'"
