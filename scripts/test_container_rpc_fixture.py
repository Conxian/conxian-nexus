"""Exercise the actual inert transport, including a real socket handshake."""
import base64
import http.client
from http.server import ThreadingHTTPServer
import json
import socket
import threading
import unittest

from container_rpc_fixture import FixtureHandler


class FixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
        cls.worker = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.worker.start()
        cls.port = cls.server.server_address[1]

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.worker.join(2)

    def test_info_is_explicitly_inert_and_uncached(self):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
        try:
            conn.request("GET", "/v2/info")
            result = conn.getresponse()
            self.assertEqual(result.status, 200)
            self.assertEqual(result.getheader("Cache-Control"), "no-store")
            self.assertEqual(json.loads(result.read()), {
                "burn_block_height": 0, "stacks_tip_height": 0,
                "fixture": "inert-container-transport"})
        finally:
            conn.close()

    def test_other_routes_are_unavailable(self):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
        try:
            conn.request("GET", "/v2/transactions")
            result = conn.getresponse()
            self.assertEqual(result.status, 404)
            result.read()
        finally:
            conn.close()

    def test_websocket_handshake_emits_no_synthetic_events(self):
        with socket.create_connection(("127.0.0.1", self.port), timeout=2) as conn:
            conn.sendall(("GET / HTTP/1.1\r\nHost: fixture\r\n"
                          "Upgrade: websocket\r\nConnection: Upgrade\r\n"
                          "Sec-WebSocket-Version: 13\r\n"
                          "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").encode())
            data = b""
            while b"\r\n\r\n" not in data:
                data += conn.recv(4096)
            headers, remainder = data.split(b"\r\n\r\n", 1)
            self.assertIn(b"101 Switching Protocols", headers)
            self.assertIn(b"s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", headers)
            self.assertEqual(remainder, b"")
            conn.settimeout(0.05)
            with self.assertRaises(socket.timeout):
                conn.recv(1)

    def test_invalid_handshakes_are_rejected(self):
        for key, version in [("!", "13"), (base64.b64encode(b"short").decode(), "13"),
                             ("dGhlIHNhbXBsZSBub25jZQ==", "12")]:
            with self.subTest(key=key, version=version):
                conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=2)
                try:
                    conn.request("GET", "/", headers={"Upgrade": "websocket",
                                 "Sec-WebSocket-Key": key, "Sec-WebSocket-Version": version})
                    result = conn.getresponse()
                    self.assertEqual(result.status, 400)
                    result.read()
                finally:
                    conn.close()


if __name__ == "__main__":
    unittest.main()
