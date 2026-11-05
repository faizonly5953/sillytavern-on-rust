#!/usr/bin/env bash
# Turns an Ubuntu/Debian x86_64 VPS into a GitHub Actions self-hosted runner that
# builds the Android APK. Releases are still published to GitHub by the workflow.
#
# Usage (as a normal user with sudo, NOT root):
#   RUNNER_TOKEN=<token> bash setup-vps-runner.sh
# Get a token (valid 1 hour) on your PC with:
#   gh api -X POST repos/faizonly5953/sillytavern-on-rust/actions/runners/registration-token --jq .token
# or from GitHub: repo Settings -> Actions -> Runners -> New self-hosted runner.
#
# Afterwards switch the workflow to this runner:
#   gh variable set ANDROID_RUNNER -R faizonly5953/sillytavern-on-rust --body '["self-hosted","linux","android"]'
#
# Re-running is safe; finished steps are skipped.
set -euo pipefail

REPO_URL="https://github.com/faizonly5953/sillytavern-on-rust"
SDK_DIR="${ANDROID_SDK_DIR:-$HOME/android-sdk}"
RUNNER_DIR="${RUNNER_DIR:-$HOME/actions-runner}"
RUNNER_NAME="${RUNNER_NAME:-$(hostname)-android}"
NDK_VERSION="27.2.12479018"
# Android command-line tools; any recent build works.
CMDLINE_TOOLS_URL="${CMDLINE_TOOLS_URL:-https://dl.google.com/android/repository/commandlinetools-linux-13114758_latest.zip}"

if [ "$(id -u)" -eq 0 ]; then
  echo "Run as a normal user with sudo; the GitHub runner refuses to run as root." >&2
  exit 1
fi
if [ "$(uname -m)" != "x86_64" ]; then
  echo "The Android NDK only ships Linux x86_64 host tools; this VPS is $(uname -m)." >&2
  exit 1
fi

echo "==> System packages"
sudo apt-get update -y
sudo apt-get install -y curl git unzip zip jq build-essential pkg-config openjdk-21-jdk-headless

mem_gb=$(awk '/MemTotal/ {printf "%d", $2/1024/1024}' /proc/meminfo)
swap_gb=$(awk '/SwapTotal/ {printf "%d", $2/1024/1024}' /proc/meminfo)
if [ $((mem_gb + swap_gb)) -lt 8 ]; then
  echo "!! Only ${mem_gb} GB RAM + ${swap_gb} GB swap. The release build (LTO) wants ~8 GB."
  echo "!! Add swap, for example:"
  echo "   sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile"
  echo "   echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab"
fi

echo "==> Android SDK in $SDK_DIR"
if [ ! -x "$SDK_DIR/cmdline-tools/latest/bin/sdkmanager" ]; then
  tmp=$(mktemp -d)
  curl -fsSL "$CMDLINE_TOOLS_URL" -o "$tmp/tools.zip"
  unzip -q "$tmp/tools.zip" -d "$tmp"
  mkdir -p "$SDK_DIR/cmdline-tools"
  rm -rf "$SDK_DIR/cmdline-tools/latest"
  mv "$tmp/cmdline-tools" "$SDK_DIR/cmdline-tools/latest"
  rm -rf "$tmp"
fi
SDKMANAGER="$SDK_DIR/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKMANAGER" --sdk_root="$SDK_DIR" --licenses >/dev/null || true
"$SDKMANAGER" --sdk_root="$SDK_DIR" --install "platform-tools" "ndk;$NDK_VERSION"
# Gradle downloads the exact platform/build-tools it needs on the first build.

echo "==> GitHub Actions runner in $RUNNER_DIR"
mkdir -p "$RUNNER_DIR"
cd "$RUNNER_DIR"
if [ ! -x ./config.sh ]; then
  version=$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | sed 's/^v//')
  curl -fsSL -o runner.tgz "https://github.com/actions/runner/releases/download/v${version}/actions-runner-linux-x64-${version}.tar.gz"
  tar xzf runner.tgz
  rm runner.tgz
fi

# Environment the runner passes to every job.
cat > .env <<EOF
ANDROID_HOME=$SDK_DIR
ANDROID_SDK_ROOT=$SDK_DIR
NDK_HOME=$SDK_DIR/ndk/$NDK_VERSION
LANG=C.UTF-8
EOF

if [ ! -f .runner ]; then
  if [ -z "${RUNNER_TOKEN:-}" ]; then
    echo "Set RUNNER_TOKEN to register the runner (see the top of this script)." >&2
    exit 1
  fi
  ./config.sh --unattended --url "$REPO_URL" --token "$RUNNER_TOKEN" \
    --name "$RUNNER_NAME" --labels android --work _work --replace
fi

if ! compgen -G "/etc/systemd/system/actions.runner.*.service" >/dev/null; then
  sudo ./svc.sh install "$USER"
fi
sudo ./svc.sh start || true
sudo ./svc.sh status | head -n 5 || true

echo
echo "Done. Point the release workflow at this runner with:"
echo "  gh variable set ANDROID_RUNNER -R faizonly5953/sillytavern-on-rust --body '[\"self-hosted\",\"linux\",\"android\"]'"
echo "To go back to GitHub's runners: gh variable delete ANDROID_RUNNER -R faizonly5953/sillytavern-on-rust"
