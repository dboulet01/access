import unittest

from docking_orchestration.session_gateway import SessionPool


class SessionPoolTests(unittest.TestCase):
    def test_assigns_unique_backends_and_keeps_sessions_sticky(self):
        pool = SessionPool(["one:8080", "two:8080"])

        first_id, first_backend, first_created = pool.acquire()
        second_id, second_backend, second_created = pool.acquire()
        repeated_id, repeated_backend, repeated_created = pool.acquire(first_id)

        self.assertNotEqual(first_id, second_id)
        self.assertNotEqual(first_backend, second_backend)
        self.assertTrue(first_created)
        self.assertTrue(second_created)
        self.assertEqual((repeated_id, repeated_backend), (first_id, first_backend))
        self.assertFalse(repeated_created)

    def test_rejects_new_session_when_pool_is_full(self):
        pool = SessionPool(["one:8080"])
        pool.acquire()

        session_id, backend, created = pool.acquire()

        self.assertIsNone(session_id)
        self.assertIsNone(backend)
        self.assertFalse(created)

    def test_reclaims_expired_session(self):
        pool = SessionPool(["one:8080"], idle_timeout_s=-1)
        first_id, _, _ = pool.acquire()

        second_id, backend, created = pool.acquire()

        self.assertNotEqual(first_id, second_id)
        self.assertEqual(backend, "one:8080")
        self.assertTrue(created)

    def test_status_excludes_expired_sessions(self):
        pool = SessionPool(["one:8080"], idle_timeout_s=-1)
        pool.acquire()

        self.assertEqual(pool.status()["active_sessions"], 0)

    def test_releases_failed_assignment(self):
        pool = SessionPool(["one:8080"])
        session_id, _, _ = pool.acquire()

        pool.release(session_id)
        replacement_id, backend, created = pool.acquire()

        self.assertNotEqual(session_id, replacement_id)
        self.assertEqual(backend, "one:8080")
        self.assertTrue(created)

    def test_reuses_client_id_after_gateway_restart(self):
        pool = SessionPool(["one:8080", "two:8080"])

        first_id, first_backend, first_created = pool.acquire("existing-client-id")
        repeated_id, repeated_backend, repeated_created = pool.acquire(
            "existing-client-id"
        )

        self.assertEqual(first_id, "existing-client-id")
        self.assertEqual((repeated_id, repeated_backend), (first_id, first_backend))
        self.assertTrue(first_created)
        self.assertFalse(repeated_created)


if __name__ == "__main__":
    unittest.main()