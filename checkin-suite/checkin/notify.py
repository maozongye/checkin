"""签到结果上报：本地 JSON 日志 / 企业微信机器人 / PushPlus。

绝不在日志或 webhook 中写入真实 token。
"""

from __future__ import annotations

import json
import logging
import os
import urllib.error
import urllib.request
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any, Dict, List, Optional

from .models import CheckinResult, RunSummary

logger = logging.getLogger(__name__)

# 北京时间
TZ_CN = timezone(timedelta(hours=8))


def now_cn_iso() -> str:
    return datetime.now(TZ_CN).strftime("%Y-%m-%d %H:%M:%S %z")


def write_local_log(
    summary: RunSummary,
    log_dir: str = "logs",
) -> str:
    """写入本地 JSON 日志，返回文件路径。"""
    Path(log_dir).mkdir(parents=True, exist_ok=True)
    stamp = datetime.now(TZ_CN).strftime("%Y%m%d_%H%M%S")
    path = Path(log_dir) / f"checkin_{stamp}.json"
    payload = summary.to_dict()
    # 二次保险：确保没有 token 字段
    text = json.dumps(payload, ensure_ascii=False, indent=2)
    path.write_text(text, encoding="utf-8")
    logger.info("本地日志已写: %s", path)
    return str(path)


def notify_wecom(webhook_url: str, summary: RunSummary) -> bool:
    """企业微信群机器人 markdown 通知。"""
    if not webhook_url:
        return False
    line = summary.human_line()
    content = (
        f"### Buddy/Trae 签到\n"
        f"> 时间：{summary.finished_at or now_cn_iso()}\n"
        f"{line}"
    )
    body = {"msgtype": "markdown", "markdown": {"content": content}}
    return _post_json(webhook_url, body, label="企业微信")


def notify_pushplus(token: str, summary: RunSummary, topic: str = "") -> bool:
    """PushPlus 推送。"""
    if not token:
        return False
    title = "Buddy/Trae 每日签到"
    content = summary.human_line()
    body: Dict[str, Any] = {
        "token": token,
        "title": title,
        "content": content,
        "template": "txt",
    }
    if topic:
        body["topic"] = topic
    return _post_json("https://www.pushplus.plus/send", body, label="PushPlus")


def dispatch_notifications(
    summary: RunSummary,
    config: Optional[Dict[str, Any]] = None,
) -> Dict[str, bool]:
    """按配置分发通知。config 来自 config.json 的 notify 段。"""
    cfg = config or {}
    results: Dict[str, bool] = {}

    log_dir = cfg.get("log_dir", "logs")
    if cfg.get("local_log", True):
        try:
            write_local_log(summary, log_dir=log_dir)
            results["local_log"] = True
        except OSError as e:
            logger.error("本地日志失败: %s", e)
            results["local_log"] = False

    wecom = cfg.get("wecom_webhook") or os.environ.get("CHECKIN_WECOM_WEBHOOK", "")
    if wecom:
        results["wecom"] = notify_wecom(wecom, summary)

    push_token = cfg.get("pushplus_token") or os.environ.get("CHECKIN_PUSHPLUS_TOKEN", "")
    if push_token:
        results["pushplus"] = notify_pushplus(
            push_token, summary, topic=cfg.get("pushplus_topic", "")
        )

    return results


def _post_json(url: str, body: Dict[str, Any], label: str = "") -> bool:
    data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            ok = 200 <= resp.status < 300
            logger.info("%s 通知: HTTP %s", label or url, resp.status)
            return ok
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as e:
        logger.warning("%s 通知失败: %s", label or url, e)
        return False
