#!/usr/bin/env python3
"""手动触发：一次性多账号签到 CLI。

用法::
    python scripts/checkin_once.py --platform all
    python scripts/checkin_once.py --platform workbuddy --accounts accounts.json
    python scripts/checkin_once.py --platform trae --dry-run --json
"""

from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

# 保证以脚本方式运行时可 import checkin
_ROOT = Path(__file__).resolve().parents[1]
if str(_ROOT) not in sys.path:
    sys.path.insert(0, str(_ROOT))

from checkin.runner import run_all, summary_to_json  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="WorkBuddy + Trae CN 手动签到")
    parser.add_argument(
        "--platform",
        choices=["all", "workbuddy", "trae"],
        default="all",
        help="签到平台（默认 all）",
    )
    parser.add_argument(
        "--accounts",
        default="accounts.json",
        help="账号配置文件路径（默认 ./accounts.json）",
    )
    parser.add_argument(
        "--config",
        default="config.json",
        help="通知/平台配置文件路径（默认 ./config.json，可不存在）",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="演练模式：不发真实签到请求",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        dest="as_json",
        help="以 JSON 输出完整结果",
    )
    parser.add_argument(
        "--no-notify",
        action="store_true",
        help="跳过通知与本地日志",
    )
    parser.add_argument(
        "--from-desktop",
        action="store_true",
        help="尝试合并本机 WorkBuddy 桌面端登录态（box 上通常无）",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="调试日志",
    )
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    accounts_path = args.accounts
    if not Path(accounts_path).is_file() and not args.from_desktop:
        # 尝试相对仓库根
        alt = _ROOT / accounts_path
        if alt.is_file():
            accounts_path = str(alt)
        else:
            example = _ROOT / "accounts.example.json"
            print(
                f"未找到账号文件: {args.accounts}\n"
                f"请复制 {example} 为 accounts.json 并填入凭据后重试。",
                file=sys.stderr,
            )
            return 2

    config_path = args.config
    if not Path(config_path).is_file():
        alt = _ROOT / config_path
        config_path = str(alt) if alt.is_file() else ""

    summary = run_all(
        accounts_path=accounts_path,
        platform=args.platform,
        config_path=config_path,
        dry_run=args.dry_run,
        notify=not args.no_notify,
        load_desktop=args.from_desktop,
    )

    if args.as_json:
        print(summary_to_json(summary))
    else:
        print(summary.human_line())
        for r in summary.results:
            print(
                f"  - [{r.platform}] {r.nickname or r.account_id}: "
                f"{r.status} | {r.message}"
                + (f" | +{r.credits_delta}" if r.credits_delta is not None else "")
                + (f" | 连签{r.streak_days}" if r.streak_days is not None else "")
            )

    failed = sum(1 for r in summary.results if r.status == "failed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
