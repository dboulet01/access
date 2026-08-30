import json
import sys
import unittest

from docking_orchestration.authority_client import AuthorityClient


class AuthorityClientTests(unittest.TestCase):
    def test_rejects_empty_command(self):
        with self.assertRaisesRegex(ValueError, "command must not be empty"):
            AuthorityClient("")

    def test_rejects_invalid_deadline(self):
        for timeout_s in (0, -1, float("nan"), float("inf")):
            with self.subTest(timeout_s=timeout_s):
                with self.assertRaisesRegex(ValueError, "timeout must be finite"):
                    AuthorityClient([sys.executable], timeout_s=timeout_s)

    def test_returns_successful_json_response(self):
        response = json.dumps({"ok": True, "value": {"approved": True}})
        command = [sys.executable, "-u", "-c", f"print({response!r})"]
        client = AuthorityClient(command)
        try:
            self.assertEqual(client.request({"command": "test"}), {"approved": True})
        finally:
            client.close()

    def test_kills_authority_that_misses_deadline(self):
        command = [sys.executable, "-u", "-c", "import time; time.sleep(60)"]
        client = AuthorityClient(command, timeout_s=0.05)
        try:
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                client.request({"command": "test"})
            self.assertIsNotNone(client._process.poll())
        finally:
            client.close()

    def test_rejects_malformed_response(self):
        command = [sys.executable, "-u", "-c", "print('not-json')"]
        client = AuthorityClient(command)
        try:
            with self.assertRaises(json.JSONDecodeError):
                client.request({"command": "test"})
        finally:
            client.close()


if __name__ == "__main__":
    unittest.main()