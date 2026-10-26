#!/usr/bin/env bash
# All builds and VM runs use the same Dockerfile-defined environment.
set -euo pipefail

repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
docker_terminal=()
if [[ ${1:-} == --interactive ]]; then
    docker_terminal=(-it)
    shift
fi
if (( $# == 0 )); then
    printf 'Usage: %s [--interactive] command [arguments...]\n' "$0" >&2
    exit 2
fi

exec docker run --rm "${docker_terminal[@]}" \
    --network=none --cpus=2 --memory=4g --memory-swap=4g \
    --mount "type=bind,src=${repository_root},dst=/workspace" \
    --workdir /workspace \
    hematite-build:phase0 "$@"
