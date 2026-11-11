#!/usr/bin/env bash
# 汇率监控与定存管理系统 — Linux 一键构建/启停脚本
#
# 用法: ./deploy.sh <命令>
#   init              创建 deploy/.env（从 .env.example）
#   hash <明文密码>   在容器内生成 ADMIN_PASSWORD 所需的 argon2 哈希
#   up | start        构建并启动全部服务（db/backend/frontend/backup），探活等待就绪
#   down | stop       停止全部服务
#   restart           重启全部服务
#   build             仅构建镜像（不启动）
#   logs [服务]       跟随日志（默认全部；可 db/backend/frontend/backup）
#   status            各容器状态 + 数据源健康
#   update            git pull 后重新构建并启动（保留数据卷）
#   backup            立即手动备份 PG 到 data/backups/manual-<时间>.sql.gz
set -euo pipefail

cd "$(dirname "$0")"

COMPOSE_FILE="deploy/docker-compose.prod.yml"
ENV_FILE="deploy/.env"

info() { printf '\033[1;32m[INFO]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[WARN]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[ERR ]\033[0m %s\n' "$*" >&2; exit 1; }

dc() {
    docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" "$@"
}

# 非 TTY 环境（一次性 ssh / CI / 脚本内调用）下 compose 的进度 UI 会报
# "failed to get console: provided file is not a console"：
# build 显式用 plain 进度（老版本 compose 无此选项则回退），run 一律加 -T。
compose_build() {
    if dc build --help 2>/dev/null | grep -q -- '--progress'; then
        dc build --progress plain "$@"
    else
        dc build "$@"
    fi
}

require_docker() {
    command -v docker >/dev/null 2>&1 || die "未找到 docker —— 请先安装 Docker Engine 与 compose 插件"
    docker info >/dev/null 2>&1 || die "Docker 守护进程未运行"
}

require_env() {
    [ -f "$ENV_FILE" ] || die "缺少 $ENV_FILE —— 先运行: ./deploy.sh init"
    # 值里含 $ 等字符，禁止 source；仅做结构性校验（compose 自行解析 --env-file）
    grep -Eq '^ADMIN_PASSWORD=\$argon2id\$' "$ENV_FILE" \
        || die "deploy/.env 中 ADMIN_PASSWORD 不是 argon2 哈希（运行: ./deploy.sh hash <你的密码>，把输出整行填入）"
}

cmd_init() {
    if [ -f "$ENV_FILE" ]; then
        warn "$ENV_FILE 已存在，跳过创建"
    else
        cp deploy/.env.example "$ENV_FILE"
        info "已创建 $ENV_FILE —— 接下来:"
        info "  1) ./deploy.sh hash <你的密码>   生成管理员密码哈希"
        info "  2) 编辑 $ENV_FILE 填入哈希与通知渠道"
        info "  3) ./deploy.sh up"
    fi
}

cmd_hash() {
    [ $# -eq 1 ] || die "用法: ./deploy.sh hash <明文密码>"
    require_docker
    [ -f "$ENV_FILE" ] || die "先运行: ./deploy.sh init"
    info "构建 backend 镜像（首次较慢）..."
    compose_build backend
    info "哈希如下，整行填入 $ENV_FILE 的 ADMIN_PASSWORD= 之后（不要加引号）:"
    printf '\nADMIN_PASSWORD=%s\n\n' "$(dc run --rm -T --no-deps --entrypoint pwd_hash backend "$1")"
}

wait_ready() {
    info "等待服务就绪（最长 120 秒）..."
    local i ok=0
    for i in $(seq 1 60); do
        if (exec 3<>"/dev/tcp/127.0.0.1/80") 2>/dev/null; then
            exec 3>&- 3<&- || true
            ok=1
            break
        fi
        sleep 2
    done
    [ "$ok" -eq 1 ] || { warn "120 秒内 80 端口未就绪，查看日志: ./deploy.sh logs"; dc ps; return 1; }
    if command -v curl >/dev/null 2>&1; then
        curl -fsS http://127.0.0.1/healthz >/dev/null && info "healthz 探活通过"
    else
        info "80 端口已开放（主机无 curl，跳过 healthz 探活）"
    fi
    info "服务就绪: http://$(hostname 2>/dev/null || echo localhost)/  （建议 Tailscale 内网访问，不暴露公网）"
}

cmd_up() {
    require_docker; require_env
    dc config -q || die "compose 配置校验失败，检查 $ENV_FILE"
    compose_build
    dc up -d
    wait_ready
    dc ps
}

cmd_down()    { require_docker; require_env; dc down; }
cmd_restart() { require_docker; require_env; dc restart; wait_ready; }
cmd_build()   { require_docker; require_env; compose_build; }

cmd_logs() {
    require_docker
    if [ $# -ge 1 ]; then dc logs -f --tail 200 "$1"; else dc logs -f --tail 200; fi
}

cmd_status() {
    require_docker; require_env
    dc ps
    echo
    info "数据源健康：浏览器登录后在「汇率总览」页查看状态灯（GET /api/sources/status）"
}

cmd_update() {
    require_docker; require_env
    info "拉取最新代码..."
    git pull --ff-only
    cmd_up
}

cmd_backup() {
    require_docker; require_env
    mkdir -p data/backups
    local out="data/backups/manual-$(date +%Y%m%d-%H%M%S).sql.gz"
    dc exec -T db pg_dump -U fx fx | gzip > "$out"
    info "已备份: $out ($(du -h "$out" | cut -f1))"
}

cmd_help() { sed -n '4,14p' "$0" | sed 's/^# \{0,1\}//'; }

case "${1:-help}" in
    init)    shift; cmd_init "$@" ;;
    hash)    shift; cmd_hash "$@" ;;
    up|start)    shift; cmd_up "$@" ;;
    down|stop)   shift; cmd_down "$@" ;;
    restart)     shift; cmd_restart "$@" ;;
    build)       shift; cmd_build "$@" ;;
    logs)        shift; cmd_logs "$@" ;;
    status)      shift; cmd_status "$@" ;;
    update)      shift; cmd_update "$@" ;;
    backup)      shift; cmd_backup "$@" ;;
    help|-h|--help) cmd_help ;;
    *) die "未知命令: $1（运行 ./deploy.sh help 查看用法）" ;;
esac
