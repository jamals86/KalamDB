#!/bin/bash
#
# macOS TCP tuning for KalamDB testing and development
#
# This script tunes macOS network settings to handle high connection loads
# from test suites. The OS error "Can't assign requested address" (error 49)
# indicates ephemeral port exhaustion.
#
# **Problem**: macOS defaults limit concurrent connections:
# - Only ~16K ephemeral ports (49152-65535)
# - Connections stay in TIME_WAIT for 30 seconds
# - System listen queue limited to 128 connections
#
# **Solution**: Expand port range, reduce TIME_WAIT, increase listen queue
#
# **Usage**:
#   sudo ./scripts/tune-macos-tcp.sh
#
# **Revert**: Settings reset after reboot, or run:
#   sudo ./scripts/tune-macos-tcp.sh --revert

set -e

echo "🔧 KalamDB macOS TCP Tuning Script"
echo "=================================="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "❌ Error: This script must be run as root (use sudo)"
    echo "   sudo $0"
    exit 1
fi

# Backup current settings
echo "📋 Current settings:"
echo "  kern.ipc.somaxconn:           $(sysctl -n kern.ipc.somaxconn)"
echo "  net.inet.tcp.msl:             $(sysctl -n net.inet.tcp.msl) (TIME_WAIT = 2*MSL = $((2 * $(sysctl -n net.inet.tcp.msl) / 1000))s)"
echo "  net.inet.ip.portrange.first:  $(sysctl -n net.inet.ip.portrange.first)"
echo "  net.inet.ip.portrange.last:   $(sysctl -n net.inet.ip.portrange.last)"
echo "  Available ephemeral ports:    $(($(sysctl -n net.inet.ip.portrange.last) - $(sysctl -n net.inet.ip.portrange.first)))"
echo ""

if [ "$1" = "--revert" ]; then
    echo "🔄 Reverting to macOS defaults..."
    sysctl kern.ipc.somaxconn=128
    sysctl net.inet.tcp.msl=15000
    sysctl net.inet.ip.portrange.first=49152
    sysctl net.inet.ip.portrange.hifirst=49152
    echo ""
    echo "✅ Reverted to defaults (or reboot system)"
    exit 0
fi

echo "🚀 Applying optimized settings..."
echo ""

# Increase listen() backlog limit (allows more pending connections)
# Default: 128, Recommended: 4096-8192
echo "1️⃣  Increasing listen backlog: 128 → 4096"
sysctl kern.ipc.somaxconn=4096

# Reduce TIME_WAIT duration (frees up ports faster)
# Default: 15000ms (30s TIME_WAIT), Recommended: 1000ms (2s TIME_WAIT)
# This allows ports to be reused 15x faster!
echo "2️⃣  Reducing TIME_WAIT duration: 30s → 2s"
sysctl net.inet.tcp.msl=1000

# Expand ephemeral port range (more ports available for outbound connections)
# Default: 49152-65535 (~16K ports), Recommended: 32768-65535 (~32K ports)
echo "3️⃣  Expanding ephemeral port range: 49152-65535 → 32768-65535"
sysctl net.inet.ip.portrange.first=32768
sysctl net.inet.ip.portrange.hifirst=32768

echo ""
echo "📋 New settings:"
echo "  kern.ipc.somaxconn:           $(sysctl -n kern.ipc.somaxconn)"
echo "  net.inet.tcp.msl:             $(sysctl -n net.inet.tcp.msl) (TIME_WAIT = 2*MSL = $((2 * $(sysctl -n net.inet.tcp.msl) / 1000))s)"
echo "  net.inet.ip.portrange.first:  $(sysctl -n net.inet.ip.portrange.first)"
echo "  net.inet.ip.portrange.last:   $(sysctl -n net.inet.ip.portrange.last)"
echo "  Available ephemeral ports:    $(($(sysctl -n net.inet.ip.portrange.last) - $(sysctl -n net.inet.ip.portrange.first)))"
echo ""

echo "✅ Tuning complete!"
echo ""
echo "📊 Connection capacity improved:"
echo "  • Ports:      16,384 → 32,768  (+100%)"
echo "  • TIME_WAIT:  30s → 2s         (-93%)"
echo "  • Backlog:    128 → 4,096      (+3,100%)"
echo ""
echo "🔢 Theoretical connection rate:"
echo "  Before: ~546 new connections/sec"
echo "  After:  ~16,384 new connections/sec (+3,000%)"
echo ""
echo "💡 These settings are temporary and will reset after reboot."
echo "   To make permanent, add to /etc/sysctl.conf (advanced users only)"
echo ""
echo "🧪 Now run your tests: cd cli && cargo nextest run --features e2e-tests"
