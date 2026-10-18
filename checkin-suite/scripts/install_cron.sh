#!/usr/bin/env bash
# Linux / macOS：安装 cron 定时任务（每天 09:10 北京时间）
# 用法：
#   chmod +x scripts/install_cron.sh
#   ./scripts/install_cron.sh
#   ./scripts/install_cron.sh 09:30
#
# 注意：cron 默认使用系统本地时区。若机器不是 Asia/Shanghai，
# 请在 crontab 行前加 CRON_TZ=Asia/Shanghai（部分 cron 支持），
# 或自行换算为本地时间。

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
AT_TIME="${1:-09:10}"
HH="${AT_TIME%%:*}"
MM="${AT_TIME##*:}"
PYTHON="${PYTHON:-python3}"
LOG_DIR="${ROOT}/logs"
mkdir -p "${LOG_DIR}"

CRON_LINE="${MM} ${HH} * * * cd \"${ROOT}\" && ${PYTHON} scripts/checkin_once.py --platform all --accounts accounts.json --config config.json >> \"${LOG_DIR}/cron.log\" 2>&1"

# 若支持 CRON_TZ，优先写入
MARKER="# checkin-suite-workbuddy-trae"
TMP="$(mktemp)"
crontab -l 2>/dev/null | grep -v "${MARKER}" | grep -v "checkin_once.py" > "${TMP}" || true
{
  echo "${MARKER}"
  echo "CRON_TZ=Asia/Shanghai"
  echo "${CRON_LINE}"
} >> "${TMP}"

crontab "${TMP}"
rm -f "${TMP}"

echo "已安装 cron："
echo "  时间: 每天 ${HH}:${MM}（CRON_TZ=Asia/Shanghai）"
echo "  命令: ${PYTHON} ${ROOT}/scripts/checkin_once.py"
echo "  日志: ${LOG_DIR}/cron.log"
echo "查看: crontab -l"
