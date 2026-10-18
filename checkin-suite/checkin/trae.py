"""Trae CN / TRAE 签到客户端。

契约摘要：
- UgHost:    https://api.trae.cn
- OAuthHost: https://api.trae.com.cn
- 状态: POST /trae/api/v2/ug/checkin_credits/status  body {"req_source":1}
- 领取: POST /trae/api/v2/ug/checkin_credits/claim   同 body
- Auth: Authorization: Cloud-IDE-JWT <accessToken>（前缀是值的一部分）
- 必需头: x-device-id（claim 必须是客户端真实注册过的设备号，否则 9004）
- 建议: x-device-type: windows；只发小写 x-device-id
- 业务码: 0 成功；9095 今日已签；9074 限流可重试；1001/1002/401/403 鉴权失败
- Token 续期: ExchangeToken（ClientID 默认 ono9krqynydwx5）
- 强制直连：关闭系统代理，多账号共用代理 IP 易触发 9074
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Dict, Optional

from .http_util import HttpError, mask_secret, request_json
from .models import Account, CheckinResult, CheckinStatus, Platform

logger = logging.getLogger(__name__)

UG_HOST = "https://api.trae.cn"
OAUTH_HOST = "https://api.trae.com.cn"
DEFAULT_CLIENT_ID = "ono9krqynydwx5"
LEGACY_CLIENT_ID = "en1oxy7wnw8j9n"

CODE_OK = 0
CODE_ALREADY = 9095
CODE_RATE_LIMIT = 9074
CODE_BAD_DEVICE = 9004
AUTH_FAIL_CODES = {1001, 1002, 401, 403}


class TraeClient:
    """Trae CN 签到客户端。"""

    def __init__(
        self,
        account: Account,
        *,
        ug_host: str = UG_HOST,
        oauth_host: str = OAUTH_HOST,
        force_direct: bool = True,
        dry_run: bool = False,
        accounts_path: str = "",
    ) -> None:
        self.account = account
        self.ug_host = ug_host.rstrip("/")
        self.oauth_host = oauth_host.rstrip("/")
        self.force_direct = force_direct
        self.dry_run = dry_run
        self.accounts_path = accounts_path
        self.client_id = account.client_id or DEFAULT_CLIENT_ID
        self._refresh_attempted = False

    def _headers(self) -> Dict[str, str]:
        device_id = self.account.device_id or ""
        token = self.account.access_token or ""
        # Authorization 值本身以 Cloud-IDE-JWT 开头
        if token.startswith("Cloud-IDE-JWT "):
            auth = token
        else:
            auth = f"Cloud-IDE-JWT {token}"
        hdrs = {
            "Authorization": auth,
            "Content-Type": "application/json",
            "x-device-type": str(
                self.account.extras.get("device_type")
                or self.account.extras.get("x-device-type")
                or "windows"
            ),
        }
        if device_id:
            # 只发小写 x-device-id
            hdrs["x-device-id"] = device_id
        return hdrs

    def checkin(self) -> CheckinResult:
        name = self.account.display_name()
        aid = self.account.account_id or name

        if not self.account.enabled:
            return self._result(CheckinStatus.DISABLED, "账号已停用", aid, name)

        if not self.account.access_token:
            return self._result(
                CheckinStatus.FAILED,
                "客户端未登录/Token 过期：缺少 accessToken",
                aid,
                name,
            )

        if not self.account.device_id:
            return self._result(
                CheckinStatus.FAILED,
                "缺少 deviceId：claim 需要客户端真实注册过的设备号，否则 9004",
                aid,
                name,
            )

        if self.dry_run:
            return self._result(
                CheckinStatus.DRY_RUN,
                f"[dry-run] 将 POST {self.ug_host}/trae/api/v2/ug/checkin_credits/claim",
                aid,
                name,
            )

        body = {"req_source": 1}

        # 可选查状态（展示用）
        status_info = self._fetch_status_safe(body)

        try:
            http_status, parsed, _raw = request_json(
                "POST",
                f"{self.ug_host}/trae/api/v2/ug/checkin_credits/claim",
                headers=self._headers(),
                body=body,
                force_direct=self.force_direct,
                retry_on_business_codes={CODE_RATE_LIMIT},
            )
        except HttpError as e:
            if e.status in (401, 403):
                if not self._refresh_attempted and self._try_refresh():
                    self._refresh_attempted = True
                    return self.checkin()
                return self._result(
                    CheckinStatus.FAILED,
                    "客户端未登录/Token 过期",
                    aid,
                    name,
                )
            return self._result(CheckinStatus.FAILED, f"网络/HTTP 错误: {e}", aid, name)

        code = _extract_code(parsed)
        data = parsed.get("data") if isinstance(parsed.get("data"), dict) else parsed

        if code == CODE_ALREADY:
            return self._result(
                CheckinStatus.SKIPPED,
                "今日已签到，跳过",
                aid,
                name,
                streak_days=_pick_int(data, "streak_days", status_info)
                or _pick_int(data, "continuous_days", status_info),
                balance=_pick_int(data, "balance", status_info)
                or _pick_int(data, "credits", status_info),
                raw_code=code,
                today_checked_in=True,
            )

        if code == CODE_OK:
            delta = (
                _pick_int(data, "credits")
                or _pick_int(data, "credit")
                or _pick_int(data, "amount")
                or _pick_int(data, "checkin_credits")
            )
            streak = _pick_int(data, "streak_days") or _pick_int(data, "continuous_days")
            balance = self._fetch_balance_safe()
            return self._result(
                CheckinStatus.SUCCESS,
                "签到成功",
                aid,
                name,
                credits_delta=delta,
                streak_days=streak,
                balance=balance,
                raw_code=code,
                today_checked_in=True,
            )

        if code == CODE_BAD_DEVICE:
            return self._result(
                CheckinStatus.FAILED,
                "设备号无效(9004)：请使用客户端真实注册过的 x-device-id",
                aid,
                name,
                raw_code=code,
            )

        if code == CODE_RATE_LIMIT:
            return self._result(
                CheckinStatus.FAILED,
                "限流(9074)：请稍后重试或确保强制直连、账号错峰",
                aid,
                name,
                raw_code=code,
            )

        if code in AUTH_FAIL_CODES or http_status in (401, 403):
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
        return self._result(CheckinStatus.FAILED, str(msg), aid, name, raw_code=code)

    def _fetch_status_safe(self, body: Dict[str, Any]) -> Dict[str, Any]:
        try:
            _st, parsed, _ = request_json(
                "POST",
                f"{self.ug_host}/trae/api/v2/ug/checkin_credits/status",
                headers=self._headers(),
                body=body,
                force_direct=self.force_direct,
                max_retries=1,
            )
            data = parsed.get("data")
            return data if isinstance(data, dict) else parsed
        except Exception as e:
            logger.debug("Trae 查状态失败（忽略）: %s", e)
            return {}

    def _fetch_balance_safe(self) -> Optional[int]:
        """可选：查积分余额。"""
        try:
            _st, parsed, _ = request_json(
                "POST",
                f"{self.ug_host}/trae/api/v2/pay/user_current_entitlement_list",
                headers=self._headers(),
                body={"require_usage": True},
                force_direct=self.force_direct,
                max_retries=1,
            )
            data = parsed.get("data")
            if isinstance(data, dict):
                for key in ("credits", "balance", "remaining_credits", "total_credits"):
                    if key in data and data[key] is not None:
                        return int(data[key])
                # 列表形态
                items = data.get("entitlements") or data.get("list") or []
                if isinstance(items, list) and items:
                    first = items[0]
                    if isinstance(first, dict):
                        for key in ("remaining", "credits", "amount"):
                            if key in first and first[key] is not None:
                                return int(first[key])
        except Exception as e:
            logger.debug("Trae 查余额失败（忽略）: %s", e)
        return None

    def _try_refresh(self) -> bool:
        """ExchangeToken 续期；refreshToken 轮换需回写 accounts 文件。"""
        rt = self.account.refresh_token
        if not rt:
            logger.warning(
                "无法刷新 Trae token（无 refreshToken），account=%s access=%s",
                self.account.account_id,
                mask_secret(self.account.access_token),
            )
            return False

        # 常见 ExchangeToken 路径（按兼容性尝试）
        candidates = [
            f"{self.oauth_host}/cloudide/api/v3/oauth/ExchangeToken",
            f"{self.oauth_host}/api/v3/oauth/ExchangeToken",
            f"{self.oauth_host}/trae/api/v3/oauth/ExchangeToken",
        ]
        payload = {
            "ClientID": self.client_id,
            "client_id": self.client_id,
            "RefreshToken": rt,
            "refresh_token": rt,
            "GrantType": "refresh_token",
            "grant_type": "refresh_token",
        }

        last_err: Optional[Exception] = None
        for url in candidates:
            try:
                _st, parsed, _ = request_json(
                    "POST",
                    url,
                    headers={"Content-Type": "application/json"},
                    body=payload,
                    force_direct=self.force_direct,
                    max_retries=1,
                )
            except HttpError as e:
                last_err = e
                continue

            data = parsed.get("data") if isinstance(parsed.get("data"), dict) else parsed
            # 嵌套 Token 结构
            token_obj = data.get("Token") or data.get("token") or data
            if not isinstance(token_obj, dict):
                token_obj = data if isinstance(data, dict) else {}

            new_access = (
                token_obj.get("AccessToken")
                or token_obj.get("accessToken")
                or token_obj.get("access_token")
                or data.get("AccessToken")
                or data.get("accessToken")
                or data.get("access_token")
            )
            new_refresh = (
                token_obj.get("RefreshToken")
                or token_obj.get("refreshToken")
                or token_obj.get("refresh_token")
                or data.get("RefreshToken")
                or data.get("refreshToken")
                or data.get("refresh_token")
            )
            if new_access:
                self.account.access_token = str(new_access)
                if new_refresh:
                    self.account.refresh_token = str(new_refresh)
                    self._persist_tokens()
                logger.info(
                    "Trae token 已刷新 account=%s access=%s",
                    self.account.account_id,
                    mask_secret(self.account.access_token),
                )
                return True

        # 尝试旧 ClientID（仅一次）
        if self.client_id != LEGACY_CLIENT_ID:
            self.client_id = LEGACY_CLIENT_ID
            return self._try_refresh()

        logger.warning("Trae token 刷新失败: %s", last_err)
        return False

    def _persist_tokens(self) -> None:
        """refreshToken 轮换后回写 accounts 文件（仅更新对应账号）。"""
        path = self.accounts_path
        if not path:
            return
        p = Path(path)
        if not p.is_file():
            return
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            logger.warning("回写 accounts 失败（读取）: %s", e)
            return

        accounts = data.get("accounts") if isinstance(data, dict) else None
        if not isinstance(accounts, list):
            if isinstance(data, list):
                accounts = data
            else:
                return

        aid = self.account.account_id
        for item in accounts:
            if not isinstance(item, dict):
                continue
            item_id = str(
                item.get("account_id")
                or item.get("id")
                or item.get("uid")
                or item.get("user_id")
                or ""
            )
            plat = str(item.get("platform") or "").lower()
            if plat and plat not in ("trae", "trae_cn", "traecode"):
                continue
            if item_id == aid or (
                self.account.nickname and item.get("nickname") == self.account.nickname
            ):
                item["accessToken"] = self.account.access_token
                item["access_token"] = self.account.access_token
                item["refreshToken"] = self.account.refresh_token
                item["refresh_token"] = self.account.refresh_token
                break

        try:
            if isinstance(data, dict) and "accounts" in data:
                data["accounts"] = accounts
                p.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
            elif isinstance(data, list):
                p.write_text(json.dumps(accounts, ensure_ascii=False, indent=2), encoding="utf-8")
            logger.info("已回写 Trae refreshToken 到 %s（已脱敏日志）", p)
        except OSError as e:
            logger.warning("回写 accounts 失败（写入）: %s", e)

    def _result(
        self,
        status: CheckinStatus,
        message: str,
        account_id: str,
        nickname: str,
        **kwargs: Any,
    ) -> CheckinResult:
        return CheckinResult(
            platform=Platform.TRAE.value,
            account_id=account_id,
            nickname=nickname,
            status=status.value,
            message=message,
            **kwargs,
        )


def _extract_code(parsed: Dict[str, Any]) -> Optional[int]:
    if not parsed:
        return None
    for key in ("code", "err_code", "errno", "status_code"):
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
