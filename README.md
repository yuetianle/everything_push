# 汇率监控与定存管理系统

Rust(axum) + Vue3 + PostgreSQL。多源汇率采集（frankfurter API / 中行 / 工行网页抓取 / 人工录入）、
阈值告警推送（企业微信 + Telegram）、图表、定期存款与到期提醒。详见 docs/SPEC.md。

## 开发
    docker compose -f deploy/docker-compose.yml up -d db
    cd backend && cargo run          # fintrack.toml 或环境变量（见 backend/fintrack.toml.example）
    cd frontend && npm i && npm run dev

- 本仓库开发机环境：PG 16 便携版（D:\pg-fx-dev，端口 5432，库 fx）+ Node 22 便携版（.tools/）；无 Docker

## 生成管理员密码哈希
    cd backend && cargo run --bin pwd_hash -- "你的密码"
    # 输出填入 ADMIN_PASSWORD（或 fintrack.toml 的 admin_password_hash）

## 部署（Linux 内网设备）
    cp deploy/.env.example deploy/.env   # 填齐变量
    docker compose -f deploy/docker-compose.prod.yml up -d --build
    # 访问 http://<主机>/ （建议 Tailscale 组网，不暴露公网）

## 通知渠道配置
- 企业微信：注册企业微信 → 建群 → 群机器人 → webhook 填 WECOM_WEBHOOK_URL
- Telegram：@BotFather 建 bot；chat_id 可通过与 bot 对话后 getUpdates 获取；
  大陆网络需 TELEGRAM_PROXY（容器内可达的 socks5/http 代理）

## 测试
    cd backend && cargo test
    TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored
    cd frontend && npm test

## 已知事项
- 工行（icbc）抓取端点核验失败（官网 JS 渲染，候选端点 404），当前按设计降级为"数据源失效"状态；恢复方法见 backend/src/sources/icbc.rs 顶部注释（核验新端点→替换 fetch→重抓 fixture）
- 基准汇率为 ECB 参考汇率（每日一次），非央行中间价（SPEC 附决策记录 #12）
