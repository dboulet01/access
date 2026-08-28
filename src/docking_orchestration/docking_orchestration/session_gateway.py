import http.client
import json
import os
import secrets
import threading
import time
from http import cookies
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


SESSION_COOKIE = "docking_session"
HOP_BY_HOP_HEADERS = {
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
}


class SessionPool:
    def __init__(self, backends, idle_timeout_s=60):
        self.backends = backends
        self.idle_timeout_s = idle_timeout_s
        self.lock = threading.Lock()
        self.leases = {}

    def _expire(self, now):
        self.leases = {
            key: value
            for key, value in self.leases.items()
            if now - value[1] < self.idle_timeout_s
        }

    def acquire(self, session_id=None):
        now = time.monotonic()
        with self.lock:
            self._expire(now)
            if session_id in self.leases:
                backend, _ = self.leases[session_id]
                self.leases[session_id] = (backend, now)
                return session_id, backend, False

            leased_backends = {backend for backend, _ in self.leases.values()}
            backend = next(
                (candidate for candidate in self.backends if candidate not in leased_backends),
                None,
            )
            if backend is None:
                return None, None, False
            session_id = session_id or secrets.token_urlsafe(24)
            self.leases[session_id] = (backend, now)
            return session_id, backend, True

    def status(self):
        with self.lock:
            self._expire(time.monotonic())
            active = len(self.leases)
        return {"capacity": len(self.backends), "active_sessions": active}

    def release(self, session_id):
        with self.lock:
            self.leases.pop(session_id, None)


def handler_for(pool):
    class SessionGatewayHandler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def do_GET(self):
            if self.path == "/gateway/status":
                self._json_response(200, pool.status())
                return
            self._proxy()

        def do_POST(self):
            if self.path == "/gateway/release":
                session_id = self._session_id()
                if session_id:
                    pool.release(session_id)
                self._json_response(200, {"released": bool(session_id)})
                return
            self._proxy()

        def _session_id(self):
            jar = cookies.SimpleCookie(self.headers.get("Cookie", ""))
            morsel = jar.get(SESSION_COOKIE)
            return morsel.value if morsel else None

        def _proxy(self):
            needs_session = self.path == "/" or self.path.startswith("/api/")
            session_id = self._session_id()
            created = False
            if needs_session:
                session_id, backend, created = pool.acquire(session_id)
                if backend is None:
                    self._json_response(
                        503,
                        {"error": "All simulation sessions are in use", **pool.status()},
                    )
                    return
                if created and not self._prepare_backend(backend):
                    pool.release(session_id)
                    self._json_response(502, {"error": "Simulation session is starting"})
                    return
            else:
                backend = pool.backends[0]
                if session_id:
                    _, leased_backend, _ = pool.acquire(session_id)
                    backend = leased_backend or backend

            host, port = backend.rsplit(":", 1)
            content_length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(content_length) if content_length else None
            forwarded_headers = {
                key: value
                for key, value in self.headers.items()
                if key.lower() not in HOP_BY_HOP_HEADERS and key.lower() != "host"
            }
            forwarded_headers["Host"] = backend
            forwarded_headers["X-Forwarded-Host"] = self.headers.get("Host", "")
            forwarded_headers["X-Forwarded-Proto"] = self.headers.get(
                "X-Forwarded-Proto", "http"
            )

            connection = http.client.HTTPConnection(host, int(port), timeout=10)
            try:
                connection.request(self.command, self.path, body, forwarded_headers)
                response = connection.getresponse()
                payload = response.read()
            except (OSError, http.client.HTTPException):
                self._json_response(502, {"error": "Simulation session is starting"})
                return
            finally:
                connection.close()

            self.send_response(response.status)
            for key, value in response.getheaders():
                if key.lower() not in HOP_BY_HOP_HEADERS and key.lower() != "content-length":
                    self.send_header(key, value)
            if created:
                self.send_header(
                    "Set-Cookie",
                    f"{SESSION_COOKIE}={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age=1800",
                )
            self.send_header("X-Simulation-Session", session_id or "shared-static")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def _prepare_backend(self, backend):
            host, port = backend.rsplit(":", 1)
            connection = http.client.HTTPConnection(host, int(port), timeout=10)
            try:
                connection.request(
                    "POST",
                    "/api/prepare",
                    b"{}",
                    {"Content-Type": "application/json"},
                )
                response = connection.getresponse()
                response.read()
                return response.status == 202
            except (OSError, http.client.HTTPException):
                return False
            finally:
                connection.close()

        def _json_response(self, status, value):
            payload = json.dumps(value).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, _format, *_args):
            pass

    return SessionGatewayHandler


def main():
    backends = [
        value.strip()
        for value in os.environ.get(
            "SIMULATION_BACKENDS",
            "docking-visual-1:8080,docking-visual-2:8080,docking-visual-3:8080",
        ).split(",")
        if value.strip()
    ]
    idle_timeout_s = int(os.environ.get("SESSION_IDLE_TIMEOUT_S", "60"))
    port = int(os.environ.get("GATEWAY_PORT", "8080"))
    pool = SessionPool(backends, idle_timeout_s)
    server = ThreadingHTTPServer(("0.0.0.0", port), handler_for(pool))
    print(f"session gateway ready on :{port} with {len(backends)} slots", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()