#!/bin/sh
# Run a local-only, ephemeral Redis sidecar (Nexus treats Redis as a cache; a
# managed/remote Redis replaces this in production hardening). Foreground mode
# avoids daemonize pidfile/logfile path issues in a minimal container.
echo "[entrypoint] starting redis sidecar"
redis-server --daemonize no --save "" --appendonly no --bind 127.0.0.1 --port 6379 \
  --dir /tmp --pidfile /tmp/redis.pid --logfile "" >/tmp/redis.log 2>&1 &
echo "[entrypoint] redis pid $!"
sleep 1

echo "[entrypoint] starting conxian-nexus"
exec conxian-nexus "$@"
