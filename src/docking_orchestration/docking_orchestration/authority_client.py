import json
import queue
import shlex
import subprocess
import threading


class AuthorityClient:
    def __init__(self, command, timeout_s=5.0):
        self._lock = threading.Lock()
        self._timeout_s = timeout_s
        arguments = shlex.split(command) if isinstance(command, str) else command
        self._process = subprocess.Popen(
            arguments,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

    def request(self, payload):
        with self._lock:
            if self._process.poll() is not None:
                raise RuntimeError("ACCESS authority process exited")
            self._process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
            self._process.stdin.flush()
            responses = queue.Queue(maxsize=1)
            reader = threading.Thread(
                target=lambda: responses.put(self._process.stdout.readline()),
                daemon=True,
            )
            reader.start()
            try:
                line = responses.get(timeout=self._timeout_s)
            except queue.Empty as error:
                self._process.kill()
                self._process.wait()
                raise RuntimeError("ACCESS authority response timed out") from error
            if not line:
                raise RuntimeError("ACCESS authority closed its response stream")
            response = json.loads(line)
            if not response.get("ok"):
                raise RuntimeError(response.get("error", "ACCESS authority rejected request"))
            return response["value"]

    def close(self):
        if self._process.poll() is None:
            self._process.terminate()
            try:
                self._process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
        self._process.stdin.close()
        self._process.stdout.close()