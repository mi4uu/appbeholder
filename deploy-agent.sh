#!/bin/bash
set -eu

# ============================================================
# AppBeholder Agent - Build & Deploy to botmarley hosts
# Targets: bob (63.180.15.36), bot4jay (63.180.169.222)
# ============================================================

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
SSH_KEY="$HOME/.ssh/aws.pem"
SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=10"
REMOTE_DIR="/opt/appbeholder"
SERVICE_NAME="beholder-agent"

# Hosts (name:ip pairs)
HOST_NAMES="bob bot4jay"
bob_IP="63.180.15.36"
bot4jay_IP="63.180.169.222"

# --- Colors ---
GREEN='\033[0;32m'
NC='\033[0m'

info()  { echo "${GREEN}[+]${NC} $1"; }

# ============================================================
# Step 1: Cross-compile for aarch64-unknown-linux-gnu
# ============================================================
info "Building appbeholder-agent for aarch64-linux-gnu using Docker..."

cd "$PROJECT_DIR"

docker run --rm \
    -v "$PROJECT_DIR":/app \
    -v cargo-cache:/usr/local/cargo/registry \
    -w /app \
    rust:latest \
    bash -c "
        apt-get update -qq && apt-get install -y -qq libssl-dev pkg-config > /dev/null 2>&1
        rustup target add aarch64-unknown-linux-gnu
        cargo build --release --target aarch64-unknown-linux-gnu -p appbeholder-agent 2>&1
    "

BINARY="target/aarch64-unknown-linux-gnu/release/appbeholder-agent"

if [ ! -f "$BINARY" ]; then
    echo "[x] Build failed - binary not found at $BINARY"
    exit 1
fi

info "Build complete: $(ls -lh "$BINARY" | awk '{print $5}')"

# ============================================================
# Step 2: Deploy to each host
# ============================================================

deploy_host() {
    local HOST_NAME="$1"
    local HOST_IP="$2"
    local SSH_CMD="ssh $SSH_OPTS -i $SSH_KEY ubuntu@${HOST_IP}"
    local SCP_CMD="scp $SSH_OPTS -i $SSH_KEY"

    info "--- Deploying to $HOST_NAME ($HOST_IP) ---"

    # Create directory
    $SSH_CMD "sudo mkdir -p $REMOTE_DIR && sudo chown ubuntu:ubuntu $REMOTE_DIR"

    # Stop service if running
    $SSH_CMD "sudo systemctl stop $SERVICE_NAME 2>/dev/null || true"

    # Copy binary
    info "Copying binary to $HOST_NAME..."
    $SCP_CMD "$BINARY" "ubuntu@${HOST_IP}:${REMOTE_DIR}/appbeholder-agent"
    $SSH_CMD "chmod +x ${REMOTE_DIR}/appbeholder-agent"

    # Write agent.toml
    info "Writing agent.toml for $HOST_NAME..."
    $SSH_CMD "cat > ${REMOTE_DIR}/agent.toml" <<EOF
endpoint = "https://beholder.lipinski.work"
service_name = "botmarley"
hostname = "${HOST_NAME}"
interval_secs = 30
process_interval_secs = 60
EOF

    # Create systemd service
    info "Setting up systemd service on $HOST_NAME..."
    $SSH_CMD <<UNIT_EOF
sudo tee /etc/systemd/system/${SERVICE_NAME}.service > /dev/null <<'UNIT'
[Unit]
Description=AppBeholder Host Metrics Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/opt/appbeholder
ExecStart=/opt/appbeholder/appbeholder-agent /opt/appbeholder/agent.toml
Restart=always
RestartSec=10
Environment=RUST_LOG=appbeholder_agent=info

[Install]
WantedBy=multi-user.target
UNIT

sudo systemctl daemon-reload
sudo systemctl enable ${SERVICE_NAME}
UNIT_EOF

    # Start service
    info "Starting agent on $HOST_NAME..."
    $SSH_CMD <<'START_EOF'
sudo systemctl restart beholder-agent
sleep 2
if systemctl is-active --quiet beholder-agent; then
    echo "[+] beholder-agent is running!"
    journalctl -u beholder-agent --no-pager -n 3
else
    echo "[x] Service failed to start:"
    journalctl -u beholder-agent --no-pager -n 20
fi
START_EOF

    info "$HOST_NAME done!"
    echo ""
}

for HOST_NAME in $HOST_NAMES; do
    eval "HOST_IP=\${${HOST_NAME}_IP}"
    deploy_host "$HOST_NAME" "$HOST_IP"
done

# ============================================================
# Done!
# ============================================================
info "============================================"
info "  Agent deployed to all hosts!"
info "  Check metrics at: https://beholder.lipinski.work/projects/botmarley/metrics"
info "============================================"
