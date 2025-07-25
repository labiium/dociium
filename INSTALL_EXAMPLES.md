# Installation Examples for Dociium

This document provides step-by-step installation examples for different scenarios and platforms.

## Quick Start Examples

### Example 1: Basic Installation

```bash
# 1. Clone the repository
git clone https://github.com/example/dociium.git
cd dociium

# 2. Install from workspace root
cargo install --path .

# 3. Verify installation
which dociium
# Output: /Users/[username]/.cargo/bin/dociium

# 4. Test basic functionality (will start MCP server on stdio)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | dociium
```

### Example 2: Development Installation

```bash
# 1. Clone and enter directory
git clone https://github.com/example/dociium.git
cd dociium

# 2. Install in development mode with debug symbols
cargo install --path . --debug

# 3. Set up development environment
export RUST_LOG=debug
export RDOCS_CACHE_DIR=./dev_cache

# 4. Run with verbose logging
dociium
```

### Example 3: Production Installation

```bash
# 1. Install with optimizations
cargo install --path . --release

# 2. Set up production cache directory
sudo mkdir -p /opt/dociium/cache
sudo chown $USER:$USER /opt/dociium/cache

# 3. Set environment variables
export RDOCS_CACHE_DIR=/opt/dociium/cache
export RUST_LOG=warn

# 4. Run as service (systemd example)
sudo systemctl enable dociium
sudo systemctl start dociium
```

## Platform-Specific Examples

### macOS

```bash
# Install using Homebrew Rust
brew install rust
git clone https://github.com/example/dociium.git
cd dociium
cargo install --path .

# macOS-specific cache location
export RDOCS_CACHE_DIR="$HOME/Library/Caches/dociium"
```

### Linux (Ubuntu/Debian)

```bash
# Install Rust if not available
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install build dependencies
sudo apt update
sudo apt install build-essential pkg-config libssl-dev

# Install dociium
git clone https://github.com/example/dociium.git
cd dociium
cargo install --path .

# Linux-specific cache location
export RDOCS_CACHE_DIR="$HOME/.cache/dociium"
```

### Windows (PowerShell)

```powershell
# Install Rust (if not already installed)
# Download from https://rustup.rs/

# Clone repository
git clone https://github.com/example/dociium.git
cd dociium

# Install
cargo install --path .

# Windows-specific cache location
$env:RDOCS_CACHE_DIR = "$env:APPDATA\dociium\cache"
```

## MCP Client Integration Examples

### Claude Desktop Configuration

Create or edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dociium": {
      "command": "dociium",
      "args": [],
      "env": {
        "RUST_LOG": "info",
        "RDOCS_CACHE_DIR": "/Users/[username]/.cache/dociium"
      }
    }
  }
}
```

### Continue.dev Configuration

Add to your Continue configuration:

```json
{
  "mcpServers": [
    {
      "name": "dociium",
      "command": "dociium",
      "args": [],
      "env": {
        "RUST_LOG": "info"
      }
    }
  ]
}
```

### Custom MCP Client

```python
import asyncio
import json
from mcp import ClientSession, StdioServerParameters

async def main():
    # Connect to dociium MCP server
    server_params = StdioServerParameters(
        command="dociium",
        args=[],
        env={"RUST_LOG": "info"}
    )
    
    async with ClientSession(server_params) as session:
        # Initialize the connection
        await session.initialize()
        
        # Use the get_item_doc tool
        result = await session.call_tool(
            "get_item_doc",
            {
                "crate_name": "tokio",
                "path": "tokio::sync::Mutex",
                "version": None
            }
        )
        
        print(json.dumps(result, indent=2))

if __name__ == "__main__":
    asyncio.run(main())
```

## Docker Examples

### Basic Docker Setup

```dockerfile
# Dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo install --path . --root /usr/local

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/dociium /usr/bin/dociium

ENV RDOCS_CACHE_DIR=/cache
VOLUME ["/cache"]

CMD ["dociium"]
```

```bash
# Build and run
docker build -t dociium .
docker run -v $(pwd)/cache:/cache dociium
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  dociium:
    build: .
    volumes:
      - cache_data:/cache
      - ./config:/config
    environment:
      - RUST_LOG=info
      - RDOCS_CACHE_DIR=/cache
    restart: unless-stopped

volumes:
  cache_data:
