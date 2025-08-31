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

echo -e "${GREEN}🚀 Starting Slide CLI...${NC}"
cd ../slide-cli
export SLIDE_APP=1
node bin/slide.js "$@"