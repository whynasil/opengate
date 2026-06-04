#!/usr/bin/env bash
set -euo pipefail

docker build -f Dockerfile.musl -t opengate:musl .
