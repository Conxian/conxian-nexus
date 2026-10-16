#!/usr/bin/env python3
"""Exercise an already-built image with disposable, authenticated dependencies.

No production credentials, chain synchronization or payment acceptance is tested.
Docker resources created by this run are removed on both success and failure.
"""
import argparse
import json
import secrets
import subprocess
import time
import urllib.error
import urllib.request


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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    args = parser.parse_args()
    prefix = "nexus-init-" + secrets.token_hex(6)
    password = secrets.token_hex(32)
    pg, redis, app = (prefix + suffix for suffix in ("-pg", "-redis", "-app"))
    created = []
    docker("network", "create", prefix)
    try:
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
        docker(
            "run", "--detach", "--name", app, "--network", prefix,
            "--publish", "127.0.0.1::10000", "--env", "PORT=10000",
            "--env", f"DATABASE_URL=postgres://nexus:{password}@{pg}:5432/nexus",
            "--env", f"REDIS_URL=redis://:{password}@{redis}:6379",
            "--env", "NEXUS_ADMIN_API_TOKEN=" + secrets.token_hex(32),
            "--env", "ORACLE_ENABLED=false", "--env", "NEXUS_EXPERIMENTAL_APIS=false",
            "--env", "STACKS_NODE_RPC_URL=http://127.0.0.1:9",
            "--env", "STACKS_NODE_WS_URL=ws://127.0.0.1:9", args.image,
        )
        created.append(app)
        port = docker("port", app, "10000/tcp").rsplit(":", 1)[1]
        base = "http://127.0.0.1:" + port
        wait_for(base, "/health", 200, "ok")
        wait_for(base, "/live", 200)
        wait_for(base, "/admin/v1/status", 401)
        # Verify Redis safety observations and dependency loss separately.
        docker("exec", redis, "redis-cli", "-a", password, "SET", "nexus:safety_mode", "true")
        wait_for(base, "/health", 503, "safety-mode")
        docker("exec", redis, "redis-cli", "-a", password, "SET", "nexus:safety_mode", "false")
        wait_for(base, "/health", 200, "ok")
        for dependency in (pg, redis):
            docker("pause", dependency)
            try:
                code, body = response(base, "/health")
                if code != 503 or json.loads(body).get("status") != "unavailable":
                    raise RuntimeError("Dependency loss was reported healthy")
                wait_for(base, "/live", 200)
            finally:
                docker("unpause", dependency)
            wait_for(base, "/health", 200, "ok")
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
        raise SystemExit("Container initialization failed; no production acceptance is implied.")
