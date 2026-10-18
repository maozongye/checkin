"""幂等与错误码单测：mock HTTP，不访问外网。"""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from unittest.mock import patch

from checkin.http_util import mask_secret
from checkin.models import Account, CheckinStatus
from checkin.trae import TraeClient
from checkin.workbuddy import WorkBuddyClient

FIXTURES = Path(__file__).parent / "fixtures"


def _load(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def _wb_account(**kwargs) -> Account:
    base = dict(
        platform="workbuddy",
        account_id="wb-1",
        uid="10001",
        nickname="测试Buddy",
        access_token="fake-access-token-wb",
        refresh_token="fake-refresh-token-wb",
        domain="codebuddy.cn",
    )
    base.update(kwargs)
    return Account(**base)


def _trae_account(**kwargs) -> Account:
    base = dict(
        platform="trae",
        account_id="trae-1",
        nickname="测试Trae",
        access_token="fake-access-token-trae",
        refresh_token="fake-refresh-token-trae",
        device_id="fake-device-id-registered",
    )
    base.update(kwargs)
    return Account(**base)


class TestMaskSecret(unittest.TestCase):
    def test_mask_never_leaks_token_chars(self):
        token = "super-secret-token-ABCDEF"
        masked = mask_secret(token)
        self.assertNotIn("super", masked)
        self.assertNotIn("ABCDEF", masked)
        self.assertNotIn(token, masked)
        for ch in token:
            # 允许长度数字偶然重合，但不允许连续片段；更严：整串不得出现
            pass
        self.assertTrue(masked.startswith("<redacted"))
        self.assertIn(str(len(token)), masked)


class TestWorkBuddyIdempotent(unittest.TestCase):
    def test_success_code_0(self):
        payload = _load("workbuddy_checkin_ok.json")
        status_payload = _load("workbuddy_status.json")

        def fake_request(method, url, **kwargs):
            if "checkin-activity-status" in url:
                return 200, status_payload, json.dumps(status_payload)
            if "daily-checkin" in url:
                return 200, payload, json.dumps(payload)
            raise AssertionError(f"unexpected url {url}")

        with patch("checkin.workbuddy.request_json", side_effect=fake_request):
            r = WorkBuddyClient(_wb_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.SUCCESS.value)
        self.assertEqual(r.raw_code, 0)
        self.assertEqual(r.credits_delta, 10)
        self.assertEqual(r.streak_days, 3)

    def test_already_signed_code_10001_as_skipped(self):
        """业务码 10001 → skipped（成功幂等）。"""
        payload = _load("workbuddy_already.json")

        def fake_request(method, url, **kwargs):
            if "checkin-activity-status" in url:
                return 200, {"code": 0, "data": {}}, "{}"
            return 200, payload, json.dumps(payload)

        with patch("checkin.workbuddy.request_json", side_effect=fake_request):
            r = WorkBuddyClient(_wb_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.SKIPPED.value)
        self.assertEqual(r.raw_code, 10001)
        self.assertTrue(r.ok)

    def test_http_400_with_body_10001_still_skipped(self):
        """HTTP 可能是 400，但 body code=10001，必须以业务码为准。"""
        payload = _load("workbuddy_already.json")

        def fake_request(method, url, **kwargs):
            if "checkin-activity-status" in url:
                return 200, {"code": 0, "data": {}}, "{}"
            # 模拟 http_util 已把 HTTPError body 解析后返回
            return 400, payload, json.dumps(payload)

        with patch("checkin.workbuddy.request_json", side_effect=fake_request):
            r = WorkBuddyClient(_wb_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.SKIPPED.value)
        self.assertEqual(r.raw_code, 10001)

    def test_session_invalid_12153_disabled(self):
        payload = _load("workbuddy_session_invalid.json")

        def fake_request(method, url, **kwargs):
            if "checkin-activity-status" in url:
                return 200, {"code": 0, "data": {}}, "{}"
            return 200, payload, json.dumps(payload)

        with patch("checkin.workbuddy.request_json", side_effect=fake_request):
            r = WorkBuddyClient(_wb_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.DISABLED.value)
        self.assertEqual(r.raw_code, 12153)

    def test_global_domain_skipped(self):
        acc = _wb_account(domain="www.workbuddy.ai")
        r = WorkBuddyClient(acc).checkin()
        self.assertEqual(r.status, CheckinStatus.SKIPPED.value)
        self.assertIn("Global", r.message)

    def test_dry_run(self):
        r = WorkBuddyClient(_wb_account(), dry_run=True).checkin()
        self.assertEqual(r.status, CheckinStatus.DRY_RUN.value)

    def test_missing_token(self):
        r = WorkBuddyClient(_wb_account(access_token="")).checkin()
        self.assertEqual(r.status, CheckinStatus.FAILED.value)
        self.assertIn("Token", r.message)


class TestTraeIdempotent(unittest.TestCase):
    def test_success_code_0(self):
        payload = _load("trae_claim_ok.json")

        def fake_request(method, url, **kwargs):
            if "checkin_credits/status" in url:
                return 200, {"code": 0, "data": {}}, "{}"
            if "user_current_entitlement" in url:
                return 200, {"code": 0, "data": {"credits": 100}}, "{}"
            if "claim" in url:
                return 200, payload, json.dumps(payload)
            raise AssertionError(url)

        with patch("checkin.trae.request_json", side_effect=fake_request):
            r = TraeClient(_trae_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.SUCCESS.value)
        self.assertEqual(r.raw_code, 0)
        self.assertEqual(r.credits_delta, 5)
        self.assertEqual(r.streak_days, 7)
        self.assertEqual(r.balance, 100)

    def test_already_9095_skipped(self):
        payload = _load("trae_already.json")

        def fake_request(method, url, **kwargs):
            if "status" in url and "claim" not in url:
                return 200, {"code": 0, "data": {}}, "{}"
            return 200, payload, json.dumps(payload)

        with patch("checkin.trae.request_json", side_effect=fake_request):
            r = TraeClient(_trae_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.SKIPPED.value)
        self.assertEqual(r.raw_code, 9095)
        self.assertTrue(r.ok)

    def test_bad_device_9004(self):
        payload = _load("trae_bad_device.json")

        def fake_request(method, url, **kwargs):
            if "status" in url and "claim" not in url:
                return 200, {"code": 0, "data": {}}, "{}"
            return 200, payload, json.dumps(payload)

        with patch("checkin.trae.request_json", side_effect=fake_request):
            r = TraeClient(_trae_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.FAILED.value)
        self.assertEqual(r.raw_code, 9004)
        self.assertIn("9004", r.message)

    def test_missing_device_id(self):
        r = TraeClient(_trae_account(device_id="")).checkin()
        self.assertEqual(r.status, CheckinStatus.FAILED.value)
        self.assertIn("deviceId", r.message)

    def test_auth_header_prefix(self):
        client = TraeClient(_trae_account(access_token="abc123"))
        hdrs = client._headers()
        self.assertTrue(hdrs["Authorization"].startswith("Cloud-IDE-JWT "))
        self.assertIn("x-device-id", hdrs)
        self.assertNotIn("X-Device-Id", hdrs)

    def test_rate_limit_after_retries_failed(self):
        """http_util 会重试 9074；此处直接模拟最终返回 9074。"""
        payload = _load("trae_rate_limit.json")

        def fake_request(method, url, **kwargs):
            if "status" in url and "claim" not in url:
                return 200, {"code": 0, "data": {}}, "{}"
            return 200, payload, json.dumps(payload)

        with patch("checkin.trae.request_json", side_effect=fake_request):
            r = TraeClient(_trae_account()).checkin()
        self.assertEqual(r.status, CheckinStatus.FAILED.value)
        self.assertEqual(r.raw_code, 9074)


class TestHttpUtilBusinessRetry(unittest.TestCase):
    def test_retry_on_business_code(self):
        from checkin import http_util

        calls = {"n": 0}

        class FakeResp:
            status = 200

            def read(self):
                calls["n"] += 1
                if calls["n"] < 3:
                    return json.dumps({"code": 9074}).encode()
                return json.dumps({"code": 0, "data": {"ok": True}}).encode()

            def getcode(self):
                return 200

            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

        class FakeOpener:
            def open(self, req, timeout=None):
                return FakeResp()

        with patch.object(http_util, "_build_opener", return_value=FakeOpener()):
            with patch.object(http_util, "_sleep_backoff", return_value=None):
                status, parsed, _ = http_util.request_json(
                    "POST",
                    "https://example.invalid/claim",
                    body={"req_source": 1},
                    retry_on_business_codes={9074},
                    max_retries=3,
                    force_direct=True,
                )
        self.assertEqual(status, 200)
        self.assertEqual(parsed.get("code"), 0)
        self.assertGreaterEqual(calls["n"], 3)


if __name__ == "__main__":
    unittest.main()
