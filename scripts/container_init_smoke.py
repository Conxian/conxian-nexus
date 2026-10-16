#!/usr/bin/env python3
"""Exercise an already-built image with disposable, authenticated dependencies.

No production credentials, chain synchronization or payment acceptance is tested.
Docker resources created by this run are removed on both success and failure.
"""
import argparse
import json
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.request

CURRENT_PHASE = "setup"


def docker(*args, timeout=60):
    result = subprocess.run(
        ["docker", *args], capture_output=True, text=True, timeout=timeout
    )
    if result.returncode:
        # Commands and backend output may contain generated credentials.
        raise RuntimeError("Docker operation failed: " + args[0])
    return result.stdout.strip()


def response(base, path):
    try:
        with urllib.request.urlopen(base + path, timeout=4) as result:
            return result.status, result.read(65537)
    except urllib.error.HTTPError as error:
        return error.code, error.read(65537)


def wait_for(base, path, status, expected=None):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            code, body = response(base, path)
            if code == status and len(body) <= 65536:
                if expected is None or json.loads(body).get("status") == expected:
                    return
        except (OSError, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError("HTTP initialization assertion failed: " + path)


def main():
    global CURRENT_PHASE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    args = parser.parse_args()
    prefix = "nexus-init-" + secrets.token_hex(6)
    password = secrets.token_hex(32)
    pg, redis, app, rpc = (prefix + suffix for suffix in ("-pg", "-redis", "-app", "-rpc"))
    created = []
    docker("network", "create", prefix)
    try:
        CURRENT_PHASE = "dependency-start"
        for name, command in [
            (pg, ["--env", "POSTGRES_USER=nexus", "--env", "POSTGRES_DB=nexus",
                  "--env", "POSTGRES_PASSWORD=" + password, "postgres:16-bookworm"]),
            (redis, ["redis:7.4-bookworm", "redis-server", "--requirepass", password]),
        ]:
            docker("run", "--detach", "--name", name, "--network", prefix, *command)
            created.append(name)
        deadline = time.monotonic() + 60
        while True:
            try:
                docker("exec", pg, "pg_isready", "-U", "nexus", "-d", "nexus")
                if docker("exec", redis, "redis-cli", "-a", password, "PING") == "PONG":
                    break
            except RuntimeError:
                pass
            if time.monotonic() > deadline:
                raise RuntimeError("Dependencies did not initialize")
            time.sleep(1)
        # The sync worker is critical: a refused WebSocket correctly stops the
        # process. Supply an explicitly synthetic, inert transport fixture for
        # this initialization-only check; never weaken production supervision.
        CURRENT_PHASE = "fixture-start"
        fixture = Path(__file__).with_name("container_rpc_fixture.py").resolve()
        docker("run", "--detach", "--name", rpc, "--network", prefix,
               "--mount", f"type=bind,src={fixture},dst=/fixture.py,readonly",
               "python:3.12-alpine", "python", "/fixture.py")
        created.append(rpc)
        deadline = time.monotonic() + 60
        while True:
            try:
                docker("exec", rpc, "python", "-c",
                       "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8080/v2/info', timeout=2).close()")
                break
            except RuntimeError:
                if time.monotonic() > deadline:
                    raise RuntimeError("Inert transport fixture did not initialize")
                time.sleep(1)
        CURRENT_PHASE = "application-start"
        docker(
            "run", "--detach", "--name", app, "--network", prefix,
            "--publish", "127.0.0.1::10000", "--env", "PORT=10000",
            "--env", f"DATABASE_URL=postgres://nexus:{password}@{pg}:5432/nexus",
            "--env", f"REDIS_URL=redis://:{password}@{redis}:6379",
            "--env", "NEXUS_ADMIN_API_TOKEN=" + secrets.token_hex(32),
            "--env", "ORACLE_ENABLED=false", "--env", "NEXUS_EXPERIMENTAL_APIS=false",
            "--env", f"STACKS_NODE_RPC_URL=http://{rpc}:8080",
            "--env", f"STACKS_NODE_WS_URL=ws://{rpc}:8080", args.image,
        )
        created.append(app)
        port = docker("port", app, "10000/tcp").rsplit(":", 1)[1]
        base = "http://127.0.0.1:" + port
        CURRENT_PHASE = "initial-readiness"
        wait_for(base, "/health", 200, "ok")
        wait_for(base, "/live", 200)
        wait_for(base, "/admin/v1/status", 401)
        # Verify Redis safety observations and dependency loss separately.
        CURRENT_PHASE = "safety-mode"
        docker("exec", redis, "redis-cli", "-a", password, "SET", "nexus:safety_mode", "true")
        wait_for(base, "/health", 503, "safety-mode")
        docker("exec", redis, "redis-cli", "-a", password, "SET", "nexus:safety_mode", "false")
        wait_for(base, "/health", 200, "ok")
        for dependency in (pg, redis):
            CURRENT_PHASE = "postgres-outage" if dependency == pg else "redis-outage"
            docker("pause", dependency)
            try:
                code, body = response(base, "/health")
                if code != 503 or json.loads(body).get("status") != "unavailable":
                    raise RuntimeError("Dependency loss was reported healthy")
                wait_for(base, "/live", 200)
            finally:
                docker("unpause", dependency)
            CURRENT_PHASE = "dependency-recovery"
            wait_for(base, "/health", 200, "ok")
        CURRENT_PHASE = "restart"
        docker("restart", app)
        wait_for(base, "/health", 200, "ok")
        print(json.dumps({"result": "pass", "scope": "isolated-container-initialization",
                          "checks": ["hosting-port", "migrations", "backend-health",
                                     "liveness", "admin-denial", "safety-mode",
                                     "dependency-timeout-recovery", "restart"]}))
    finally:
        for name in reversed(created):
            subprocess.run(["docker", "rm", "--force", "--volumes", name],
                           capture_output=True, timeout=60)
        subprocess.run(["docker", "network", "rm", prefix], capture_output=True, timeout=30)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.TimeoutExpired):
        print(json.dumps({"result": "fail", "phase": CURRENT_PHASE,
                          "scope": "isolated-container-initialization"}))
        raise SystemExit(1)

