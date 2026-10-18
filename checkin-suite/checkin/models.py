"""数据模型：账号、签到结果、平台枚举等。"""

from __future__ import annotations

from dataclasses import dataclass, field, asdict
from enum import Enum
from typing import Any, Dict, List, Optional


class Platform(str, Enum):
    """签到平台。"""

    WORKBUDDY = "workbuddy"
    TRAE = "trae"
    ALL = "all"


class CheckinStatus(str, Enum):
    """单次签到结果状态。"""

    SUCCESS = "success"      # 本次新签成功
    SKIPPED = "skipped"      # 今日已签，幂等跳过
    FAILED = "failed"        # 失败（网络/业务码/未知）
    DISABLED = "disabled"    # 账号被标记不可用（会话失效等）
    DRY_RUN = "dry_run"      # 演练模式，未真正请求


@dataclass
class Account:
    """统一账号配置。

    WorkBuddy 字段：access_token, refresh_token, uid, nickname, domain
    Trae 字段：access_token, refresh_token, device_id, nickname, client_id
    """

    platform: str
    account_id: str
    access_token: str
    refresh_token: str = ""
    nickname: str = ""
    uid: str = ""
    device_id: str = ""
    domain: str = ""           # workbuddy: codebuddy.cn / copilot.tencent.com
    client_id: str = ""        # trae OAuth client id
    enabled: bool = True
    extras: Dict[str, Any] = field(default_factory=dict)

    def display_name(self) -> str:
        return self.nickname or self.account_id or self.uid or "unknown"


@dataclass
class CheckinResult:
    """统一签到结果，供汇总与通知使用。"""

    platform: str
    account_id: str
    nickname: str
    status: str                          # CheckinStatus 值
    message: str = ""
    credits_delta: Optional[int] = None  # 本次获得积分
    streak_days: Optional[int] = None    # 连签天数
    balance: Optional[int] = None        # 当前余额（若可查）
    raw_code: Optional[int] = None       # 业务码
    today_checked_in: Optional[bool] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @property
    def ok(self) -> bool:
        return self.status in (
            CheckinStatus.SUCCESS.value,
            CheckinStatus.SKIPPED.value,
            CheckinStatus.DRY_RUN.value,
        )


@dataclass
class RunSummary:
    """一次批量运行的汇总。"""

    results: List[CheckinResult] = field(default_factory=list)
    started_at: str = ""
    finished_at: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "total": len(self.results),
            "success": sum(1 for r in self.results if r.status == CheckinStatus.SUCCESS.value),
            "skipped": sum(1 for r in self.results if r.status == CheckinStatus.SKIPPED.value),
            "failed": sum(1 for r in self.results if r.status == CheckinStatus.FAILED.value),
            "disabled": sum(1 for r in self.results if r.status == CheckinStatus.DISABLED.value),
            "dry_run": sum(1 for r in self.results if r.status == CheckinStatus.DRY_RUN.value),
            "results": [r.to_dict() for r in self.results],
        }

    def human_line(self) -> str:
        """一行人话汇总，绝不含 token。"""
        d = self.to_dict()
        parts = [
            f"签到完成：共 {d['total']} 个账号",
            f"成功 {d['success']}",
            f"已签跳过 {d['skipped']}",
            f"失败 {d['failed']}",
            f"停用 {d['disabled']}",
        ]
        if d.get("dry_run"):
            parts.append(f"演练 {d['dry_run']}")
        detail_bits = []
        for r in self.results:
            name = r.nickname or r.account_id
            tag = f"{r.platform}/{name}"
            if r.status == CheckinStatus.SUCCESS.value:
                delta = f"+{r.credits_delta}" if r.credits_delta is not None else ""
                streak = f"连签{r.streak_days}天" if r.streak_days is not None else ""
                extra = " ".join(x for x in (delta, streak) if x)
                detail_bits.append(f"{tag}✓{extra}".rstrip())
            elif r.status == CheckinStatus.SKIPPED.value:
                detail_bits.append(f"{tag}○已签")
            elif r.status == CheckinStatus.DISABLED.value:
                detail_bits.append(f"{tag}⊗停用")
            elif r.status == CheckinStatus.DRY_RUN.value:
                detail_bits.append(f"{tag}◇演练")
            else:
                detail_bits.append(f"{tag}✗{r.message[:40]}")
        head = "，".join(parts)
        if detail_bits:
            return head + "｜" + "；".join(detail_bits)
        return head
