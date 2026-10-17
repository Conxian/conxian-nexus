#!/usr/bin/env python3
"""Inert HTTP/WebSocket transport for disposable container initialization only.

This is not a chain node, proof source or synchronization test. It reports zero
height and emits no blocks, transactions or payment records. No secrets are used.
"""
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json


class FixtureHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.headers.get("Upgrade", "").lower() == "websocket":
            key = self.headers.get("Sec-WebSocket-Key", "")
            try:
                valid = len(base64.b64decode(key, validate=True)) == 16
            except ValueError:
                valid = False
            if not valid or self.headers.get("Sec-WebSocket-Version") != "13":
                self.send_error(400)
                return
            # RFC 6455 transport handshake only. Never emit fabricated events.
            accept = base64.b64encode(hashlib.sha1(
                (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode("ascii")
            ).digest()).decode("ascii")
            self.send_response(101)
            self.send_header("Upgrade", "websocket")
            self.send_header("Connection", "Upgrade")
            self.send_header("Sec-WebSocket-Accept", accept)
            self.end_headers()
            self.wfile.flush()
            self.close_connection = True
            try:
                while self.connection.recv(4096):
                    pass
            except OSError:
                pass
            return
        if self.path != "/v2/info":
            self.send_error(404)
            return
        body = json.dumps({"burn_block_height": 0, "stacks_tip_height": 0,
                           "fixture": "inert-container-transport"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)


if __name__ == "__main__":
    ThreadingHTTPServer(("0.0.0.0", 8080), FixtureHandler).serve_forever()
