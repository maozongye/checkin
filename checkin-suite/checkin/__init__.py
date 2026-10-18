"""WorkBuddy + Trae CN 每日签到套件。

可独立运行，也可被桌面端 import::

    from checkin.runner import run_all
"""

__version__ = "1.0.0"

from .models import Account, CheckinResult, Platform, CheckinStatus
from .runner import run_all

__all__ = [
    "Account",
    "CheckinResult",
    "Platform",
    "CheckinStatus",
    "run_all",
    "__version__",
]
