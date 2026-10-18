"""基于 urllib 的 HTTP 封装：超时、重试、可选强制直连（不走系统代理）。"""

from __future__ import annotations

import json
import logging
import random
import time
import urllib.error
import urllib.request
from typing import Any, Dict, Optional, Tuple

logger = logging.getLogger(__name__)

# 默认超时（秒）
DEFAULT_TIMEOUT = 30
# 最大重试次数（不含首次）
DEFAULT_MAX_RETRIES = 3
# 指数退避基数（秒）
DEFAULT_BACKOFF_BASE = 1.0
# 可重试的 HTTP 状态码
RETRYABLE_HTTP = {408, 429, 500, 502, 503, 504}


class HttpError(Exception):
    """HTTP 层错误（含状态码与响应体摘要）。"""

    def __init__(
        self,
        message: str,
        status: Optional[int] = None,
        body: Optional[str] = None,
    ) -> None:
        super().__init__(message)
        self.status = status
        self.body = body or ""


def _build_opener(force_direct: bool) -> urllib.request.OpenerDirector:
    """构建 opener；force_direct=True 时清空代理，避免多账号共用代理 IP 触发限流。"""
    if force_direct:
        return urllib.request.build_opener(urllib.request.ProxyHandler({}))
    return urllib.request.build_opener()


def request_json(
    method: str,
    url: str,
    *,
    headers: Optional[Dict[str, str]] = None,
    body: Optional[Dict[str, Any]] = None,
    timeout: float = DEFAULT_TIMEOUT,
    max_retries: int = DEFAULT_MAX_RETRIES,
    backoff_base: float = DEFAULT_BACKOFF_BASE,
    force_direct: bool = True,
    retry_on_business_codes: Optional[set] = None,
) -> Tuple[int, Dict[str, Any], str]:
    """发起 JSON 请求，返回 (http_status, parsed_json, raw_text)。

    - 网络异常与可重试 HTTP 状态码做指数退避重试。
    - HTTP 非 2xx 时若 body 仍是 JSON，仍解析返回（WorkBuddy 可能 HTTP 400 + code=10001）。
    - retry_on_business_codes：若解析后业务码在此集合中，也触发重试（如 Trae 9074）。
    """
    retry_biz = retry_on_business_codes or set()
    hdrs = {"Accept": "application/json", "User-Agent": "checkin-suite/1.0"}
    if headers:
        hdrs.update(headers)

    data_bytes: Optional[bytes] = None
    if body is not None:
        data_bytes = json.dumps(body, ensure_ascii=False).encode("utf-8")
        hdrs.setdefault("Content-Type", "application/json")

    opener = _build_opener(force_direct)
    last_err: Optional[Exception] = None

    for attempt in range(max_retries + 1):
        req = urllib.request.Request(url, data=data_bytes, headers=hdrs, method=method.upper())
        try:
            with opener.open(req, timeout=timeout) as resp:
                raw = resp.read().decode("utf-8", errors="replace")
                status = getattr(resp, "status", None) or resp.getcode()
                parsed = _safe_json(raw)
                # 业务码可重试
                code = _biz_code(parsed)
                if code in retry_biz and attempt < max_retries:
                    _sleep_backoff(attempt, backoff_base)
                    continue
                return status, parsed, raw
        except urllib.error.HTTPError as e:
            raw = ""
            try:
                raw = e.read().decode("utf-8", errors="replace")
            except Exception:
                pass
            parsed = _safe_json(raw)
            status = e.code
            # WorkBuddy：HTTP 400 + body code=10001 仍是「已签」，交给上层按业务码判定
            code = _biz_code(parsed)
            if code in retry_biz and attempt < max_retries:
                _sleep_backoff(attempt, backoff_base)
                last_err = e
                continue
            if status in RETRYABLE_HTTP and attempt < max_retries:
                _sleep_backoff(attempt, backoff_base)
                last_err = e
                continue
            # 非重试：把 body 一并返回，让调用方按业务码处理
            if parsed or raw:
                return status, parsed, raw
            raise HttpError(f"HTTP {status}: {e.reason}", status=status, body=raw) from e
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            last_err = e
            if attempt < max_retries:
                logger.warning("网络异常，重试 %s/%s: %s", attempt + 1, max_retries, e)
                _sleep_backoff(attempt, backoff_base)
                continue
            raise HttpError(f"网络异常: {e}", status=None, body="") from e

    raise HttpError(f"重试耗尽: {last_err}", status=None, body="")


def _safe_json(raw: str) -> Dict[str, Any]:
    if not raw or not raw.strip():
        return {}
    try:
        obj = json.loads(raw)
        return obj if isinstance(obj, dict) else {"_data": obj}
    except json.JSONDecodeError:
        return {"_raw": raw[:500]}


def _biz_code(parsed: Dict[str, Any]) -> Optional[int]:
    """从常见响应结构提取业务码。"""
    if not parsed:
        return None
    for key in ("code", "err_code", "errno", "status_code"):
        if key in parsed and parsed[key] is not None:
            try:
                return int(parsed[key])
            except (TypeError, ValueError):
                pass
    # Trae 有时包在 data 外层
    data = parsed.get("data")
    if isinstance(data, dict):
        for key in ("code", "err_code"):
            if key in data and data[key] is not None:
                try:
                    return int(data[key])
                except (TypeError, ValueError):
                    pass
    return None


def _sleep_backoff(attempt: int, base: float) -> None:
    delay = base * (2 ** attempt) + random.uniform(0, 0.3)
    time.sleep(delay)


def mask_secret(value: str, keep: int = 0) -> str:
    """脱敏：不输出任何真实 token 字符。

    keep 参数保留仅为兼容调用方，实际一律返回固定占位符，
    避免日志/通知泄露凭据。
    """
    if not value:
        return "<empty>"
    return f"<redacted len={len(value)}>"
