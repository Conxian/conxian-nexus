#!/bin/sh
set -e

# Start the bundled Redis sidecar (local-only, ephemeral). Nexus treats Redis as
# a cache; a managed/remote Redis replaces this sidecar in production hardening.
redis-server --daemonize yes --save "" --appendonly no --bind 127.0.0.1 --port 6379

exec conxian-nexus "$@"
