#!/usr/bin/env bash
# synthia image build entry — Docker only, no cluster interaction.
#
#   scripts/images.sh build             # build server + web + mcp
#   scripts/images.sh build server      # build only the server
#   scripts/images.sh build web         # build only the web
#   scripts/images.sh build mcp         # build only the mcp stdio binary
#   TAG=0.1.0-rc.1 scripts/images.sh build
#
#   scripts/images.sh artifact          # export server + windows binaries to ./dist/
#   scripts/images.sh artifact server   # export only the Linux musl binary
#   scripts/images.sh artifact windows  # export only the Windows PE
#
# Overrides:
#   TAG=vX.Y.Z        image tag (default: 0.1.0)
#   RUST_VERSION=1.98  Rust toolchain for the builder stage
#   NODE_VERSION=20    Node toolchain for the web builder stage
#   PUSH=1             push to the registry after build
#   REGISTRY=docker.io/crochee  registry to tag for (PUSH=1)
#   ARTIFACT_DIR= ./dist  local destination for the binary export (artifact target)
#
# The release workflow runs the equivalent commands directly; this
# script is for local dev / CI fallback / k8s-deploy.sh callers.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

TAG=${TAG:-0.1.0}
RUST_VERSION=${RUST_VERSION:-1.98}
NODE_VERSION=${NODE_VERSION:-20}
REGISTRY=${REGISTRY:-docker.io/crochee}

GIT_SHA=${GIT_SHA:-$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo "unknown")}
BUILD_TIME=${BUILD_TIME:-$(date -u +%FT%TZ)}

die() { echo "!! $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 not on PATH"; }

usage() {
    sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

build_server() {
    need docker
    echo "== build synthia-server:${TAG} (sha=${GIT_SHA:0:7}) =="
    docker build \
        --build-arg "SYNTHIA_GIT_SHA=${GIT_SHA}" \
        --build-arg "SYNTHIA_BUILD_TIME=${BUILD_TIME}" \
        --build-arg "RUST_VERSION=${RUST_VERSION}" \
        --build-arg "SYNTHIA_VERSION=${TAG}" \
        -f "${REPO_ROOT}/Dockerfile.server" \
        -t "${REGISTRY}/synthia-server:${TAG}" \
        "${REPO_ROOT}"
}

# 走 Dockerfile.server 的 `--target artifact` 阶段把可执行文件直接
# 落到本地 ARTIFACT_DIR (默认 ./dist/), 与 ci.yml 的 cross-linux-musl
# 走同一条路径 —— 本地 `make server-artifact` 复现 CI smoke, 排查时不
# 必 push 到 registry 再 pull.
build_server_artifact() {
    need docker
    ARTIFACT_DIR=${ARTIFACT_DIR:-./dist}
    mkdir -p "${ARTIFACT_DIR}"
    echo "== artifact synthia-server (musl, sha=${GIT_SHA:0:7}) -> ${ARTIFACT_DIR}/synthia-server =="
    docker buildx build --target artifact \
        --build-arg "SYNTHIA_GIT_SHA=${GIT_SHA}" \
        --build-arg "SYNTHIA_BUILD_TIME=${BUILD_TIME}" \
        --build-arg "RUST_VERSION=${RUST_VERSION}" \
        --output "type=local,dest=${ARTIFACT_DIR}" \
        -f "${REPO_ROOT}/Dockerfile.server" \
        "${REPO_ROOT}"
}

# 走 Dockerfile.server.windows 的 `--target artifact` 阶段出 Windows PE;
# 与 ci.yml 的 cross-windows 走同一条路径. 输出 synthia-server.exe.
build_server_windows() {
    need docker
    ARTIFACT_DIR=${ARTIFACT_DIR:-./dist}
    mkdir -p "${ARTIFACT_DIR}"
    echo "== artifact synthia-server (windows gnu, sha=${GIT_SHA:0:7}) -> ${ARTIFACT_DIR}/synthia-server.exe =="
    docker buildx build --target artifact \
        --build-arg "SYNTHIA_GIT_SHA=${GIT_SHA}" \
        --build-arg "SYNTHIA_BUILD_TIME=${BUILD_TIME}" \
        --output "type=local,dest=${ARTIFACT_DIR}" \
        -f "${REPO_ROOT}/Dockerfile.server.windows" \
        "${REPO_ROOT}"
}

build_web() {
    need docker
    echo "== build synthia-web:${TAG} (sha=${GIT_SHA:0:7}) =="
    docker build \
        --build-arg "SYNTHIA_VERSION=${TAG}" \
        --build-arg "SYNTHIA_GIT_SHA=${GIT_SHA}" \
        --build-arg "SYNTHIA_BUILD_TIME=${BUILD_TIME}" \
        --build-arg "NODE_VERSION=${NODE_VERSION}" \
        -f "${REPO_ROOT}/Dockerfile.web" \
        -t "${REGISTRY}/synthia-web:${TAG}" \
        "${REPO_ROOT}"
}

build_mcp() {
    need docker
    echo "== build synthia-mcp-server:${TAG} (sha=${GIT_SHA:0:7}) =="
    docker build \
        --build-arg "SYNTHIA_GIT_SHA=${GIT_SHA}" \
        --build-arg "SYNTHIA_BUILD_TIME=${BUILD_TIME}" \
        --build-arg "RUST_VERSION=${RUST_VERSION}" \
        --build-arg "SYNTHIA_VERSION=${TAG}" \
        -f "${REPO_ROOT}/Dockerfile.mcp" \
        -t "${REGISTRY}/synthia-mcp-server:${TAG}" \
        "${REPO_ROOT}"
}

maybe_push() {
    if [ "${PUSH:-0}" = "1" ]; then
        echo "== push =="
        docker push "${REGISTRY}/synthia-server:${TAG}"
        docker push "${REGISTRY}/synthia-web:${TAG}"
        docker push "${REGISTRY}/synthia-mcp-server:${TAG}"
    fi
}

case "${1:-all}" in
build)
    target="${2:-all}"
    case "$target" in
    server) build_server ;;
    web)    build_web ;;
    mcp)    build_mcp ;;
    all)    build_server && build_web && build_mcp ;;
    *) die "unknown target: $target" ;;
    esac
    maybe_push
    ;;
artifact)
    target="${2:-all}"
    case "$target" in
    server)  build_server_artifact ;;
    windows) build_server_windows ;;
    all)     build_server_artifact && build_server_windows ;;
    *) die "unknown artifact target: $target" ;;
    esac
    ;;
-h|--help|help)
    usage
    ;;
*)
    usage >&2
    exit 2
    ;;
esac