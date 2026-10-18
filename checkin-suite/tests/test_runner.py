"""runner / auth_loader / notify 编排单测。"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from checkin.auth_loader import load_accounts, _parse_account_item
from checkin.models import Account, CheckinResult, CheckinStatus, RunSummary
from checkin.notify import write_local_log
from checkin.runner import run_all


class TestAuthLoader(unittest.TestCase):
    def test_load_from_json(self):
        data = {
            "accounts": [
                {
                    "platform": "workbuddy",
                    "account_id": "a1",
                    "uid": "u1",
                    "accessToken": "tok1",
                    "refreshToken": "rt1",
                    "nickname": "N1",
                },
                {
                    "platform": "trae",
                    "account_id": "t1",
                    "access_token": "tok2",
                    "deviceId": "dev1",
                    "enabled": True,
                },
            ]
        }
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "accounts.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            accs = load_accounts(str(path), platform_filter="all")
        self.assertEqual(len(accs), 2)
        self.assertEqual(accs[0].platform, "workbuddy")
        self.assertEqual(accs[0].access_token, "tok1")
        self.assertEqual(accs[1].device_id, "dev1")

    def test_platform_filter(self):
        data = {
            "accounts": [
                {"platform": "workbuddy", "account_id": "a", "accessToken": "x", "uid": "1"},
                {"platform": "trae", "account_id": "b", "accessToken": "y", "deviceId": "d"},
            ]
        }
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "a.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            only_wb = load_accounts(str(path), platform_filter="workbuddy")
            only_tr = load_accounts(str(path), platform_filter="trae")
        self.assertEqual(len(only_wb), 1)
        self.assertEqual(only_wb[0].platform, "workbuddy")
        self.assertEqual(len(only_tr), 1)

    def test_infer_platform(self):
        wb = _parse_account_item({"uid": "1", "accessToken": "t", "domain": "codebuddy.cn"})
        tr = _parse_account_item({"accessToken": "t", "deviceId": "d"})
        self.assertEqual(wb.platform, "workbuddy")
        self.assertEqual(tr.platform, "trae")


class TestRunner(unittest.TestCase):
    def test_run_all_multi_account_idempotent(self):
        accounts = [
            Account(
                platform="workbuddy",
                account_id="wb1",
                access_token="secret-token-SHOULD-NOT-APPEAR-xyz9",
                uid="1",
                nickname="WB",
                domain="codebuddy.cn",
            ),
            Account(
                platform="trae",
                account_id="tr1",
                access_token="secret-token-SHOULD-NOT-APPEAR-xyz9",
                device_id="dev",
                nickname="TR",
            ),
        ]

        def fake_wb_checkin(self):
            return CheckinResult(
                platform="workbuddy",
                account_id="wb1",
                nickname="WB",
                status=CheckinStatus.SKIPPED.value,
                message="今日已签到，跳过",
                raw_code=10001,
            )

        def fake_trae_checkin(self):
            return CheckinResult(
                platform="trae",
                account_id="tr1",
                nickname="TR",
                status=CheckinStatus.SUCCESS.value,
                message="签到成功",
                credits_delta=5,
                raw_code=0,
            )

        with patch("checkin.runner.WorkBuddyClient.checkin", fake_wb_checkin):
            with patch("checkin.runner.TraeClient.checkin", fake_trae_checkin):
                summary = run_all(accounts=accounts, notify=False, dry_run=False)

        self.assertEqual(len(summary.results), 2)
        d = summary.to_dict()
        self.assertEqual(d["skipped"], 1)
        self.assertEqual(d["success"], 1)
        self.assertEqual(d["failed"], 0)
        line = summary.human_line()
        self.assertIn("签到完成", line)
        self.assertNotIn("fake", line.lower())
        # 人话汇总不含 token
        self.assertNotIn(accounts[0].access_token, line)

    def test_disabled_account(self):
        accounts = [
            Account(
                platform="workbuddy",
                account_id="x",
                access_token="secret-token-SHOULD-NOT-APPEAR-xyz9",
                uid="1",
                enabled=False,
            )
        ]
        summary = run_all(accounts=accounts, notify=False)
        self.assertEqual(summary.results[0].status, CheckinStatus.DISABLED.value)

    def test_dry_run_path(self):
        accounts = [
            Account(
                platform="workbuddy",
                account_id="wb1",
                access_token="secret-token-SHOULD-NOT-APPEAR-xyz9",
                uid="1",
                domain="codebuddy.cn",
            )
        ]
        summary = run_all(accounts=accounts, notify=False, dry_run=True)
        self.assertEqual(summary.results[0].status, CheckinStatus.DRY_RUN.value)

    def test_importable(self):
        from checkin.runner import run_all as ra
        self.assertTrue(callable(ra))


class TestNotify(unittest.TestCase):
    def test_local_log_no_token(self):
        summary = RunSummary(
            results=[
                CheckinResult(
                    platform="workbuddy",
                    account_id="a",
                    nickname="n",
                    status="success",
                    message="ok",
                )
            ],
            started_at="t0",
            finished_at="t1",
        )
        with tempfile.TemporaryDirectory() as td:
            path = write_local_log(summary, log_dir=td)
            text = Path(path).read_text(encoding="utf-8")
        self.assertIn("success", text)
        self.assertNotIn("accessToken", text)
        self.assertNotIn("refreshToken", text)


if __name__ == "__main__":
    unittest.main()
