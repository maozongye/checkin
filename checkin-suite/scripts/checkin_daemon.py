#!/usr/bin/env python3
"""内置定时：每天在指定北京时间 HH:MM 执行签到。

用法::
    python scripts/checkin_daemon.py --at 09:10 --platform all
    python scripts/checkin_daemon.py --at 09:10 --once-if-missed

说明：
- 时区固定 Asia/Shanghai（UTC+8）
- 适合长期挂起的本地服务 / 后台进程
- 生产环境更推荐系统 cron / schtasks（见 install_*.sh/ps1）
"""

from __future__ import annotations

import argparse
import logging
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
if str(_ROOT) not in sys.path:
    sys.path.insert(0, str(_ROOT))

from checkin.runner import run_all  # noqa: E402

TZ_CN = timezone(timedelta(hours=8))
logger = logging.getLogger("checkin_daemon")


def parse_hhmm(text: str) -> tuple[int, int]:
    parts = text.strip().split(":")
    if len(parts) != 2:
        raise argparse.ArgumentTypeError("时间格式应为 HH:MM，例如 09:10")
    h, m = int(parts[0]), int(parts[1])
    if not (0 <= h <= 23 and 0 <= m <= 59):
        raise argparse.ArgumentTypeError("时间超出范围")
    return h, m


def seconds_until(h: int, m: int, now: datetime | None = None) -> float:
    now = now or datetime.now(TZ_CN)
    target = now.replace(hour=h, minute=m, second=0, microsecond=0)
    if target <= now:
        target = target + timedelta(days=1)
    return (target - now).total_seconds()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="签到内置定时守护进程（北京时间）")
    parser.add_argument("--at", required=True, type=parse_hhmm, help="每天触发时间 HH:MM（北京时间）")
    parser.add_argument("--platform", choices=["all", "workbuddy", "trae"], default="all")
    parser.add_argument("--accounts", default="accounts.json")
    parser.add_argument("--config", default="config.json")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--once-if-missed",
        action="store_true",
        help="若启动时已过今日触发点，先立即跑一次再进入循环",
    )
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    h, m = args.at
    accounts_path = args.accounts
    if not Path(accounts_path).is_file():
        alt = _ROOT / accounts_path
        if alt.is_file():
            accounts_path = str(alt)

    config_path = args.config
    if not Path(config_path).is_file():
        alt = _ROOT / config_path
        config_path = str(alt) if alt.is_file() else ""

    def do_run() -> None:
        logger.info("开始签到 platform=%s", args.platform)
        summary = run_all(
            accounts_path=accounts_path,
            platform=args.platform,
            config_path=config_path,
            dry_run=args.dry_run,
            notify=True,
        )
        logger.info("%s", summary.human_line())

    now = datetime.now(TZ_CN)
    today_target = now.replace(hour=h, minute=m, second=0, microsecond=0)
    if args.once_if_missed and now >= today_target:
        logger.info("已过今日 %02d:%02d，先执行一次补签到", h, m)
        do_run()

    logger.info("守护进程已启动，每天 %02d:%02d（Asia/Shanghai）触发", h, m)
    while True:
        wait = seconds_until(h, m)
        logger.info("距离下次签到还有 %.0f 秒", wait)
        # 分段 sleep，便于信号中断
        end = time.time() + wait
        while time.time() < end:
            time.sleep(min(60.0, end - time.time()))
        try:
            do_run()
        except Exception:
            logger.exception("定时签到失败，将在明日同一时间重试")
        # 防止同一分钟内重复触发
        time.sleep(61)


if __name__ == "__main__":
    raise SystemExit(main())
