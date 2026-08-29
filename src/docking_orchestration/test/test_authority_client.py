import json
import sys
import unittest

from docking_orchestration.authority_client import AuthorityClient


class AuthorityClientTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()