"""多账号编排：按平台签到、幂等跳过、汇总与通知。"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Dict, List, Optional, Union

from .auth_loader import load_accounts
from .models import Account, CheckinResult, CheckinStatus, Platform, RunSummary
from .notify import dispatch_notifications, now_cn_iso
from .trae import TraeClient
from .workbuddy import WorkBuddyClient

logger = logging.getLogger(__name__)


def run_all(
    accounts: Optional[List[Account]] = None,
    *,
    accounts_path: str = "accounts.json",
    platform: str = "all",
    config: Optional[Dict[str, Any]] = None,
    config_path: str = "",
    dry_run: bool = False,
    notify: bool = True,
    load_desktop: bool = False,
) -> RunSummary:
    """批量签到入口，可供桌面端 ``from checkin.runner import run_all`` 调用。

    :param accounts: 直接传入账号列表；为 None 时从 accounts_path 加载
    :param platform: all | workbuddy | trae
    :param dry_run: 不发真实请求
    :param notify: 是否按配置发送通知/写本地日志
    """
    cfg = config or {}
    if config_path and Path(config_path).is_file():
        try:
            loaded = json.loads(Path(config_path).read_text(encoding="utf-8"))
            if isinstance(loaded, dict):
                # 文件配置为底，显式 config 覆盖
                merged = dict(loaded)
                merged.update(cfg)
                cfg = merged
        except (OSError, json.JSONDecodeError) as e:
            logger.warning("读取配置失败: %s", e)

    if accounts is None:
        accounts = load_accounts(
            accounts_path,
            platform_filter=platform,
            load_desktop=load_desktop,
        )
    else:
        # 过滤平台
        if platform and platform != "all":
            accounts = [
                a for a in accounts
                if a.platform == platform
                or (platform == "workbuddy" and a.platform in ("workbuddy", "buddy"))
                or (platform == "trae" and a.platform in ("trae", "trae_cn"))
            ]

    summary = RunSummary(started_at=now_cn_iso(), results=[])
    wb_base = (cfg.get("workbuddy") or {}).get("base_url", "")
    force_direct = cfg.get("force_direct", True)
    if isinstance(cfg.get("trae"), dict) and "force_direct" in cfg["trae"]:
        trae_direct = cfg["trae"]["force_direct"]
    else:
        trae_direct = force_direct

    for acc in accounts:
        if not acc.enabled:
            summary.results.append(
                CheckinResult(
                    platform=acc.platform,
                    account_id=acc.account_id,
                    nickname=acc.display_name(),
                    status=CheckinStatus.DISABLED.value,
                    message="账号 enabled=false",
                )
            )
            continue
        try:
            result = _checkin_one(
                acc,
                workbuddy_base=wb_base,
                force_direct=force_direct if acc.platform == Platform.WORKBUDDY.value else trae_direct,
                dry_run=dry_run,
                accounts_path=accounts_path,
                config=cfg,
            )
        except Exception as e:
            logger.exception("签到异常 platform=%s id=%s", acc.platform, acc.account_id)
            result = CheckinResult(
                platform=acc.platform,
                account_id=acc.account_id,
                nickname=acc.display_name(),
                status=CheckinStatus.FAILED.value,
                message=f"未捕获异常: {e}",
            )
        summary.results.append(result)

    summary.finished_at = now_cn_iso()

    if notify:
        notify_cfg = cfg.get("notify") if isinstance(cfg.get("notify"), dict) else cfg
        dispatch_notifications(summary, notify_cfg)

    return summary


def _checkin_one(
    acc: Account,
    *,
    workbuddy_base: str,
    force_direct: bool,
    dry_run: bool,
    accounts_path: str,
    config: Dict[str, Any],
) -> CheckinResult:
    plat = acc.platform.lower()
    if plat in (Platform.WORKBUDDY.value, "buddy", "codebuddy"):
        client = WorkBuddyClient(
            acc,
            base_url=workbuddy_base,
            force_direct=force_direct,
            dry_run=dry_run,
            accounts_path=accounts_path,
        )
        return client.checkin()
    if plat in (Platform.TRAE.value, "trae_cn", "traecode"):
        trae_cfg = config.get("trae") or {}
        client = TraeClient(
            acc,
            ug_host=trae_cfg.get("ug_host", "https://api.trae.cn"),
            oauth_host=trae_cfg.get("oauth_host", "https://api.trae.com.cn"),
            force_direct=force_direct,
            dry_run=dry_run,
            accounts_path=accounts_path,
        )
        return client.checkin()
    return CheckinResult(
        platform=acc.platform,
        account_id=acc.account_id,
        nickname=acc.display_name(),
        status=CheckinStatus.FAILED.value,
        message=f"未知平台: {acc.platform}",
    )


def summary_to_json(summary: RunSummary) -> str:
    return json.dumps(summary.to_dict(), ensure_ascii=False, indent=2)
