"""WorkBuddy（Buddy 加油站）签到客户端。

契约摘要：
- CN Base（本机已验证默认）: https://copilot.tencent.com
- 兼容备选: https://www.codebuddy.cn
- Global: https://www.workbuddy.ai — 不签到，跳过
- 本期仅 daily-checkin（不含旅行/补登）
- 查状态: POST /v2/billing/meter/checkin-activity-status  body {}
- 签到:   POST /v2/billing/meter/daily-checkin           body {}
- Headers: Authorization: Bearer <accessToken>, X-User-Id: <uid>
- 业务码: 0 成功；10001 今日已签 → 跳过成功；12153 会话失效 → disabled
- 幂等主路径：直接调 daily-checkin，以 0/10001 判定（today_checked_in 不可靠）
"""

from __future__ import annotations

import logging
from typing import Any, Dict, Optional, Tuple
from urllib.parse import urlparse

from .http_util import HttpError, mask_secret, request_json
from .models import Account, CheckinResult, CheckinStatus, Platform

logger = logging.getLogger(__name__)

DEFAULT_BASE = "https://copilot.tencent.com"  # 本机已验证默认（仍可配置）
ALT_BASE = "https://www.codebuddy.cn"  # 候选备选
GLOBAL_HOSTS = {"www.workbuddy.ai", "workbuddy.ai"}

CODE_OK = 0
CODE_ALREADY = 10001
CODE_SESSION_INVALID = 12153


def resolve_base(account: Account, config_base: str = "") -> Optional[str]:
    """根据账号 domain / 配置决定 base URL；Global 域名返回 None 表示跳过。"""
    domain = (account.domain or "").strip().lower()
    if not domain and account.extras.get("domain"):
        domain = str(account.extras["domain"]).strip().lower()

    # 从 domain 推断（按主机名映射到对应 URL，不依赖 DEFAULT/ALT 别名语义）
    if domain:
        host = domain.replace("https://", "").replace("http://", "").split("/")[0]
        if host in GLOBAL_HOSTS:
            return None
        if "copilot.tencent.com" in host:
            return "https://copilot.tencent.com"
        if "codebuddy.cn" in host:
            return "https://www.codebuddy.cn"
        # 若是完整 URL
        if domain.startswith("http"):
            return domain.rstrip("/")

    if config_base:
        return config_base.rstrip("/")
    return DEFAULT_BASE