```

## Troubleshooting Examples

### Fix Permission Issues

```bash
# Problem: Cache permission denied
# Solution: Fix cache directory permissions
sudo chown -R $USER:$USER ~/.cache/dociium
chmod -R 755 ~/.cache/dociium
```

### Clear Cache

```bash
# Problem: Stale cache data
# Solution: Clear cache directory
rm -rf ~/.cache/dociium
# Or use environment variable location
rm -rf $RDOCS_CACHE_DIR
```

### Network Issues

```bash
# Problem: Cannot reach docs.rs
# Solution: Test connectivity and set proxy if needed
curl -I https://docs.rs

# If behind proxy, set environment variables
export HTTP_PROXY=http://proxy.company.com:8080
export HTTPS_PROXY=http://proxy.company.com:8080
export NO_PROXY=localhost,127.0.0.1
```

### Memory Issues

```bash
# Problem: High memory usage
# Solution: Monitor and limit cache size
du -sh ~/.cache/dociium

# Clear old cache entries
find ~/.cache/dociium -type f -mtime +30 -delete

# Set memory limits (if running as service)
systemd-run --scope -p MemoryLimit=512M dociium
```

## Performance Optimization Examples

### High-Performance Setup

```bash
# Install with maximum optimizations
RUSTFLAGS="-C target-cpu=native" cargo install --path . --release

# Use faster allocator
export MALLOC_CONF="background_thread:true,metadata_thp:auto"

# Optimize cache location (SSD recommended)
export RDOCS_CACHE_DIR="/fast/ssd/path/dociium"

# Pre-warm cache for common crates
echo 'Pre-warming cache...'
curl -X POST http://localhost:8080/tools/call \
  -d '{"name":"get_item_doc","arguments":{"crate_name":"tokio","path":"tokio::sync::Mutex"}}'
```

### Batch Operations

```bash
# Cache multiple popular crates
crates=("tokio" "serde" "reqwest" "clap" "anyhow")

for crate in "${crates[@]}"; do
  echo "Caching $crate..."
  # Use your MCP client to call get_item_doc for common items
done
```

## Monitoring Examples

### Basic Health Check

```bash
#!/bin/bash
# health_check.sh

# Check if dociium process is running
if pgrep -x "dociium" > /dev/null; then
    echo "✅ Dociium is running"
else
    echo "❌ Dociium is not running"
    exit 1
fi

# Check cache directory
if [ -d "$RDOCS_CACHE_DIR" ]; then
    echo "✅ Cache directory exists"
    echo "Cache size: $(du -sh $RDOCS_CACHE_DIR | cut -f1)"
else
    echo "⚠️  Cache directory not found"
fi
```

### Log Monitoring

```bash
# Follow logs with filtering
tail -f /var/log/dociium.log | grep -E "(ERROR|WARN|timeout)"

# Analyze performance metrics
grep "Successfully fetched docs" /var/log/dociium.log | \
  awk '{print $NF}' | \
  sort -n | \
  awk '{sum+=$1; count++} END {print "Average:", sum/count "ms"}'
```

## Update Examples

### Update Installation

```bash
# Pull latest changes
cd dociium
git pull origin main

# Reinstall with force flag
cargo install --path . --force

# Clear cache to ensure compatibility
rm -rf ~/.cache/dociium

# Restart service if running
sudo systemctl restart dociium
```

### Rollback

```bash
# If update causes issues, rollback to previous version
cd dociium
git checkout <previous-commit>
cargo install --path . --force
```

## Integration Testing

### Test Installation

```bash
#!/bin/bash
# test_installation.sh

echo "Testing dociium installation..."

# Test 1: Binary exists
if command -v dociium &> /dev/null; then
    echo "✅ dociium binary found"
else
    echo "❌ dociium binary not found"
    exit 1
fi

# Test 2: Cache directory creation
export RDOCS_CACHE_DIR="/tmp/dociium_test_cache"
mkdir -p "$RDOCS_CACHE_DIR"

# Test 3: Basic MCP communication
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | \
  timeout 10s dociium > /tmp/dociium_test.out 2>&1 &

sleep 2
if pgrep -x "dociium" > /dev/null; then
    echo "✅ dociium starts successfully"
    pkill dociium
else
    echo "❌ dociium failed to start"
    cat /tmp/dociium_test.out
    exit 1
fi

# Cleanup
rm -rf "$RDOCS_CACHE_DIR"
rm -f /tmp/dociium_test.out

echo "🎉 Installation test passed!"
```

This completes the comprehensive installation examples covering various scenarios, platforms, and use cases.