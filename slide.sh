#!/usr/bin/env bash

# slide.sh - Slide CLI local execution script
# This script builds the Rust binary and runs it with the Node.js launcher

set -euo pipefail

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🔨 Building Rust binary...${NC}"
cd slide-rs
cargo build --release
if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Rust build failed${NC}"
    exit 1
fi

# Determine target triple for this platform
PLATFORM="$(uname -s)"
ARCH="$(uname -m)"
case "$PLATFORM" in
    Linux)
        case "$ARCH" in
            x86_64) TARGET_TRIPLE="x86_64-unknown-linux-musl" ;;
            aarch64) TARGET_TRIPLE="aarch64-unknown-linux-musl" ;;
            *) echo -e "${RED}❌ Unsupported Linux architecture: $ARCH${NC}"; exit 1 ;;
        esac
        ;;
    Darwin)
        case "$ARCH" in
            x86_64) TARGET_TRIPLE="x86_64-apple-darwin" ;;
            arm64) TARGET_TRIPLE="aarch64-apple-darwin" ;;
            *) echo -e "${RED}❌ Unsupported macOS architecture: $ARCH${NC}"; exit 1 ;;
        esac
        ;;
    *)
        echo -e "${RED}❌ Unsupported platform: $PLATFORM${NC}"
        exit 1
        ;;
esac

echo -e "${GREEN}📦 Copying binary to slide-cli/bin/slide-${TARGET_TRIPLE}...${NC}"
cp target/release/slide "../slide-cli/bin/slide-${TARGET_TRIPLE}"
chmod +x "../slide-cli/bin/slide-${TARGET_TRIPLE}"

echo -e "${GREEN}🚀 Starting Slide CLI...${NC}"
cd ../slide-cli
export SLIDE_APP=1
node bin/slide.js "$@"