class WorkBuddyClient:
    """WorkBuddy 签到客户端。"""

    def __init__(
        self,
        account: Account,
        *,
        base_url: str = "",
        force_direct: bool = True,
        dry_run: bool = False,
        accounts_path: str = "",
    ) -> None:
        self.account = account
        self.force_direct = force_direct
        self.dry_run = dry_run
        self.accounts_path = accounts_path
        resolved = resolve_base(account, base_url)
        self.base_url = resolved  # None → Global，应跳过
        self._disabled = False
        self._refresh_attempted = False

    def _headers(self) -> Dict[str, str]:
        uid = self.account.uid or self.account.account_id
        return {
            "Authorization": f"Bearer {self.account.access_token}",
            "X-User-Id": str(uid),
            "Content-Type": "application/json",
        }

    def checkin(self) -> CheckinResult:
        """执行签到（幂等主路径：直接 daily-checkin）。"""
        name = self.account.display_name()
        aid = self.account.account_id or self.account.uid

        if not self.account.enabled or self._disabled:
            return self._result(CheckinStatus.DISABLED, "账号已停用", aid, name)

        if self.base_url is None:
            return self._result(
                CheckinStatus.SKIPPED,
                "Global 站点不参与签到，已跳过",
                aid,
                name,
            )

        if not self.account.access_token:
            return self._result(
                CheckinStatus.FAILED,
                "客户端未登录/Token 过期：缺少 accessToken",
                aid,
                name,
            )

        if self.dry_run:
            return self._result(
                CheckinStatus.DRY_RUN,
                f"[dry-run] 将 POST {self.base_url}/v2/billing/meter/daily-checkin",
                aid,
                name,
            )

        # 可选：先查状态仅作展示（失败不影响签到）
        status_info = self._fetch_status_safe()

        try:
            http_status, parsed, _raw = request_json(
                "POST",
                f"{self.base_url}/v2/billing/meter/daily-checkin",
                headers=self._headers(),
                body={},
                force_direct=self.force_direct,
            )
        except HttpError as e:
            # Token 可能过期，尝试刷新后重试一次
            if e.status in (401, 403) or "token" in str(e).lower():
                if not self._refresh_attempted and self._try_refresh():
                    self._refresh_attempted = True
                    return self.checkin()
                return self._result(
                    CheckinStatus.FAILED,
                    "客户端未登录/Token 过期：刷新失败",
                    aid,
                    name,
                )
            else:
                return self._result(CheckinStatus.FAILED, f"网络/HTTP 错误: {e}", aid, name)

        code = _extract_code(parsed)
        data = parsed.get("data") if isinstance(parsed.get("data"), dict) else parsed

        if code == CODE_SESSION_INVALID:
            self._disabled = True
            return self._result(
                CheckinStatus.DISABLED,
                "会话失效(12153)，账号已标记 disabled",
                aid,
                name,
                raw_code=code,
            )

        if code == CODE_ALREADY:
            streak = _pick_int(data, "streak_days", status_info)
            credit = _pick_int(data, "today_credit", status_info) or _pick_int(
                data, "daily_credit", status_info
            )
            return self._result(
                CheckinStatus.SKIPPED,
                "今日已签到，跳过",
                aid,
                name,
                credits_delta=None,
                streak_days=streak,
                balance=_pick_int(data, "balance", status_info),
                raw_code=code,
                today_checked_in=True,
            )

        if code == CODE_OK:
            streak = _pick_int(data, "streak_days", status_info)
            delta = _pick_int(data, "today_credit", status_info) or _pick_int(
                data, "daily_credit", status_info
            )
            return self._result(
                CheckinStatus.SUCCESS,
                "签到成功",
                aid,
                name,
                credits_delta=delta,
                streak_days=streak,
                balance=_pick_int(data, "balance", status_info),
                raw_code=code,
                today_checked_in=True,
            )

        # 其它业务失败；若像鉴权问题再尝试 refresh
        if code in (401, 403) or http_status in (401, 403):
            if not self._refresh_attempted and self._try_refresh():
                self._refresh_attempted = True
                return self.checkin()
            return self._result(
                CheckinStatus.FAILED,
                "客户端未登录/Token 过期",
                aid,
                name,
                raw_code=code,
            )

        msg = parsed.get("message") or parsed.get("msg") or f"业务失败 code={code}"
        return self._result(
            CheckinStatus.FAILED,
            str(msg),
            aid,
            name,
            raw_code=code,
        )

    def _fetch_status_safe(self) -> Dict[str, Any]:
        """查签到活动状态，仅展示用；失败返回空 dict。"""
        try:
            _st, parsed, _ = request_json(
                "POST",
                f"{self.base_url}/v2/billing/meter/checkin-activity-status",
                headers=self._headers(),
                body={},
                force_direct=self.force_direct,
                max_retries=1,
            )
            data = parsed.get("data")
            return data if isinstance(data, dict) else parsed
        except Exception as e:
            logger.debug("查签到状态失败（忽略）: %s", e)
            return {}

    def _try_refresh(self) -> bool:
        """使用 refreshToken 刷新 accessToken。成功则更新 account 并可选回写。"""
        rt = self.account.refresh_token
        if not rt or not self.base_url:
            logger.warning(
                "无法刷新 WorkBuddy token（无 refreshToken），account=%s token=%s",
                self.account.account_id,
                mask_secret(self.account.access_token),
            )
            return False
        try:
            _st, parsed, _ = request_json(
                "POST",
                f"{self.base_url}/v2/plugin/auth/token/refresh",
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {self.account.access_token}",
                },
                body={"refreshToken": rt, "refresh_token": rt},
                force_direct=self.force_direct,
                max_retries=1,
            )
        except HttpError as e:
            logger.warning("WorkBuddy token 刷新失败: %s", e)
            return False

        code = _extract_code(parsed)
        if code not in (None, CODE_OK):
            if code == CODE_SESSION_INVALID:
                self._disabled = True
            logger.warning("WorkBuddy token 刷新业务失败 code=%s", code)
            return False

        data = parsed.get("data") if isinstance(parsed.get("data"), dict) else parsed
        new_access = (
            data.get("accessToken")
            or data.get("access_token")
            or parsed.get("accessToken")
            or parsed.get("access_token")
        )
        new_refresh = (
            data.get("refreshToken")
            or data.get("refresh_token")
            or parsed.get("refreshToken")
            or parsed.get("refresh_token")
        )
        if not new_access:
            return False
        self.account.access_token = str(new_access)
        if new_refresh:
            self.account.refresh_token = str(new_refresh)
        logger.info(
            "WorkBuddy token 已刷新 account=%s access=%s",
            self.account.account_id,
            mask_secret(self.account.access_token),
        )
        return True

    def _result(
        self,
        status: CheckinStatus,
        message: str,
        account_id: str,
        nickname: str,
        **kwargs: Any,
    ) -> CheckinResult:
        return CheckinResult(
            platform=Platform.WORKBUDDY.value,
            account_id=account_id,
            nickname=nickname,
            status=status.value,
            message=message,
            **kwargs,
        )


def _extract_code(parsed: Dict[str, Any]) -> Optional[int]:
    if not parsed:
        return None
    for key in ("code", "err_code", "errno"):
        if key in parsed and parsed[key] is not None:
            try:
                return int(parsed[key])
            except (TypeError, ValueError):
                pass
    return None


def _pick_int(data: Any, key: str, fallback: Optional[Dict] = None) -> Optional[int]:
    for src in (data, fallback or {}):
        if isinstance(src, dict) and key in src and src[key] is not None:
            try:
                return int(src[key])
            except (TypeError, ValueError):
                pass
    return None
