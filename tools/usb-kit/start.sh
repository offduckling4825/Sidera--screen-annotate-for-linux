#!/bin/bash
set -euo pipefail
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
fail() {
    echo "$1"
    if mkdir -p "$HERE/logs" 2>/dev/null; then
        printf '%s\n%s\n' "$(date -Iseconds)" "$1" > "$HERE/logs/bootstrap-check-$(date +%Y%m%d-%H%M%S)-$$.txt" || true
    fi
    exit 1
}
if ! command -v python3 >/dev/null 2>&1; then
    fail '缺少 Python 3，安装已停止。原版没有改变。'
fi
if ! python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)'; then
    fail '此工具需要 Python 3.8 或更新版本，原版没有改变。'
fi
if [ -f "$HERE/CONTROL-SHA256SUMS.txt" ]; then
    (cd "$HERE" && sha256sum --check --status CONTROL-SHA256SUMS.txt) || {
        fail '工具控制文件校验失败，请重新复制这套工具。原版仍可从原来的应用入口启动。'
    }
fi
python3 "$HERE/sidera_usb.py" "$@"
