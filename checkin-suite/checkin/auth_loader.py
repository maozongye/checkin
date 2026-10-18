"""凭据加载：accounts.json 扁平多账号 + 本机桌面端登录态路径说明。

注意：本模块在 box/CI 上默认不访问用户真实本机登录态；
仅当显式传入 --from-desktop 或 load_desktop=True 时才尝试读取本地文件。
绝不在日志中打印真实 token。
"""

from __future__ import annotations

import json
import logging
import os
import platform
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

from .http_util import mask_secret
from .models import Account, Platform

logger = logging.getLogger(__name__)


def load_accounts(
    path: str,
    *,
    platform_filter: str = "all",
    load_desktop: bool = False,
) -> List[Account]:
    """从 accounts.json 加载账号；可选合并本机桌面端 WorkBuddy 登录态。"""
    accounts: List[Account] = []
    p = Path(path)
    if p.is_file():
        raw = json.loads(p.read_text(encoding="utf-8"))
        items = raw.get("accounts") if isinstance(raw, dict) else raw
        if not isinstance(items, list):
            raise ValueError(f"accounts 文件格式错误，期望 list 或 {{accounts:[...]}}: {path}")
        for item in items:
            if not isinstance(item, dict):
                continue
            acc = _parse_account_item(item)
            if acc is None:
                continue
            if not _platform_match(acc.platform, platform_filter):
                continue
            accounts.append(acc)
            logger.info(
                "加载账号 platform=%s id=%s nick=%s access=%s",
                acc.platform,
                acc.account_id,
                acc.nickname,
                mask_secret(acc.access_token),
            )
    elif path:
        logger.warning("accounts 文件不存在: %s", path)

    if load_desktop:
        for acc in load_workbuddy_desktop_auth():
            if _platform_match(acc.platform, platform_filter):
                # 避免与 JSON 重复
                if any(
                    a.platform == acc.platform and a.account_id == acc.account_id
                    for a in accounts
                ):
                    continue
                accounts.append(acc)

    return accounts


def _platform_match(plat: str, filt: str) -> bool:
    f = (filt or "all").lower()
    if f in ("", "all"):
        return True
    p = plat.lower()
    if f == "workbuddy":
        return p in ("workbuddy", "buddy", "codebuddy")
    if f == "trae":
        return p in ("trae", "trae_cn", "traecode")
    return p == f


def _parse_account_item(item: Dict[str, Any]) -> Optional[Account]:
    plat = str(item.get("platform") or "").strip().lower()
    if not plat:
        # 根据字段推断
        if item.get("deviceId") or item.get("device_id"):
            plat = Platform.TRAE.value
        elif item.get("uid") or item.get("domain"):
            plat = Platform.WORKBUDDY.value
        else:
            logger.warning("跳过无法识别平台的账号项")
            return None

    if plat in ("buddy", "codebuddy", "workbuddy-cn"):
        plat = Platform.WORKBUDDY.value
    if plat in ("trae_cn", "traecode", "trae-cn"):
        plat = Platform.TRAE.value

    access = (
        item.get("accessToken")
        or item.get("access_token")
        or item.get("token")
        or ""
    )
    refresh = (
        item.get("refreshToken")
        or item.get("refresh_token")
        or ""
    )
    uid = str(item.get("uid") or item.get("user_id") or item.get("userId") or "")
    account_id = str(
        item.get("account_id")
        or item.get("id")
        or uid
        or item.get("nickname")
        or item.get("email")
        or ""
    )
    nickname = str(item.get("nickname") or item.get("name") or account_id)
    device_id = str(item.get("deviceId") or item.get("device_id") or "")
    domain = str(item.get("domain") or item.get("base_url") or item.get("baseUrl") or "")
    client_id = str(item.get("clientId") or item.get("client_id") or "")
    enabled = item.get("enabled", True)
    if isinstance(enabled, str):
        enabled = enabled.lower() not in ("0", "false", "no")

    extras = {k: v for k, v in item.items() if k not in {
        "platform", "account_id", "id", "accessToken", "access_token", "token",
        "refreshToken", "refresh_token", "uid", "user_id", "userId", "nickname",
        "name", "email", "deviceId", "device_id", "domain", "base_url", "baseUrl",
        "clientId", "client_id", "enabled",
    }}

    return Account(
        platform=plat,
        account_id=account_id or nickname or "unknown",
        access_token=str(access),
        refresh_token=str(refresh),
        nickname=nickname,
        uid=uid,
        device_id=device_id,
        domain=domain,
        client_id=client_id,
        enabled=bool(enabled),
        extras=extras,
    )


def workbuddy_desktop_auth_path() -> Optional[Path]:
    """返回本机 WorkBuddy 桌面端登录态文件路径（若可推断）。"""
    system = platform.system()
    if system == "Windows":
        local = os.environ.get("LOCALAPPDATA", "")
        if local:
            return Path(local) / "CodeBuddyExtension" / "Data" / "Public" / "auth" / "workbuddy-desktop.info"
    elif system == "Darwin":
        home = Path.home()
        return (
            home
            / "Library"
            / "Application Support"
            / "CodeBuddyExtension"
            / "Data"
            / "Public"
            / "auth"
            / "workbuddy-desktop.info"
        )
    else:
        # Linux：常见 Electron 路径（非官方保证）
        xdg = os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))
        return Path(xdg) / "CodeBuddyExtension" / "Data" / "Public" / "auth" / "workbuddy-desktop.info"
    return None


def load_workbuddy_desktop_auth() -> List[Account]:
    """从桌面端 workbuddy-desktop.info 加载账号。

    结构::
        { account: {uid, nickname}, auth: {accessToken, refreshToken, expiresAt, domain} }

    若文件不存在则返回空列表（不报错），便于 box/CI 环境。
    """
    path = workbuddy_desktop_auth_path()
    if path is None or not path.is_file():
        logger.info("未找到 WorkBuddy 桌面端登录态文件（跳过）: %s", path)
        return []
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        logger.warning("读取桌面端登录态失败: %s", e)
        return []

    account_obj = raw.get("account") or {}
    auth_obj = raw.get("auth") or {}
    # 也兼容顶层扁平
    if not auth_obj and raw.get("accessToken"):
        auth_obj = raw
        account_obj = raw.get("account") or {
            "uid": raw.get("uid"),
            "nickname": raw.get("nickname"),
        }

    uid = str(account_obj.get("uid") or "")
    nickname = str(account_obj.get("nickname") or uid)
    access = auth_obj.get("accessToken") or auth_obj.get("access_token") or ""
    refresh = auth_obj.get("refreshToken") or auth_obj.get("refresh_token") or ""
    domain = str(auth_obj.get("domain") or "")

    if not access:
        logger.warning("桌面端登录态无 accessToken")
        return []

    logger.info(
        "从桌面端加载 WorkBuddy uid=%s nick=%s access=%s domain=%s",
        uid,
        nickname,
        mask_secret(str(access)),
        domain,
    )
    return [
        Account(
            platform=Platform.WORKBUDDY.value,
            account_id=uid or nickname,
            access_token=str(access),
            refresh_token=str(refresh),
            nickname=nickname,
            uid=uid,
            domain=domain,
            enabled=True,
        )
    ]


def describe_desktop_paths() -> Dict[str, str]:
    """供 README/CLI 打印的桌面端路径说明（不读文件、不回传 token）。

    Windows 相对路径 = 本机已验证默认（Architect 探测）；仍可配置覆盖。
    """
    return {
        "workbuddy_windows": r"%LOCALAPPDATA%\CodeBuddyExtension\Data\Public\auth\workbuddy-desktop.info",
        "workbuddy_windows_status": "本机已验证默认",
        "workbuddy_macos": "~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info",
        "workbuddy_macos_status": "候选·未拍板",
        "trae_windows": r"%APPDATA%\Trae CN\User\globalStorage\storage.json",
        "trae_windows_status": "本机已验证默认",
        "trae_auth_key_hint": "iCubeAuthInfo://icube.cloudide",
        "trae_device_id_key": "telemetry.devDeviceId",
        "trae_note": (
            "v1：优先读本机已验证路径并托管；允许后续导入。"
            "deviceId 配对 telemetry.devDeviceId。"
            "storage 可能加密时请用 Switcher / 手工将字段填入 accounts.json；"
            "本脚本不把 token 写入日志。"
        ),
        "scope": "workbuddy_desktop+trae_cn; daily-checkin-only; windows-first",
    }
