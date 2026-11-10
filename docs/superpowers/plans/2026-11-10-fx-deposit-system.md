# 汇率监控与定存管理系统 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建"多源汇率采集 + 阈值告警推送 + 定存到期提醒"的单管理员家庭财务系统。

**Architecture:** 后端单体 axum 服务承载 REST API、采集调度（tokio interval 循环）、告警引擎与推送；exchange_rates 按月分区存时序数据；前端 Vue3 SPA 经 nginx 反代 `/api`。所有定时逻辑（采集循环、每小时基准刷新、每日 09:05 到期扫描+分区维护）在进程内 spawn，不引外部调度器。

**Tech Stack:** Rust(axum 0.8, sqlx 0.8, reqwest, scraper, argon2) / Vue3 + TS + Element Plus + ECharts / PostgreSQL 16 / docker-compose。

**Spec:** `docs/SPEC.md`（执行者须同时阅读 SPEC 与本计划）

## Global Constraints

- Rust edition 2021；crate 名 `fintrack`；依赖：axum 0.8 / sqlx 0.8 / reqwest 0.12 / scraper 0.21 / argon2 0.5
- **禁用 sqlx 编译期宏（`query!`/`query_as!`）**：一律 `sqlx::query`/`sqlx::query_as` 运行时绑定，避免宏的 DATABASE_URL 编译检查
- 金额与汇率一律 `rust_decimal::Decimal`（SQL `numeric`）；**Decimal 经 serde 序列化为 JSON 字符串**（前端用 `Number()` 转）
- PostgreSQL 16，dev 库/用户/密码均 `fx`：`postgres://fx:fx@localhost:5432/fx`；时间戳一律 `timestamptz`；业务时区 Asia/Shanghai
- 单元测试 `cargo test`（无 DB 依赖）；DB 集成测试标注 `#[ignore]`，运行命令 `TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored`（先 `docker compose -f deploy/docker-compose.yml up -d db`）
- 前端 Node 20+，`npm run build` 必须零错误；纯逻辑用 vitest
- 每个任务至少一次 git commit，提交信息末尾加：`Co-Authored-By: Claude Code <noreply@anthropic.com>`
- 开发机 Windows（git-bash），命令均按 bash 书写
- 环境变量名以 SPEC §11 为准：`DATABASE_URL`、`ADMIN_PASSWORD`（argon2 哈希）、`WECOM_WEBHOOK_URL`、`TELEGRAM_BOT_TOKEN`、`TELEGRAM_CHAT_ID`、`TELEGRAM_PROXY`、`SCRAPE_INTERVAL_SECS`

## File Structure（最终形态）

```
backend/
  Cargo.toml
  migrations/0001_init.sql        # 全部表 DDL（Task 2）
  fintrack.toml.example           # dev 配置样例（gitignore 真身）
  tests/
    fixtures/boc.html, icbc.html  # 抓取器 fixture（Task 9/10）
    auth.rs, rates_int.rs, alert_int.rs, notify_int.rs,
    deposits_int.rs, maturity_int.rs, partitions_int.rs  # #[ignore] 集成测试
  src/
    main.rs                       # 组装：配置/DB/路由/调度（Task 1 起步，Task 13 定稿）
    bin/pwd_hash.rs               # 生成 argon2 哈希（Task 3）
    config.rs                     # TOML + 环境变量配置（Task 1）
    error.rs                      # AppError（Task 1）
    db.rs                         # PgPool + 分区维护（Task 2）
    auth.rs                       # argon2 校验 + HMAC 签名 cookie + 中间件（Task 3）
    rates/mod.rs                  # RatePoint + 存储与聚合查询（Task 4）
    rates/api.rs                  # /api/rates/* handlers（Task 4）
    sources/frankfurter.rs        # 基准 API 客户端 + 回填（Task 5）
    sources/health.rs             # 健康跟踪 HealthRegistry（Task 8）
    sources/boc.rs                # 中行抓取器（Task 9）
    sources/icbc.rs               # 工行抓取器（Task 10）
    alerts/mod.rs                 # 规则 CRUD（Task 6 起步，api 同文件）
    alerts/engine.rs              # 告警引擎纯逻辑 + evaluate_all（Task 6）
    notify/mod.rs                 # Notifier trait + dispatch 重试入库（Task 7）
    notify/wecom.rs, notify/telegram.rs, notify/api.rs  # Task 7
    deposits/mod.rs               # 模型 + 利息/到期纯函数 + CRUD + summary（Task 11）
    deposits/api.rs               # Task 11
    maturity.rs                   # 到期扫描 + next_0905（Task 12）
    scheduler.rs                  # 采集循环 + 每日任务 + 总装（Task 13）
frontend/
  vite.config.ts                  # dev 代理 /api → localhost:8080（Task 1）
  src/main.ts, App.vue, router/index.ts, stores/auth.ts,
  src/api/client.ts, src/utils/format.ts
  src/views/{Login,Overview,Charts,Rules,Notifications,Deposits}.vue
  src/components/{TrendChart,SourceLight,ManualEntryDialog}.vue
deploy/
  docker-compose.yml              # dev：仅 db（Task 1）
  docker-compose.prod.yml         # 生产：db+backend+frontend+backup（Task 17）
  Dockerfile.backend, Dockerfile.frontend, nginx.conf（Task 17）
docs/SPEC.md
README.md                         # Task 17
```

---

### Task 1: 仓库脚手架（cargo + vite + dev 数据库）

**Files:**
- Create: `.gitignore`、`deploy/docker-compose.yml`、`backend/Cargo.toml`、`backend/src/main.rs`、`backend/src/config.rs`、`backend/src/error.rs`、`backend/fintrack.toml.example`、`frontend/`（vite 脚手架）、`frontend/vite.config.ts`

**Interfaces:**
- Produces: `config::AppConfig::load() -> Result<AppConfig, config::ConfigError>`（字段见下）；`error::{AppError, ApiResult}`；axum 服务监听 `0.0.0.0:8080`，路由 `GET /healthz`；dev PG 容器（fx/fx/fx）

- [ ] **Step 1: git init 与 .gitignore**

```bash
cd "D:/code/ai-code/金融" && git init
```

`.gitignore`：

```
target/
node_modules/
dist/
data/
.env
backend/fintrack.toml
frontend/dist/
```

- [ ] **Step 2: dev 数据库 compose**

`deploy/docker-compose.yml`：

```yaml
services:
  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: fx
      POSTGRES_PASSWORD: fx
      POSTGRES_DB: fx
    ports:
      - "5432:5432"
    volumes:
      - ../data/pg:/var/lib/postgresql/data
```

- [ ] **Step 3: backend 骨架**

`backend/Cargo.toml`：

```toml
[package]
name = "fintrack"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5", features = ["util"] }
axum-extra = { version = "0.10", features = ["cookie"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "chrono", "rust_decimal", "migrate"] }
reqwest = { version = "0.12", features = ["json", "socks"] }
scraper = "0.21"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
config = { version = "0.14", features = ["toml"] }
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
rust_decimal = { version = "1", features = ["serde"] }
argon2 = "0.5"
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
subtle = "2"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
async-trait = "0.1"

[dev-dependencies]
wiremock = "0.6"
```

`backend/src/config.rs`：

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub admin_password_hash: String,
    pub wecom_webhook_url: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub telegram_proxy: Option<String>,
    #[serde(default = "d_interval")]
    pub scrape_interval_secs: u64,
    #[serde(default = "d_pairs")]
    pub currency_pairs: Vec<String>,
}

fn d_interval() -> u64 { 180 }
fn d_pairs() -> Vec<String> {
    ["USD", "EUR", "JPY", "HKD", "GBP", "AUD"]
        .iter().map(|c| format!("{c}/CNY")).collect()
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::with_name("fintrack").required(false))
            .add_source(config::Environment::empty())
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_pairs_are_six_cny_pairs() {
        assert_eq!(d_pairs(), vec![
            "USD/CNY", "EUR/CNY", "JPY/CNY", "HKD/CNY", "GBP/CNY", "AUD/CNY"]);
    }
}
```

`backend/src/error.rs`：

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub struct AppError(pub StatusCode, pub String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError(StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}"))
    }
}

pub type ApiResult<T> = Result<T, AppError>;
```

`backend/src/main.rs`（本任务版本）：

```rust
mod config;
mod error;

use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = config::AppConfig::load()?;
    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

`backend/fintrack.toml.example`：

```toml
# 复制为 backend/fintrack.toml（已 gitignore），或改用环境变量
database_url = "postgres://fx:fx@localhost:5432/fx"
admin_password_hash = "替换为 cargo run --bin pwd_hash -- 你的密码 的输出"
# wecom_webhook_url = "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=xxx"
# telegram_bot_token = "123:abc"
# telegram_chat_id = "10086"
# telegram_proxy = "socks5://127.0.0.1:1080"
# scrape_interval_secs = 180
```

注意：本任务 `cargo run` 需要 `database_url` 与 `admin_password_hash` 存在（config 反序列化要求）——用 fintrack.toml.example 复制一份填假哈希即可，如 `"x"`。

- [ ] **Step 4: 启动 dev 数据库并验证后端**

```bash
docker compose -f deploy/docker-compose.yml up -d db
cd backend && cargo test   # 期望：config 单测 PASS
DATABASE_URL=postgres://fx:fx@localhost:5432/fx ADMIN_PASSWORD=x cargo run &
sleep 5 && curl -s localhost:8080/healthz   # 期望输出: ok
```

- [ ] **Step 5: frontend 脚手架**

```bash
cd "D:/code/ai-code/金融"
npm create vite@latest frontend -- --template vue-ts
cd frontend && npm install
```

`frontend/vite.config.ts`（覆盖）：

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  server: {
    proxy: { '/api': 'http://localhost:8080' },
  },
})
```

```bash
npm run build   # 期望：零错误构建
```

- [ ] **Step 6: Commit**

```bash
cd "D:/code/ai-code/金融"
git add -A
git commit -m "chore: 项目脚手架（cargo + vite + dev pg compose）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 2: 数据库迁移与分区维护

**Files:**
- Create: `backend/migrations/0001_init.sql`、`backend/src/db.rs`、`backend/tests/partitions_int.rs`
- Modify: `backend/src/main.rs`

**Interfaces:**
- Produces: `db::connect(&str) -> Result<sqlx::PgPool, sqlx::Error>`（内部设置 `PgPoolOptions::max_connections(5)`）；`db::ensure_partitions(&PgPool) -> Result<(), sqlx::Error>`（当月+下月）；`db::ensure_partition_for(&PgPool, i32, u32)`；`db::drop_old_partitions(&PgPool, u32) -> Result<Vec<String>, sqlx::Error>`；main 启动时 `sqlx::migrate!()` + `ensure_partitions`

- [ ] **Step 1: 迁移 SQL**

`backend/migrations/0001_init.sql`：

```sql
CREATE TABLE exchange_rates (
  id          bigserial,
  source      text        NOT NULL,            -- 'api' | 'boc' | 'icbc' | 'manual'
  pair        text        NOT NULL,            -- 'USD/CNY'
  fx_bid      numeric(14,6),                   -- 现汇买入价（银行源）
  fx_ask      numeric(14,6),                   -- 现汇卖出价
  mid         numeric(14,6),                   -- 中间价/基准价
  recorded_at timestamptz NOT NULL,
  note        text,
  PRIMARY KEY (id, recorded_at)
) PARTITION BY RANGE (recorded_at);

CREATE INDEX idx_rates_lookup ON exchange_rates (pair, source, recorded_at DESC);

CREATE TABLE alert_rules (
  id               bigserial PRIMARY KEY,
  source           text NOT NULL,
  pair             text NOT NULL,
  price_type       text NOT NULL CHECK (price_type IN ('fx_bid','fx_ask','mid')),
  direction        text NOT NULL CHECK (direction IN ('above','below')),
  threshold        numeric NOT NULL,
  cooldown_minutes int  NOT NULL DEFAULT 60,
  enabled          bool NOT NULL DEFAULT true,
  created_at       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE alert_rule_state (
  rule_id           bigint PRIMARY KEY REFERENCES alert_rules(id) ON DELETE CASCADE,
  armed             bool NOT NULL DEFAULT true,
  last_triggered_at timestamptz,
  last_value        numeric
);

CREATE TABLE notifications (
  id         bigserial PRIMARY KEY,
  channel    text NOT NULL,                    -- 'wecom' | 'telegram'
  title      text NOT NULL,
  body       text NOT NULL,
  status     text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','sent','failed')),
  retries    int  NOT NULL DEFAULT 0,
  error      text,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE deposits (
  id             bigserial PRIMARY KEY,
  person         text NOT NULL,
  bank           text NOT NULL,
  currency       char(3) NOT NULL DEFAULT 'CNY',
  amount         numeric NOT NULL CHECK (amount > 0),
  rate           numeric NOT NULL,             -- 年利率 %
  start_date     date NOT NULL,
  term_months    int  NOT NULL CHECK (term_months > 0),
  maturity_date  date NOT NULL,
  remind_7d      bool NOT NULL DEFAULT true,
  remind_1d      bool NOT NULL DEFAULT true,
  remind_today   bool NOT NULL DEFAULT true,
  notified_7d    date,
  notified_1d    date,
  notified_today date,
  note           text,
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_at     timestamptz NOT NULL DEFAULT now()
);
```

- [ ] **Step 2: db.rs**

`backend/src/db.rs`：

```rust
use chrono::{DateTime, Months, NaiveDate, Utc};
use sqlx::{PgPool, PoolOptions};

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PoolOptions::new().max_connections(5).connect(url).await
}

/// 确保当月与下月分区存在（幂等）
pub async fn ensure_partitions(pool: &PgPool) -> Result<(), sqlx::Error> {
    let first = Utc::now().date_naive().with_day(1).unwrap();
    for m in 0u32..=1 {
        let p = first.checked_add_months(Months::new(m)).unwrap();
        ensure_partition_for(pool, p.year(), p.month()).await?;
    }
    Ok(())
}

pub async fn ensure_partition_for(pool: &PgPool, year: i32, month: u32) -> Result<(), sqlx::Error> {
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let end = start.checked_add_months(Months::new(1)).unwrap();
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS exchange_rates_{year}{month:02} \
         PARTITION OF exchange_rates FOR VALUES FROM ('{start}') TO ('{end}')"
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

/// 删除早于保留窗口的月分区，返回被删除的分区名
pub async fn drop_old_partitions(pool: &PgPool, retain_months: u32) -> Result<Vec<String>, sqlx::Error> {
    let cutoff = Utc::now().date_naive().with_day(1).unwrap()
        .checked_sub_months(Months::new(retain_months)).unwrap();
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT tablename FROM pg_tables WHERE tablename LIKE 'exchange_rates______'")
        .fetch_all(pool).await?;
    let mut dropped = vec![];
    for (t,) in rows {
        let ym = &t["exchange_rates_".len()..];
        let (y, m) = (ym[..4].parse::<i32>().unwrap(), ym[4..].parse::<u32>().unwrap());
        if NaiveDate::from_ymd_opt(y, m, 1).unwrap() < cutoff {
            sqlx::query(&format!("DROP TABLE IF EXISTS {t}")).execute(pool).await?;
            dropped.push(t);
        }
    }
    Ok(dropped)
}

/// 仅供集成测试：插入一条汇率
#[cfg(test)]
#[allow(dead_code)]
pub async fn _test_insert(pool: &PgPool, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO exchange_rates (source, pair, mid, recorded_at) VALUES ('api','USD/CNY', 7.1, $1)")
        .bind(at).execute(pool).await?;
    Ok(())
}
```

- [ ] **Step 3: main.rs 接入 DB + 迁移**

`backend/src/main.rs` 顶部加 `mod db;`，`main()` 中 `let cfg = ...` 之后插入：

```rust
    let pool = db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    db::ensure_partitions(&pool).await?;
```

`_ = &pool;` 暂时压制未用警告（Task 4 起进入 AppState）。

- [ ] **Step 4: 集成测试（#[ignore]）**

`backend/tests/partitions_int.rs`：

```rust
use chrono::Utc;
use sqlx::PgPool;
use fintrack::db;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    db::connect(&url).await.unwrap()
}

#[tokio::test]
#[ignore]
async fn ensure_partitions_idempotent_and_old_partition_dropped() {
    let pool = pool().await;
    sqlx::migrate!().run(&pool).await.unwrap();
    db::ensure_partitions(&pool).await.unwrap();
    db::ensure_partitions(&pool).await.unwrap(); // 幂等

    // 造一个 2024-01 老分区并写入数据
    db::ensure_partition_for(&pool, 2024, 1).await.unwrap();
    sqlx::query("INSERT INTO exchange_rates (source,pair,mid,recorded_at) VALUES ('api','USD/CNY',7.1,'2024-01-15T00:00:00Z')")
        .execute(&pool).await.unwrap();

    let dropped = db::drop_old_partitions(&pool, 24).await.unwrap();
    assert!(dropped.contains(&"exchange_rates_202401".to_string()));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM exchange_rates WHERE recorded_at < '2024-02-01'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0); // 分区删除后数据随分区消失
}
```

注意：`main.rs` 里 lib 代码需可被集成测试引用——在 `backend/Cargo.toml` 加：

```toml
[lib]
name = "fintrack"
path = "src/lib.rs"
```

并创建 `backend/src/lib.rs`：

```rust
pub mod config;
pub mod db;
pub mod error;
```

`main.rs` 改为 `use fintrack::{config, db};` 风格引用（删除原 `mod` 声明，模块统一经 lib 暴露；后续任务的 `mod xxx;` 一律加到 lib.rs）。

- [ ] **Step 5: 运行验证**

```bash
docker compose -f deploy/docker-compose.yml up -d db
cd backend
cargo test                       # 期望：此前单测仍 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored
# 期望：partitions_int PASS
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(db): 全量表结构迁移 + 按月分区维护

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 3: 认证（argon2 + HMAC 签名 cookie + 中间件）

**Files:**
- Create: `backend/src/auth.rs`、`backend/src/bin/pwd_hash.rs`、`backend/tests/auth.rs`
- Modify: `backend/src/lib.rs`、`backend/src/main.rs`

**Interfaces:**
- Produces:
  - `auth::COOKIE_NAME = "fx_session"`、`auth::SESSION_DAYS: i64 = 30`
  - `auth::verify_password(hash: &str, password: &str) -> bool`
  - `auth::sign_token(secret: &str, expires_at_unix: i64) -> String`（格式 `"{exp}.{hmac_hex}"`，HMAC-SHA256，密钥即 secret）
  - `auth::verify_token(secret: &str, token: &str, now_unix: i64) -> bool`
  - `#[derive(Clone)] pub struct AuthState { pub password_hash: std::sync::Arc<String> }`
  - `pub async fn auth_mw(State<AuthState>, req: Request, next: Next) -> Response`（校验 cookie，失败 401）
  - `pub fn auth_routes() -> Router<AuthState>`（POST `/api/auth/login`、POST `/api/auth/logout`、GET `/api/auth/me`）
  - 登录成功 Set-Cookie：`fx_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000`
  - 哈希生成命令：`cargo run --bin pwd_hash -- <明文密码>`

- [ ] **Step 1: 写失败测试**

`backend/tests/auth.rs`（单元测试部分不需要 DB）：

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use fintrack::auth::*;
use tower::ServiceExt;

fn state() -> AuthState {
    AuthState { password_hash: std::sync::Arc::new("fakehash".into()) }
}

#[test]
fn sign_and_verify_roundtrip() {
    let tok = sign_token("secret", 2000);
    assert!(verify_token("secret", &tok, 1999));
}

#[test]
fn verify_rejects_expired_tampered_wrong_secret() {
    let tok = sign_token("secret", 2000);
    assert!(!verify_token("secret", &tok, 2001), "过期");
    assert!(!verify_token("secret", "2000.deadbeef", 1999), "签名篡改");
    assert!(!verify_token("other", &tok, 1999), "密钥不同");
    assert!(!verify_token("secret", "garbage", 1999));
}

#[tokio::test]
async fn login_wrong_password_401_and_middleware_blocks_without_cookie() {
    let app: Router = auth_routes().with_state(state());
    let resp = app.clone()
        .oneshot(Request::post("/api/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"password":"wrong"}"#)).unwrap())
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // fakehash 不是合法 argon2 串 → 正确密码也 401；这里只验证无 cookie 访问受保护路由被拦截
    // auth_mw 的 401 用一个最小路由验证：
    let protected = axum::Router::new()
        .route("/api/x", axum::routing::get(|| async { "ok" }))
        .route_layer(axum::middleware::from_fn_with_state(state(), auth_mw));
    let resp = protected.oneshot(Request::get("/api/x").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cd backend && cargo test --test auth
# 期望：编译失败（auth 模块不存在 / 函数未定义）
```

- [ ] **Step 3: 实现 auth.rs**

`backend/src/auth.rs`：

```rust
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::error::{ApiResult, AppError};

pub const COOKIE_NAME: &str = "fx_session";
pub const SESSION_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AuthState {
    pub password_hash: Arc<String>,
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    match argon2::PasswordHash::new(hash) {
        Ok(parsed) => {
            argon2::Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
        }
        Err(_) => false,
    }
}

fn hmac_hex(secret: &str, msg: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn sign_token(secret: &str, expires_at_unix: i64) -> String {
    let exp = expires_at_unix.to_string();
    format!("{exp}.{}", hmac_hex(secret, &exp))
}

pub fn verify_token(secret: &str, token: &str, now_unix: i64) -> bool {
    let Some((exp, sig)) = token.split_once('.') else { return false };
    let Ok(exp) = exp.parse::<i64>() else { return false };
    if now_unix > exp {
        return false;
    }
    let expect = hmac_hex(secret, exp.to_string().as_str());
    expect.as_bytes().ct_eq(sig.as_bytes()).into()
}

fn cookie_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == COOKIE_NAME).then(|| v.to_string())
    })
}

pub async fn auth_mw(State(st): State<AuthState>, req: Request, next: Next) -> Response {
    let now = chrono::Utc::now().timestamp();
    let ok = cookie_from_headers(req.headers())
        .map(|tok| verify_token(&st.password_hash, &tok, now))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        AppError(StatusCode::UNAUTHORIZED, "未登录".into()).into_response()
    }
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(State(st): State<AuthState>, Json(body): Json<LoginBody>) -> ApiResult<Response> {
    if !verify_password(&st.password_hash, &body.password) {
        return Err(AppError(StatusCode::UNAUTHORIZED, "密码错误".into()));
    }
    let now = chrono::Utc::now();
    let token = sign_token(&st.password_hash, now.timestamp() + SESSION_DAYS * 86400);
    let cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 86400
    );
    let mut resp = (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&cookie).unwrap(),
    );
    Ok(resp)
}

async fn logout() -> Response {
    let mut resp = (StatusCode::OK, axum::Json(serde_json::json!({"ok": true}))).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        header::HeaderValue::from_str(&format!("{COOKIE_NAME}=; Path=/; Max-Age=0")).unwrap(),
    );
    resp
}

async fn me() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"ok": true}))
}

pub fn auth_routes() -> Router<AuthState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}
```

`backend/src/bin/pwd_hash.rs`：

```rust
use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;

fn main() {
    let pw = std::env::args().nth(1).expect("用法: cargo run --bin pwd_hash -- <密码>");
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .unwrap()
        .to_string();
    println!("{hash}");
}
```

`lib.rs` 加 `pub mod auth;`。`main.rs`：构建 `let auth_state = auth::AuthState { password_hash: Arc::new(cfg.admin_password_hash.clone()) };` 并把路由改为：

```rust
let app = Router::new()
    .route("/healthz", get(|| async { "ok" }))
    .merge(auth::auth_routes().with_state(auth_state.clone()));
```

（`/api/auth/login` 公开；`me/logout` 在 Task 4 随受保护路由挂上中间件——本任务先全部放行，测试只覆盖 login/mw。）

- [ ] **Step 4: 运行测试通过**

```bash
cargo test --test auth    # 期望：3 个测试 PASS
cargo run --bin pwd_hash -- test123   # 期望：输出 $argon2id$... 哈希串
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(auth): argon2 登录 + HMAC 签名 cookie + 认证中间件

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 4: 汇率存储与 API（latest / history 聚合 / 人工录入）

**Files:**
- Create: `backend/src/rates/mod.rs`、`backend/src/rates/api.rs`、`backend/tests/rates_int.rs`
- Modify: `backend/src/lib.rs`、`backend/src/main.rs`（引入 AppState 并挂受保护路由）

**Interfaces:**
- Consumes: Task 2 的表结构与 pool、Task 3 的 `auth_mw`/`AuthState`
- Produces:
  - `rates::RatePoint { source: String, pair: String, fx_bid: Option<Decimal>, fx_ask: Option<Decimal>, mid: Option<Decimal>, recorded_at: DateTime<Utc>, note: Option<String> }`（`#[derive(Debug, Clone, Serialize)]`）
  - `rates::store::insert_many(&PgPool, &[RatePoint]) -> Result<u64, sqlx::Error>`
  - `rates::store::latest(&PgPool) -> Result<Vec<LatestRate>, sqlx::Error>`，`LatestRate { pair, source, fx_bid, fx_ask, mid, recorded_at }`
  - `rates::store::bucket_secs(from: DateTime<Utc>, to: DateTime<Utc>, max_points: u32) -> i64`（下限 60）
  - `rates::store::history(&PgPool, pair: &str, sources: &[String], from, to, max_points) -> Result<Vec<BucketPoint>, sqlx::Error>`，`BucketPoint { source, t, mid, fx_bid, fx_ask }`（均 Option<Decimal> 除 source/t）
  - `rates::api::routes() -> Router<AppState>`：`GET /api/rates/latest`、`GET /api/rates/history`、`POST /api/rates/manual`
  - `main.rs` 定义 `#[derive(Clone)] pub struct AppState { pool: PgPool, cfg: Arc<AppConfig>, auth: AuthState }`（后续任务扩展此结构）
  - API 认证：`/api` 除 `auth/login` 外全部经 `auth_mw`

- [ ] **Step 1: 写失败测试（bucket_secs 单测 + DB 集成测试）**

`backend/src/rates/mod.rs` 中包含的单元测试（先写测试再写实现，以下测试放入 `backend/tests/rates_int.rs` 的 `#[ignore]` 部分与 mod 内 `#[cfg(test)]`）：

```rust
// src/rates/mod.rs 内 #[cfg(test)]
#[test]
fn bucket_secs_has_floor_and_scales() {
    use chrono::Duration;
    let now = chrono::Utc::now();
    assert_eq!(super::store::bucket_secs(now, now + Duration::seconds(10), 300), 60);
    let day = super::store::bucket_secs(now, now + Duration::hours(24), 288);
    assert!(day >= 300, "24h/288点 → 桶≥300s");
}
```

`backend/tests/rates_int.rs`：

```rust
use chrono::{Duration, Utc};
use fintrack::rates::{store, RatePoint};
use rust_decimal::Decimal;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    fintrack::db::connect(&url).await.unwrap()
}

async fn seed(pool: &PgPool) {
    sqlx::query("DELETE FROM exchange_rates").execute(pool).await.unwrap();
    let now = Utc::now();
    let pts: Vec<RatePoint> = (0..5)
        .map(|i| RatePoint {
            source: "boc".into(),
            pair: "USD/CNY".into(),
            fx_bid: Some(Decimal::new(7100 + i, 3)),
            fx_ask: Some(Decimal::new(7300 + i, 3)),
            mid: None,
            recorded_at: now - Duration::minutes(i * 10),
            note: None,
        })
        .collect();
    store::insert_many(pool, &pts).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn latest_returns_distinct_source_pair() {
    let pool = pool().await;
    seed(&pool).await;
    let rows = store::latest(&pool).await.unwrap();
    let usd_boc = rows.iter().find(|r| r.pair == "USD/CNY" && r.source == "boc").unwrap();
    assert_eq!(usd_boc.fx_bid, Some(Decimal::new(7100, 3)), "最新一条");
}

#[tokio::test]
#[ignore]
async fn history_buckets_points() {
    let pool = pool().await;
    seed(&pool).await;
    let now = Utc::now();
    let rows = store::history(
        &pool, "USD/CNY", &["boc".into()], now - Duration::hours(1), now + Duration::minutes(1), 6,
    ).await.unwrap();
    // 50 分钟跨度、桶下限 60s → 5 点收进 5~6 个桶
    assert!(rows.len() >= 4 && rows.len() <= 6, "got {}", rows.len());
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --lib rates && TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test --test rates_int -- --ignored
# 期望：编译失败（rates 模块不存在）
```

- [ ] **Step 3: 实现 rates/mod.rs**

`backend/src/rates/mod.rs`：

```rust
pub mod api;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::types::chrono as sqlx_chrono; // sqlx chrono 直通，无需额外处理

#[derive(Debug, Clone, Serialize)]
pub struct RatePoint {
    pub source: String,
    pub pair: String,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
    pub mid: Option<Decimal>,
    pub recorded_at: DateTime<Utc>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LatestRate {
    pub pair: String,
    pub source: String,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
    pub mid: Option<Decimal>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BucketPoint {
    pub source: String,
    pub t: DateTime<Utc>,
    pub mid: Option<Decimal>,
    pub fx_bid: Option<Decimal>,
    pub fx_ask: Option<Decimal>,
}

pub mod store {
    use super::*;
    use sqlx::PgPool;

    pub async fn insert_many(pool: &PgPool, pts: &[RatePoint]) -> Result<u64, sqlx::Error> {
        let mut n = 0;
        for p in pts {
            let r = sqlx::query(
                "INSERT INTO exchange_rates (source, pair, fx_bid, fx_ask, mid, recorded_at, note)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&p.source).bind(&p.pair).bind(p.fx_bid).bind(p.fx_ask)
            .bind(p.mid).bind(p.recorded_at).bind(&p.note)
            .execute(pool).await?;
            n += r.rows_affected();
        }
        Ok(n)
    }

    pub async fn latest(pool: &PgPool) -> Result<Vec<LatestRate>, sqlx::Error> {
        sqlx::query_as(
            "SELECT DISTINCT ON (pair, source) pair, source, fx_bid, fx_ask, mid, recorded_at
             FROM exchange_rates ORDER BY pair, source, recorded_at DESC",
        )
        .fetch_all(pool).await
    }

    pub fn bucket_secs(from: DateTime<Utc>, to: DateTime<Utc>, max_points: u32) -> i64 {
        let range = (to - from).num_seconds().max(1);
        (range / max_points.max(1) as i64).max(60)
    }

    pub async fn history(
        pool: &PgPool, pair: &str, sources: &[String],
        from: DateTime<Utc>, to: DateTime<Utc>, max_points: u32,
    ) -> Result<Vec<BucketPoint>, sqlx::Error> {
        let b = bucket_secs(from, to, max_points);
        sqlx::query_as(
            "SELECT source,
                    to_timestamp(floor(extract(epoch FROM recorded_at) / $3) * $3) AS t,
                    avg(mid) AS mid, avg(fx_bid) AS fx_bid, avg(fx_ask) AS fx_ask
             FROM exchange_rates
             WHERE pair = $1 AND source = ANY($2) AND recorded_at >= $4 AND recorded_at < $5
             GROUP BY source, t
             ORDER BY source, t",
        )
        .bind(pair).bind(sources).bind(b).bind(from).bind(to)
        .fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bucket_secs_has_floor_and_scales() {
        use chrono::Duration;
        let now = chrono::Utc::now();
        assert_eq!(super::store::bucket_secs(now, now + Duration::seconds(10), 300), 60);
        let day = super::store::bucket_secs(now, now + Duration::hours(24), 288);
        assert!(day >= 300, "24h/288点 → 桶≥300s");
    }
}
```

（若 `use sqlx::types::chrono` 未用则删除该行。）

- [ ] **Step 4: 实现 rates/api.rs 与 main 组装**

`backend/src/rates/api.rs`：

```rust
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use rust_decimal::Decimal;

use crate::error::{ApiResult, AppError};
use crate::AppState;
use super::{store, RatePoint};

#[derive(Deserialize)]
pub struct ManualBody {
    pair: String,
    fx_bid: Option<Decimal>,
    fx_ask: Option<Decimal>,
    mid: Option<Decimal>,
    note: Option<String>,
}

async fn manual_entry(State(st): State<AppState>, Json(b): Json<ManualBody>) -> ApiResult<Json<serde_json::Value>> {
    if !st.cfg.currency_pairs.iter().any(|p| p == &b.pair) {
        return Err(AppError(StatusCode::BAD_REQUEST, format!("不支持的货币对: {}", b.pair)));
    }
    if b.fx_bid.is_none() && b.fx_ask.is_none() && b.mid.is_none() {
        return Err(AppError(StatusCode::BAD_REQUEST, "至少填一个价格".into()));
    }
    let pt = RatePoint {
        source: "manual".into(),
        pair: b.pair,
        fx_bid: b.fx_bid,
        fx_ask: b.fx_ask,
        mid: b.mid,
        recorded_at: Utc::now(),
        note: b.note,
    };
    store::insert_many(&st.pool, std::slice::from_ref(&pt)).await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn latest(State(st): State<AppState>) -> ApiResult<Json<Vec<super::LatestRate>>> {
    Ok(Json(store::latest(&st.pool).await?))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub pair: String,
    pub sources: Option<String>, // 逗号分隔，默认全部
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub points: Option<u32>,
}

async fn history(State(st): State<AppState>, Query(q): Query<HistoryQuery>) -> ApiResult<Json<serde_json::Value>> {
    let to = q.to.unwrap_or_else(Utc::now);
    let from = q.from.unwrap_or(to - Duration::hours(24));
    let sources: Vec<String> = q.sources
        .map(|s| s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect())
        .unwrap_or_else(|| vec!["api".into(), "boc".into(), "icbc".into(), "manual".into()]);
    let pts = store::history(&st.pool, &q.pair, &sources, from, to, q.points.unwrap_or(300)).await?;
    Ok(Json(serde_json::json!({ "points": pts })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/rates/latest", get(latest))
        .route("/api/rates/history", get(history))
        .route("/api/rates/manual", post(manual_entry))
}
```

`backend/src/main.rs` 改造（本任务定稿 AppState 与路由挂载模式）：

```rust
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

mod nothing_else; // （删除——仅示意；main.rs 实际内容如下）

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: Arc<fintrack::config::AppConfig>,
    pub auth: fintrack::auth::AuthState,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = Arc::new(fintrack::config::AppConfig::load()?);
    let pool = fintrack::db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    fintrack::db::ensure_partitions(&pool).await?;

    let auth_state = fintrack::auth::AuthState {
        password_hash: Arc::new(cfg.admin_password_hash.clone()),
    };
    let state = AppState { pool: pool.clone(), cfg: cfg.clone(), auth: auth_state };

    let login_only = Router::new().route("/api/auth/login", post(fintrack::auth::login_public));
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(fintrack::auth::auth_routes().with_state(auth_state.clone()))
        .merge(fintrack::rates::api::routes())
        .with_state(state)
        .route_layer(axum::middleware::from_fn_with_state(auth_state, fintrack::auth::auth_mw));
    // 注意：auth_routes 里 login 需公开——见下方实现说明
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

**实现说明（重要）**：axum 中 `route_layer` 只对已注册路由生效且后注册的不受影响不可行——正确做法是**分层合并**：

```rust
let public = Router::new()
    .route("/healthz", get(|| async { "ok" }))
    .route("/api/auth/login", post(fintrack::auth::login_handler));

let protected = Router::new()
    .route("/api/auth/logout", post(fintrack::auth::logout_handler))
    .route("/api/auth/me", get(fintrack::auth::me_handler))
    .merge(fintrack::rates::api::routes());

let app = public
    .merge(protected.route_layer(axum::middleware::from_fn_with_state(auth_state.clone(), fintrack::auth::auth_mw)))
    .with_state(state);
```

因此 Task 3 的 `auth.rs` 需微调：把 `login`、`logout`、`me` 三个 handler 改为 `pub`，`auth_routes()` 拆成 `pub fn login_handler/logout_handler/me_handler` 公开导出（Router 组装移入 main）。本任务执行此重构并保持 Task 3 测试通过（测试改为直接调用 `auth_routes()` 若保留，或对 handler 单测）。同时 AppState 定义放 `main.rs` 并在 `lib.rs` 中 `pub use` 不可行（main.rs 不是 lib）——**AppState 放入 `backend/src/lib.rs`**：

```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: Arc<AppConfig>,
    pub auth: crate::auth::AuthState,
}
```

`rates/api.rs` 中的 `use crate::AppState;` 即引用 `fintrack::AppState`。main.rs use `fintrack::AppState`。

- [ ] **Step 5: 运行验证**

```bash
cargo test                       # 单测全 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored   # rates_int PASS
cargo build                      # 零 error（warning 可接受，后续任务消化）
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(rates): 汇率存储 + latest/history 服务端聚合 + 人工录入 API + /api 认证分层

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 5: 基准汇率客户端（frankfurter）与历史回填

**Files:**
- Create: `backend/src/sources/mod.rs`、`backend/src/sources/frankfurter.rs`、`backend/tests/frankfurter.rs`（wiremock 单测 + #[ignore] 集成）

**Interfaces:**
- Consumes: `rates::RatePoint`、`rates::store::insert_many`
- Produces:
  - `frankfurter::BASE_URL = "https://api.frankfurter.dev/v1"`
  - `frankfurter::fetch_latest(client: &reqwest::Client, base: &str) -> Result<RatePoint, String>`（mid=CNY 价，recorded_at=当日 09:00Z）
  - `frankfurter::fetch_range(client, base: &str, from: NaiveDate, to: NaiveDate) -> Result<Vec<RatePoint>, String>`
  - `frankfurter::ingest_api_rates(pool: &PgPool, client: &reqwest::Client, pairs: &[String]) -> Result<usize, String>`（幂等：从各 pair 已存最大日期次日拉到今天；首次回填 365 天）

- [ ] **Step 1: 写失败测试（wiremock）**

`backend/tests/frankfurter.rs`：

```rust
use fintrack::sources::frankfurter;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn parses_latest() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/latest"))
        .and(query_param("base", "USD")).and(query_param("symbols", "CNY"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_string(r#"{"amount":1.0,"base":"USD","date":"2026-11-09","rates":{"CNY":7.1825}}"#))
        .mount(&server).await;
    let c = reqwest::Client::new();
    let pt = frankfurter::fetch_latest_at(&c, &server.uri(), "USD").await.unwrap();
    assert_eq!(pt.pair, "USD/CNY");
    assert_eq!(pt.source, "api");
    assert_eq!(pt.mid.unwrap().to_string(), "7.1825");
    assert_eq!(pt.recorded_at.date_naive().to_string(), "2026-11-09");
}

#[tokio::test]
async fn parses_range() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/2026-11-01..2026-11-03"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"rates":{"2026-11-02":{"CNY":7.18},"2026-11-03":{"CNY":7.19}}}"#))
        .mount(&server).await;
    let c = reqwest::Client::new();
    let pts = frankfurter::fetch_range_at(&c, &server.uri(), "USD",
        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()).await.unwrap();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0].mid.unwrap().to_string(), "7.18");
    assert_eq!(pts[1].recorded_at.date_naive().to_string(), "2026-11-03");
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --test frankfurter   # 期望：编译失败
```

- [ ] **Step 3: 实现**

`backend/src/sources/mod.rs`：

```rust
pub mod frankfurter;
pub mod health;   // Task 8 创建，先注释掉此行
pub mod boc;      // Task 9
pub mod icbc;     // Task 10
```

（本任务只保留 `pub mod frankfurter;`，其余行在对应任务创建时解除注释。）

`backend/src/sources/frankfurter.rs`：

```rust
use crate::rates::RatePoint;
use chrono::{NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashMap;

pub const BASE_URL: &str = "https://api.frankfurter.dev/v1";

#[derive(Deserialize)]
struct LatestResp {
    date: NaiveDate,
    rates: HashMap<String, f64>,
}

#[derive(Deserialize)]
struct RangeResp {
    rates: HashMap<NaiveDate, HashMap<String, f64>>,
}

fn dec(v: f64) -> Decimal {
    Decimal::from_f64_retain(v).unwrap_or_default()
}

fn point(base: &str, date: NaiveDate, cny: f64) -> RatePoint {
    RatePoint {
        source: "api".into(),
        pair: format!("{base}/CNY"),
        fx_bid: None,
        fx_ask: None,
        mid: Some(dec(cny)),
        recorded_at: NaiveDateTime::from_timestamp_opt(date.and_hms_opt(9, 0, 0).unwrap(), 0)
            .unwrap().and_utc(),
        note: None,
    }
}

/// base_url 可注入（测试），生产传 BASE_URL
pub async fn fetch_latest_at(client: &reqwest::Client, base_url: &str, base: &str) -> Result<RatePoint, String> {
    let url = format!("{base_url}/latest?base={base}&symbols=CNY");
    let r: LatestResp = client.get(&url).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    let cny = r.rates.get("CNY").copied().ok_or("响应缺少 CNY")?;
    Ok(point(base, r.date, cny))
}

pub async fn fetch_latest(client: &reqwest::Client, base: &str) -> Result<RatePoint, String> {
    fetch_latest_at(client, BASE_URL, base).await
}

pub async fn fetch_range_at(
    client: &reqwest::Client, base_url: &str, base: &str, from: NaiveDate, to: NaiveDate,
) -> Result<Vec<RatePoint>, String> {
    let url = format!("{base_url}/{from}..{to}?base={base}&symbols=CNY");
    let r: RangeResp = client.get(&url).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    let mut pts: Vec<RatePoint> = r.rates.into_iter()
        .filter_map(|(d, m)| m.get("CNY").map(|v| point(base, d, *v)))
        .collect();
    pts.sort_by_key(|p| p.recorded_at);
    Ok(pts)
}

/// 幂等入库：每对货币从已存最大日期的次日开始增量拉取；首次回填 365 天。返回新增行数。
pub async fn ingest_api_rates(pool: &PgPool, client: &reqwest::Client, pairs: &[String]) -> Result<usize, String> {
    let today = Utc::now().date_naive();
    let mut total = 0usize;
    for pair in pairs {
        let base = pair.split('/').next().unwrap();
        let max: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(recorded_at) FROM exchange_rates WHERE source='api' AND pair=$1")
            .bind(pair).fetch_optional(pool).await.map_err(|e| e.to_string())?
            .flatten();
        let from = max.map(|m| m.date_naive() + chrono::Duration::days(1))
            .unwrap_or(today - chrono::Duration::days(365));
        if from > today {
            continue;
        }
        let pts = fetch_range(client, base, from, today).await?;
        total += crate::rates::store::insert_many(pool, &pts).await.map_err(|e| e.to_string())? as usize;
    }
    Ok(total)
}
```

`lib.rs` 加 `pub mod sources;`（sources/mod.rs 已有）。

- [ ] **Step 4: 运行测试通过 + 集成验证**

```bash
cargo test --test frankfurter   # 期望：2 个 PASS
# 集成（真实 DB + mock 不适用，验证幂等逻辑可直接打真 API，网络允许时）：
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored || true
```

在 `backend/tests/frankfurter.rs` 追加幂等集成测试：

```rust
#[tokio::test]
#[ignore]
async fn ingest_is_idempotent() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM exchange_rates WHERE source='api'").execute(&pool).await.unwrap();
    let c = reqwest::Client::new();
    let n1 = frankfurter::ingest_api_rates(&pool, &c, &["USD/CNY".into()]).await.unwrap();
    assert!(n1 > 200, "首次回填约一年交易日, got {n1}");
    let n2 = frankfurter::ingest_api_rates(&pool, &c, &["USD/CNY".into()]).await.unwrap();
    assert!(n2 <= 1, "二次拉取几乎无新增, got {n2}");
}
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(sources): frankfurter 基准汇率客户端 + 365 天回填 + 幂等入库

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 6: 告警引擎（纯逻辑 + evaluate_all）

**Files:**
- Create: `backend/src/alerts/mod.rs`（模型 + CRUD）、`backend/src/alerts/engine.rs`、`backend/tests/alert_int.rs`

**Interfaces:**
- Consumes: `rates::RatePoint`、表 `alert_rules`/`alert_rule_state`
- Produces:
  - `alerts::Direction { Above, Below }`、`alerts::PriceType { FxBid, FxAsk, Mid }`
  - `alerts::engine::Decision { Trigger, Rearm, Silent }`
  - `alerts::engine::Rule { id: i64, direction: Direction, threshold: Decimal, cooldown_minutes: i64 }`
  - `alerts::engine::RuleState { armed: bool, last_triggered_at: Option<DateTime<Utc>> }`
  - `alerts::engine::evaluate(rule: &Rule, state: &RuleState, value: Decimal, now: DateTime<Utc>) -> Decision`
    语义：未越限 → `Rearm`（武装，不通知）；越限且已武装且冷却已过 → `Trigger`；越限但未武装或冷却未过 → `Silent`
  - `alerts::engine::TriggeredRule { rule_id, source, pair, price_type: String, direction: String, threshold: Decimal, value: Decimal }`，`.message() -> String`
  - `alerts::engine::evaluate_all(pool: &PgPool, pt: &RatePoint, now: DateTime<Utc>) -> Result<Vec<TriggeredRule>, sqlx::Error>`（查询启用规则→逐条 evaluate→写回 state）
  - `alerts::api` CRUD 在本任务一并实现：`alerts::api::routes() -> Router<AppState>`（GET/POST `/api/alert-rules`、PUT/DELETE `/api/alert-rules/:id`）

- [ ] **Step 1: 写失败测试（纯逻辑）**

`backend/src/alerts/engine.rs` 内 `#[cfg(test)]`（先建空文件与测试）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use rust_decimal::Decimal;

    fn rule(direction: Direction) -> Rule {
        Rule { id: 1, direction, threshold: Decimal::new(7300, 2), cooldown_minutes: 60 }
    }

    fn armed() -> RuleState {
        RuleState { armed: true, last_triggered_at: None }
    }

    fn t0() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 10, 0, 0).unwrap()
    }

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn first_breach_triggers() {
        let r = rule(Direction::Above);
        assert_eq!(evaluate(&r, &armed(), d("7.31"), t0()), Decision::Trigger);
    }

    #[test]
    fn below_direction_triggers_on_low() {
        let r = rule(Direction::Below);
        assert_eq!(evaluate(&r, &armed(), d("7.10"), t0()), Decision::Trigger);
        assert_eq!(evaluate(&r, &armed(), d("7.31"), t0()), Decision::Rearm);
    }

    #[test]
    fn still_breached_after_trigger_stays_silent() {
        let r = rule(Direction::Above);
        let st = RuleState { armed: false, last_triggered_at: Some(t0()) };
        assert_eq!(evaluate(&r, &st, d("7.40"), t0() + Duration::hours(2)), Decision::Silent);
    }

    #[test]
    fn recovery_arms_and_rebreach_within_cooldown_is_silent() {
        let r = rule(Direction::Above);
        let triggered = RuleState { armed: false, last_triggered_at: Some(t0()) };
        // 恢复正常 → 再武装
        assert_eq!(evaluate(&r, &triggered, d("7.20"), t0() + Duration::minutes(30)), Decision::Rearm);
        // 冷却内再越限 → 不推（防抖）
        let rearmed = RuleState { armed: true, last_triggered_at: Some(t0()) };
        assert_eq!(evaluate(&r, &rearmed, d("7.31"), t0() + Duration::minutes(30)), Decision::Silent);
        // 冷却过后再越限 → 推
        assert_eq!(evaluate(&r, &rearmed, d("7.31"), t0() + Duration::minutes(61)), Decision::Trigger);
    }
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --lib alerts   # 期望：编译失败
```

- [ ] **Step 3: 实现引擎**

`backend/src/alerts/mod.rs`：

```rust
pub mod api;
pub mod engine;
```

`backend/src/alerts/engine.rs`：

```rust
use crate::rates::RatePoint;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction { Above, Below }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PriceType { FxBid, FxAsk, Mid }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision { Trigger, Rearm, Silent }

pub struct Rule {
    pub id: i64,
    pub direction: Direction,
    pub threshold: Decimal,
    pub cooldown_minutes: i64,
}

pub struct RuleState {
    pub armed: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
}

pub fn evaluate(rule: &Rule, state: &RuleState, value: Decimal, now: DateTime<Utc>) -> Decision {
    let breached = match rule.direction {
        Direction::Above => value >= rule.threshold,
        Direction::Below => value <= rule.threshold,
    };
    if !breached {
        return Decision::Rearm;
    }
    let cooldown_elapsed = state
        .last_triggered_at
        .map_or(true, |t| now.signed_duration_since(t) >= Duration::minutes(rule.cooldown_minutes));
    if state.armed && cooldown_elapsed {
        Decision::Trigger
    } else {
        Decision::Silent
    }
}

#[derive(Debug, Serialize)]
pub struct TriggeredRule {
    pub rule_id: i64,
    pub source: String,
    pub pair: String,
    pub price_type: String,
    pub direction: String,
    pub threshold: Decimal,
    pub value: Decimal,
}

impl TriggeredRule {
    pub fn message(&self) -> String {
        let dir = if self.direction == "above" { "上破" } else { "下破" };
        let field = match self.price_type.as_str() {
            "fx_bid" => "现汇买入价",
            "fx_ask" => "现汇卖出价",
            _ => "中间价",
        };
        format!("{} {}[{}] {} 阈值 {}，当前 {}",
            self.pair, self.source, field, dir, self.threshold, self.value)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct JoinedRow {
    id: i64,
    source: String,
    pair: String,
    price_type: String,
    direction: String,
    threshold: Decimal,
    cooldown_minutes: i64,
    armed: Option<bool>,
    last_triggered_at: Option<DateTime<Utc>>,
}

/// 收到新数据点时评估所有匹配的启用规则；写回状态并返回触发的规则
pub async fn evaluate_all(pool: &PgPool, pt: &RatePoint, now: DateTime<Utc>) -> Result<Vec<TriggeredRule>, sqlx::Error> {
    let rows: Vec<JoinedRow> = sqlx::query_as(
        "SELECT r.id, r.source, r.pair, r.price_type, r.direction, r.threshold, r.cooldown_minutes,
                s.armed, s.last_triggered_at
         FROM alert_rules r
         LEFT JOIN alert_rule_state s ON s.rule_id = r.id
         WHERE r.enabled AND r.source = $1 AND r.pair = $2")
        .bind(&pt.source).bind(&pt.pair)
        .fetch_all(pool).await?;

    let mut triggered = vec![];
    for row in rows {
        let value = match row.price_type.as_str() {
            "fx_bid" => pt.fx_bid,
            "fx_ask" => pt.fx_ask,
            _ => pt.mid,
        };
        let Some(value) = value else { continue };
        let direction = if row.direction == "above" { Direction::Above } else { Direction::Below };
        let rule = Rule {
            id: row.id,
            direction,
            threshold: row.threshold,
            cooldown_minutes: row.cooldown_minutes,
        };
        let state = RuleState {
            armed: row.armed.unwrap_or(true),
            last_triggered_at: row.last_triggered_at,
        };
        match evaluate(&rule, &state, value, now) {
            Decision::Trigger => {
                sqlx::query(
                    "INSERT INTO alert_rule_state (rule_id, armed, last_triggered_at, last_value)
                     VALUES ($1, false, $2, $3)
                     ON CONFLICT (rule_id) DO UPDATE
                       SET armed = false, last_triggered_at = $2, last_value = $3")
                    .bind(row.id).bind(now).bind(value)
                    .execute(pool).await?;
                triggered.push(TriggeredRule {
                    rule_id: row.id,
                    source: row.source,
                    pair: row.pair,
                    price_type: row.price_type,
                    direction: row.direction,
                    threshold: row.threshold,
                    value,
                });
            }
            Decision::Rearm if !state.armed => {
                sqlx::query(
                    "INSERT INTO alert_rule_state (rule_id, armed) VALUES ($1, true)
                     ON CONFLICT (rule_id) DO UPDATE SET armed = true")
                    .bind(row.id).execute(pool).await?;
            }
            _ => {}
        }
    }
    Ok(triggered)
}
```

- [ ] **Step 4: 实现 CRUD API**

`backend/src/alerts/api.rs`：

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::error::{ApiResult, AppError};
use crate::AppState;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct RuleRow {
    pub id: i64,
    pub source: String,
    pub pair: String,
    pub price_type: String,
    pub direction: String,
    pub threshold: Decimal,
    pub cooldown_minutes: i32,
    pub enabled: bool,
}

async fn list(State(st): State<AppState>) -> ApiResult<Json<Vec<RuleRow>>> {
    let rows = sqlx::query_as::<_, RuleRow>(
        "SELECT id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled
         FROM alert_rules ORDER BY id")
        .fetch_all(&st.pool).await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct RuleBody {
    pub source: String,
    pub pair: String,
    pub price_type: String,     // fx_bid | fx_ask | mid
    pub direction: String,      // above | below
    pub threshold: Decimal,
    #[serde(default = "d_cd")]
    pub cooldown_minutes: i32,
    pub enabled: bool,
}

fn d_cd() -> i32 { 60 }

fn validate(b: &RuleBody) -> Result<(), AppError> {
    let ok_pt = matches!(b.price_type.as_str(), "fx_bid" | "fx_ask" | "mid");
    let ok_dir = matches!(b.direction.as_str(), "above" | "below");
    let ok_src = matches!(b.source.as_str(), "api" | "boc" | "icbc" | "manual");
    if ok_pt && ok_dir && ok_src {
        Ok(())
    } else {
        Err(AppError(StatusCode::BAD_REQUEST, "price_type/direction/source 非法".into()))
    }
}

async fn create(State(st): State<AppState>, Json(b): Json<RuleBody>) -> ApiResult<Json<RuleRow>> {
    validate(&b)?;
    let row = sqlx::query_as::<_, RuleRow>(
        "INSERT INTO alert_rules (source, pair, price_type, direction, threshold, cooldown_minutes, enabled)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled")
        .bind(&b.source).bind(&b.pair).bind(&b.price_type).bind(&b.direction)
        .bind(b.threshold).bind(b.cooldown_minutes).bind(b.enabled)
        .fetch_one(&st.pool).await?;
    Ok(Json(row))
}

async fn update(State(st): State<AppState>, Path(id): Path<i64>, Json(b): Json<RuleBody>) -> ApiResult<Json<RuleRow>> {
    validate(&b)?;
    let row = sqlx::query_as::<_, RuleRow>(
        "UPDATE alert_rules SET source=$2, pair=$3, price_type=$4, direction=$5,
                threshold=$6, cooldown_minutes=$7, enabled=$8
         WHERE id=$1
         RETURNING id, source, pair, price_type, direction, threshold, cooldown_minutes, enabled")
        .bind(id).bind(&b.source).bind(&b.pair).bind(&b.price_type).bind(&b.direction)
        .bind(b.threshold).bind(b.cooldown_minutes).bind(b.enabled)
        .fetch_optional(&st.pool).await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("规则 {id} 不存在")))?;
    Ok(Json(row))
}

async fn delete(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    let r = sqlx::query("DELETE FROM alert_rules WHERE id=$1").bind(id).execute(&st.pool).await?;
    if r.rows_affected() == 0 {
        return Err(AppError(StatusCode::NOT_FOUND, format!("规则 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/alert-rules", get(list).post(create))
        .route("/api/alert-rules/:id", put(update).delete(delete))
}
```

`lib.rs` 加 `pub mod alerts;`。`main.rs` 的 `protected` 合并 `.merge(fintrack::alerts::api::routes())`。

- [ ] **Step 5: 集成测试 evaluate_all**

`backend/tests/alert_int.rs`：

```rust
use chrono::Utc;
use fintrack::alerts::engine;
use fintrack::rates::{store, RatePoint};
use rust_decimal::Decimal;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    fintrack::db::connect(&url).await.unwrap()
}

#[tokio::test]
#[ignore]
async fn evaluate_all_triggers_once_then_rearms() {
    let pool = pool().await;
    sqlx::query("DELETE FROM alert_rules").execute(&pool).await.unwrap();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO alert_rules (source, pair, price_type, direction, threshold)
         VALUES ('api','USD/CNY','mid','above',7.10) RETURNING id")
        .fetch_one(&pool).await.unwrap();

    let mk = |mid: Decimal| RatePoint {
        source: "api".into(), pair: "USD/CNY".into(),
        fx_bid: None, fx_ask: None, mid: Some(mid),
        recorded_at: Utc::now(), note: None,
    };

    let t1 = engine::evaluate_all(&pool, &mk(Decimal::new(711, 2)), Utc::now()).await.unwrap();
    assert_eq!(t1.len(), 1);
    assert_eq!(t1[0].rule_id, id);
    let t2 = engine::evaluate_all(&pool, &mk(Decimal::new(715, 2)), Utc::now()).await.unwrap();
    assert!(t2.is_empty(), "仍越限不重推");
    engine::evaluate_all(&pool, &mk(Decimal::new(700, 2)), Utc::now()).await.unwrap(); // 恢复→再武装
    let armed: bool = sqlx::query_scalar("SELECT armed FROM alert_rule_state WHERE rule_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert!(armed);
}
```

- [ ] **Step 6: 运行验证 + Commit**

```bash
cargo test   # 引擎 4 个单测 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored
git add -A && git commit -m "feat(alerts): 告警引擎（触发/冷却/再武装）+ 规则 CRUD API

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 7: 通知模块（Notifier trait + 企业微信 + Telegram + 重试入库）

**Files:**
- Create: `backend/src/notify/mod.rs`、`backend/src/notify/wecom.rs`、`backend/src/notify/telegram.rs`、`backend/src/notify/api.rs`、`backend/tests/notify_int.rs`

**Interfaces:**
- Consumes: 表 `notifications`
- Produces:
  - `notify::Notifier` trait：`fn channel(&self) -> &'static str;` + `async fn send(&self, title: &str, body: &str) -> Result<(), String>`
  - `notify::wecom::WecomNotifier { webhook_url: String, client: reqwest::Client }`（msgtype=markdown，校验 errcode==0）
  - `notify::telegram::TelegramNotifier { bot_token, chat_id, api_base: String, client }`（`api_base` 默认 `https://api.telegram.org`，可注入测试）
  - `notify::telegram::build_client(proxy: Option<&str>) -> reqwest::Client`（socks5 代理）
  - `notify::dispatch(pool, notifiers: &[Arc<dyn Notifier>], title, body).await`（内部 backoff 1s）
  - `notify::dispatch_with_backoff(pool, notifiers, title, body, base: Duration)`：每渠道先 INSERT pending → 最多 3 次尝试（指数退避 base*2^n）→ UPDATE sent/failed+retries+error
  - `notify::api::routes() -> Router<AppState>`：`GET /api/notifications`（Query: channel/status/page/page_size）

- [ ] **Step 1: 写失败测试**

`backend/tests/notify_int.rs`（send 用 wiremock 无 DB；dispatch 用 DB）：

```rust
use std::sync::Arc;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn wecom_send_ok_and_errcode_check() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/webhook/send"))
        .and(body_partial_json(serde_json::json!({"msgtype": "markdown"})))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"errcode":0,"errmsg":"ok"}"#))
        .mount(&server).await;
    let n = fintrack::notify::wecom::WecomNotifier {
        webhook_url: format!("{}/webhook/send", server.uri()),
        client: reqwest::Client::new(),
    };
    assert!(n.send("标题", "内容").await.is_ok());
}

#[tokio::test]
async fn telegram_send_uses_sendmessage() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/bot123:abc/sendMessage"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true,"result":{"message_id":1}}"#))
        .mount(&server).await;
    let n = fintrack::notify::telegram::TelegramNotifier {
        bot_token: "123:abc".into(), chat_id: "10086".into(),
        api_base: server.uri(), client: reqwest::Client::new(),
    };
    assert!(n.send("标题", "内容").await.is_ok());
}

#[tokio::test]
#[ignore]
async fn dispatch_retries_three_times_then_marks_failed() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM notifications").execute(&pool).await.unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/webhook/send"))
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&server).await;

    let n = Arc::new(fintrack::notify::wecom::WecomNotifier {
        webhook_url: format!("{}/webhook/send", server.uri()),
        client: reqwest::Client::new(),
    });
    let notifiers: Vec<Arc<dyn fintrack::notify::Notifier>> = vec![n];
    fintrack::notify::dispatch_with_backoff(
        &pool, &notifiers, "t", "b", std::time::Duration::from_millis(1)).await;

    let row: (String, i32) = sqlx::query_as(
        "SELECT status, retries FROM notifications WHERE channel='wecom' ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(row, ("failed".into(), 3));
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --test notify_int   # 期望：编译失败
```

- [ ] **Step 3: 实现**

`backend/src/notify/mod.rs`：

```rust
pub mod api;
pub mod telegram;
pub mod wecom;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

#[async_trait]
pub trait Notifier: Send + Sync {
    fn channel(&self) -> &'static str;
    async fn send(&self, title: &str, body: &str) -> Result<(), String>;
}

pub async fn dispatch(pool: &PgPool, notifiers: &[Arc<dyn Notifier>], title: &str, body: &str) {
    dispatch_with_backoff(pool, notifiers, title, body, Duration::from_secs(1)).await;
}

pub async fn dispatch_with_backoff(
    pool: &PgPool, notifiers: &[Arc<dyn Notifier>], title: &str, body: &str, base: Duration,
) {
    for n in notifiers {
        let id: i64 = sqlx::query(
            "INSERT INTO notifications (channel, title, body, status) VALUES ($1,$2,$3,'pending') RETURNING id")
            .bind(n.channel()).bind(title).bind(body)
            .fetch_one(pool).await
            .map(|r| r.get(0).unwrap_or(0i64))
            .unwrap_or(0);
        let (status, retries, err) = try_send(n, title, body, base).await;
        let _ = sqlx::query("UPDATE notifications SET status=$2, retries=$3, error=$4 WHERE id=$1")
            .bind(id).bind(status).bind(retries as i32).bind(err)
            .execute(pool).await;
    }
}

async fn try_send(
    n: &Arc<dyn Notifier>, title: &str, body: &str, base: Duration,
) -> (&'static str, u32, Option<String>) {
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match n.send(title, body).await {
            Ok(()) => return ("sent", attempts, None),
            Err(e) if attempts >= 3 => return ("failed", attempts, Some(e)),
            Err(e) => {
                tracing::warn!("{} 推送失败(第{attempts}次): {e}", n.channel());
                tokio::time::sleep(base * 2u32.pow(attempts - 1)).await;
            }
        }
    }
}
```

注：`fetch_one` 取 RETURNING id 用 `sqlx::query_scalar` 更直接——实现时写为：

```rust
let id: i64 = sqlx::query_scalar(
    "INSERT INTO notifications (channel, title, body, status) VALUES ($1,$2,$3,'pending') RETURNING id")
    .bind(n.channel()).bind(title).bind(body)
    .fetch_one(pool).await.unwrap_or(0);
```

`backend/src/notify/wecom.rs`：

```rust
use super::Notifier;
use async_trait::async_trait;

pub struct WecomNotifier {
    pub webhook_url: String,
    pub client: reqwest::Client,
}

#[async_trait]
impl Notifier for WecomNotifier {
    fn channel(&self) -> &'static str { "wecom" }

    async fn send(&self, title: &str, body: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "msgtype": "markdown",
            "markdown": { "content": format!("**{title}**\n{body}") },
        });
        let resp: serde_json::Value = self.client.post(&self.webhook_url)
            .json(&payload).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        if resp["errcode"].as_i64() == Some(0) {
            Ok(())
        } else {
            Err(format!("wecom errcode: {}", resp["errcode"]))
        }
    }
}
```

`backend/src/notify/telegram.rs`：

```rust
use super::Notifier;
use async_trait::async_trait;

pub struct TelegramNotifier {
    pub bot_token: String,
    pub chat_id: String,
    pub api_base: String,
    pub client: reqwest::Client,
}

pub fn build_client(proxy: Option<&str>) -> reqwest::Client {
    let mut b = reqwest::Client::builder();
    if let Some(p) = proxy {
        b = b.proxy(reqwest::Proxy::all(p).expect("非法代理地址"));
    }
    b.build().expect("reqwest client 构建失败")
}

#[async_trait]
impl Notifier for TelegramNotifier {
    fn channel(&self) -> &'static str { "telegram" }

    async fn send(&self, title: &str, body: &str) -> Result<(), String> {
        let url = format!("{}/bot{}/sendMessage", self.api_base, self.bot_token);
        let payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": format!("{title}\n{body}"),
        });
        let resp: serde_json::Value = self.client.post(&url)
            .json(&payload).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        if resp["ok"].as_bool() == Some(true) {
            Ok(())
        } else {
            Err(format!("telegram: {}", resp["description"]))
        }
    }
}
```

`backend/src/notify/api.rs`：

```rust
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    pub channel: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub page: i64,
    #[serde(default = "d_size")]
    pub page_size: i64,
}

fn d_size() -> i64 { 20 }

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct NotificationRow {
    pub id: i64,
    pub channel: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub retries: i32,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn list(State(st): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<serde_json::Value>> {
    let page = q.page.max(1);
    let size = q.page_size.clamp(1, 100);
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT id, channel, title, body, status, retries, error, created_at
         FROM notifications
         WHERE ($1::text IS NULL OR channel = $1)
           AND ($2::text IS NULL OR status = $2)
         ORDER BY id DESC LIMIT $3 OFFSET $4")
        .bind(&q.channel).bind(&q.status).bind(size).bind((page - 1) * size)
        .fetch_all(&st.pool).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications
         WHERE ($1::text IS NULL OR channel = $1) AND ($2::text IS NULL OR status = $2)")
        .bind(&q.channel).bind(&q.status)
        .fetch_one(&st.pool).await?;
    Ok(Json(serde_json::json!({ "rows": rows, "total": total })))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/notifications", get(list))
}
```

`lib.rs` 加 `pub mod notify;`，main.rs `protected` 合并 `.merge(fintrack::notify::api::routes())`。

- [ ] **Step 4: 运行验证 + Commit**

```bash
cargo test --test notify_int   # 2 个 wiremock 测试 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test --test notify_int -- --ignored
git add -A && git commit -m "feat(notify): Notifier 抽象 + 企业微信/Telegram + 重试入库 + 查询 API

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 8: 数据源健康跟踪（HealthRegistry）

**Files:**
- Create: `backend/src/sources/health.rs`（`sources/mod.rs` 解开 `pub mod health;`）

**Interfaces:**
- Produces:
  - `health::HealthEvent { WentDown, Recovered }`
  - `health::HealthTracker`（单源内存状态：`report(&mut self, ok: bool) -> Option<HealthEvent>`，连续 3 次失败→WentDown，成功→清零、若之前 down→Recovered）
  - `health::SourceSnapshot { name: String, is_down: bool, consecutive_fails: u32, last_ok_at: Option<DateTime<Utc>> }`
  - `health::HealthRegistry`：`new() -> Self`、`report(&self, name: &str, ok: bool) -> Option<HealthEvent>`（内部 Mutex<HashMap>）、`snapshots(&self) -> Vec<SourceSnapshot>`（按 name 排序）

- [ ] **Step 1: 写失败测试**（放 `health.rs` 的 `#[cfg(test)]`）

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_consecutive_failures_then_recovery() {
        let mut t = HealthTracker::new();
        assert_eq!(t.report(false), None);
        assert_eq!(t.report(false), None);
        assert_eq!(t.report(false), Some(HealthEvent::WentDown)); // 第 3 次
        assert_eq!(t.report(false), None);                         // 已 down 不重复
        assert_eq!(t.report(true), Some(HealthEvent::Recovered));
        assert_eq!(t.report(true), None);
    }

    #[test]
    fn intermittent_failure_never_goes_down() {
        let mut t = HealthTracker::new();
        for _ in 0..10 {
            assert_eq!(t.report(false), None);
            assert_eq!(t.report(true), None); // 成功即清零
        }
    }

    #[test]
    fn registry_stores_snapshots() {
        let r = HealthRegistry::new();
        r.report("boc", false);
        r.report("boc", false);
        r.report("boc", false);
        r.report("api", true);
        let s = r.snapshots();
        let boc = s.iter().find(|x| x.name == "boc").unwrap();
        assert!(boc.is_down && boc.consecutive_fails == 3);
        let api = s.iter().find(|x| x.name == "api").unwrap();
        assert!(!api.is_down && api.last_ok_at.is_some());
    }
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --lib health   # 期望：编译失败
```

- [ ] **Step 3: 实现**

`backend/src/sources/health.rs`：

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthEvent {
    WentDown,
    Recovered,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceSnapshot {
    pub name: String,
    pub is_down: bool,
    pub consecutive_fails: u32,
    pub last_ok_at: Option<DateTime<Utc>>,
}

pub struct HealthTracker {
    consecutive_fails: u32,
    is_down: bool,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self { consecutive_fails: 0, is_down: false }
    }

    pub fn report(&mut self, ok: bool) -> Option<HealthEvent> {
        if ok {
            self.consecutive_fails = 0;
            if self.is_down {
                self.is_down = false;
                return Some(HealthEvent::Recovered);
            }
            None
        } else {
            self.consecutive_fails += 1;
            if !self.is_down && self.consecutive_fails >= 3 {
                self.is_down = true;
                return Some(HealthEvent::WentDown);
            }
            None
        }
    }
}

impl Default for HealthTracker {
    fn default() -> Self { Self::new() }
}

#[derive(Default)]
struct Inner {
    tracker: HealthTracker,
    last_ok_at: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct HealthRegistry {
    inner: Mutex<BTreeMap<String, Inner>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&self, name: &str, ok: bool) -> Option<HealthEvent> {
        let mut map = self.inner.lock().unwrap();
        let e = map.entry(name.to_string()).or_default();
        let ev = e.tracker.report(ok);
        if ok {
            e.last_ok_at = Some(Utc::now());
        }
        ev
    }

    pub fn snapshots(&self) -> Vec<SourceSnapshot> {
        let map = self.inner.lock().unwrap();
        map.iter().map(|(name, e)| SourceSnapshot {
            name: name.clone(),
            is_down: e.tracker.is_down,
            consecutive_fails: e.tracker.consecutive_fails,
            last_ok_at: e.last_ok_at,
        }).collect()
    }
}
```

- [ ] **Step 4: 运行 + Commit**

```bash
cargo test --lib health   # 3 个 PASS
git add -A && git commit -m "feat(sources): 数据源健康跟踪（连续3次失败判失效/恢复上报）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 9: 中行（BOC）抓取器

**Files:**
- Create: `backend/tests/fixtures/boc.html`、`backend/tests/boc_parser.rs`
- Create: `backend/src/sources/boc.rs`（`sources/mod.rs` 解开 `pub mod boc;`）

**Interfaces:**
- Consumes: `rates::RatePoint`、`HealthRegistry`
- Produces:
  - `boc::fetch(client: &reqwest::Client) -> Result<String, String>`（GET `https://www.boc.cn/sourcedb/whpj/`，带浏览器 UA）
  - `boc::parse(html: &str, default_now: DateTime<Utc>) -> Result<Vec<RatePoint>, String>`（source="boc"；时间解析 `%Y.%m.%d %H:%M:%S` 按 Asia/Shanghai 转 Utc）
  - 列映射常量：`const COL_BID: usize = 1; const COL_ASK: usize = 3; const COL_MID: usize = 5; const COL_TIME: usize = 6;`（首列为币种名）
  - 币种名映射：美元→USD、欧元→EUR、日元→JPY、港币→HKD、英镑→GBP、澳元→AUD

- [ ] **Step 1: 拉取真实页面存 fixture**

```bash
mkdir -p backend/tests/fixtures
curl -s -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36" \
  "https://www.boc.cn/sourcedb/whpj/" -o backend/tests/fixtures/boc.html
head -c 500 backend/tests/fixtures/boc.html   # 确认非空且为 HTML
```

若网络不可达，用以下合成 fixture 落盘（结构按中行牌价表列序：币种/现汇买入/现钞买入/现汇卖出/现钞卖出/中行折算价/发布时间）：

```html
<html><body>
<table>
<tr>
<td>美元</td><td>712.35</td><td>705.82</td><td>715.29</td><td>715.29</td><td>710.05</td><td>2026.11.10 14:30:12</td>
</tr>
<tr>
<td>澳元</td><td>462.10</td><td>457.98</td><td>465.52</td><td>465.52</td><td>463.12</td><td>2026.11.10 14:30:12</td>
</tr>
</table>
</body></html>
```

- [ ] **Step 2: 校验列序**

打开 fixture 搜索"美元"行，确认 `<td>` 顺序为：币种名、现汇买入价、现钞买入价、现汇卖出价、现钞卖出价、中行折算价、发布时间（中行官网牌价页长期为此结构）。**若与上述不符，调整 COL_* 常量使其对齐**，并同步修改下方断言中的期望值。

- [ ] **Step 3: 写失败测试**

`backend/tests/boc_parser.rs`：

```rust
use chrono::{TimeZone, Utc};

#[test]
fn parses_usd_and_aud_rows() {
    let html = std::fs::read_to_string("tests/fixtures/boc.html").unwrap();
    let now = Utc::now();
    let pts = fintrack::sources::boc::parse(&html, now).unwrap();
    let usd = pts.iter().find(|p| p.pair == "USD/CNY").expect("应解析出美元");
    assert_eq!(usd.source, "boc");
    assert!(usd.fx_bid.is_some_and(|v| v > rust_decimal::Decimal::new(600, 2)), "现汇买入价应>6.00");
    assert!(usd.fx_ask.is_some_and(|v| v > rust_decimal::Decimal::new(600, 2)));
    let aud = pts.iter().find(|p| p.pair == "AUD/CNY");
    assert!(aud.is_some(), "fixture 应含澳元行");
}

#[test]
fn rejects_html_without_rows() {
    let r = fintrack::sources::boc::parse("<html><body>维护中</body></html>", Utc::now());
    assert!(r.is_err());
}

#[test]
fn boc_time_converts_shanghai_to_utc() {
    // 2026.11.10 14:30:12 +08:00 == 06:30:12Z
    let t = fintrack::sources::boc::parse_time("2026.11.10 14:30:12");
    let expect = Utc.with_ymd_and_hms(2026, 11, 10, 6, 30, 12).unwrap();
    assert_eq!(t, Some(expect));
}
```

- [ ] **Step 4: 运行确认失败**

```bash
cargo test --test boc_parser   # 期望：编译失败
```

- [ ] **Step 5: 实现 boc.rs**

`backend/src/sources/boc.rs`：

```rust
use crate::rates::RatePoint;
use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Asia::Shanghai;
use rust_decimal::Decimal;
use scraper::{Html, Selector};

// 中行牌价列序（0 基）：币种名 | 现汇买入 | 现钞买入 | 现汇卖出 | 现钞卖出 | 中行折算价 | 发布时间
const COL_BID: usize = 1;
const COL_ASK: usize = 3;
const COL_MID: usize = 5;
const COL_TIME: usize = 6;

const NAME2PAIR: &[(&str, &str)] = &[
    ("美元", "USD"), ("欧元", "EUR"), ("日元", "JPY"),
    ("港币", "HKD"), ("英镑", "GBP"), ("澳元", "AUD"),
];

pub const PAGE_URL: &str = "https://www.boc.cn/sourcedb/whpj/";

pub async fn fetch(client: &reqwest::Client) -> Result<String, String> {
    client.get(PAGE_URL)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .text().await.map_err(|e| e.to_string())
}

pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y.%m.%d %H:%M:%S").ok()?
        .and_local_timezone(Shanghai).single()
}

pub fn parse(html: &str, default_now: DateTime<Utc>) -> Result<Vec<RatePoint>, String> {
    let doc = Html::parse_document(html);
    let tr = Selector::parse("tr").unwrap();
    let td = Selector::parse("td").unwrap();
    let mut out = vec![];
    for row in doc.select(&tr) {
        let cells: Vec<String> = row.select(&td)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();
        if cells.len() <= COL_TIME {
            continue;
        }
        let Some(code) = NAME2PAIR.iter().find(|(n, _)| cells[0].contains(n)).map(|(_, c)| *c) else {
            continue;
        };
        let dec = |i: usize| cells.get(i).and_then(|s| s.parse::<Decimal>().ok());
        let (bid, ask, mid) = (dec(COL_BID), dec(COL_ASK), dec(COL_MID));
        if bid.is_none() && ask.is_none() && mid.is_none() {
            continue;
        }
        let recorded_at = parse_time(&cells[COL_TIME]).unwrap_or(default_now);
        // 中行牌价为 100 外币兑人民币 → 换算为 1 外币
        let scale = |d: Option<Decimal>| d.map(|d| d / Decimal::from(100));
        out.push(RatePoint {
            source: "boc".into(),
            pair: format!("{code}/CNY"),
            fx_bid: scale(bid),
            fx_ask: scale(ask),
            mid: scale(mid),
            recorded_at,
            note: None,
        });
    }
    if out.is_empty() {
        Err("BOC 解析失败：未匹配到任何币种行".into())
    } else {
        Ok(out)
    }
}
```

注意：**中行官网牌价单位是 100 外币兑人民币**（如美元行显示 712.35 表示 100 美元兑 712.35 元），上面已除以 100 归一。验证 fixture 中美元现汇买入价 ≈ 712（百元单位）→ 存库后 ≈ 7.12。若 Step 1 用的是其他单位页，调整 scale。

- [ ] **Step 6: 运行 + Commit**

```bash
cargo test --test boc_parser   # 3 个 PASS（AUD 断言视 fixture 内容，必要时按 Step 3 注释调整）
git add -A && git commit -m "feat(sources): 中行牌价抓取器（fixture 驱动解析 + 单位归一）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 10: 工行（ICBC）抓取器

**Files:**
- Create: `backend/tests/fixtures/icbc.html`、`backend/tests/icbc_parser.rs`、`backend/src/sources/icbc.rs`

**Interfaces:**
- Produces（与 boc 对齐）：
  - `icbc::fetch(client) -> Result<String, String>`、`icbc::parse(html, default_now) -> Result<Vec<RatePoint>, String>`（source="icbc"，工行牌价同为 100 外币单位，除以 100 归一）
  - 列映射常量与 boc 相同结构：币种名/现汇买入/现钞买入/现汇卖出/…/发布时间

**⚠️ 前置事实核验（执行者必做）**：工行官网为 JS 渲染，静态 HTML 可能不含数据。按顺序尝试：

```bash
# 候选 1：牌价数据接口（历史上有静态 HTML 片段返回）
curl -s -A "Mozilla/5.0" \
  "https://www.icbc.com.cn/ICBCDynamicSite2/Charts/SiteDefault.aspx?Type=36" \
  -o backend/tests/fixtures/icbc.html
# 候选 2：外汇牌价栏目页
curl -s -A "Mozilla/5.0" "https://www.icbc.com.cn/column/1438058341489590609.html" \
  -o backend/tests/fixtures/icbc.html
head -c 500 backend/tests/fixtures/icbc.html
```

- 若拿到含币种/价格表格的 HTML → 按 fixture 实际列序配置 `COL_*` 常量，流程同 Task 9。
- 若两个候选都拿不到可用数据 → **用 Task 9 的合成 fixture 模板落盘 icbc.html**（币种列改含"美元/英镑"），解析器照常实现并通过测试；在 `icbc.rs` 顶部加注释记录"线上端点待核验"，`fetch()` 返回 `Err("icbc 端点待核验")`，健康跟踪会把它标记为失效源（系统其余部分不受影响）。README 记入已知事项。

- [ ] **Step 1-6：同 Task 9 流程**（拉 fixture → 核验列序 → 失败测试 → 实现 → 通过 → commit）

测试 `backend/tests/icbc_parser.rs` 关键断言：

```rust
#[test]
fn parses_icbc_fixture_rows() {
    let html = std::fs::read_to_string("tests/fixtures/icbc.html").unwrap();
    let pts = fintrack::sources::icbc::parse(&html, chrono::Utc::now()).unwrap();
    assert!(pts.iter().any(|p| p.pair == "USD/CNY" && p.source == "icbc"));
    assert!(pts.iter().all(|p| p.fx_bid.map_or(true, |v| v > rust_decimal::Decimal::ZERO)));
}

#[test]
fn rejects_empty_html() {
    assert!(fintrack::sources::icbc::parse("<html></html>", chrono::Utc::now()).is_err());
}
```

`icbc.rs` 实现与 `boc.rs` 相同骨架（复制后改）：`source: "icbc"`、`PAGE_URL` 用核验出的端点、`NAME2PAIR` 按工行币种名（"美元/欧元/日元/港币/英镑/澳元"或"美元/英镑/港币…"按 fixture 实名调整 `contains` 匹配）。

```bash
cargo test --test icbc_parser
git add -A && git commit -m "feat(sources): 工行牌价抓取器（fixture 驱动）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 11: 存款模块（纯函数 + CRUD + summary API）

**Files:**
- Create: `backend/src/deposits/mod.rs`、`backend/src/deposits/api.rs`、`backend/tests/deposits_int.rs`
- Modify: `lib.rs`、`main.rs`（挂 `/api/deposits`）

**Interfaces:**
- Consumes: 表 `deposits`
- Produces:
  - `deposits::maturity_interest(amount: Decimal, rate_pct: Decimal, term_months: i32) -> Decimal`（= amount×rate%×月/12）
  - `deposits::compute_maturity(start: NaiveDate, term_months: i32) -> NaiveDate`（chrono 月份钳制：1-31 + 1月 → 2-28）
  - `deposits::Tier { D7, D1, D0 }`、`tier_for(days_left: i64) -> Option<Tier>`（仅 7/1/0）
  - `deposits::DepositLike { remind_7d, remind_1d, remind_today: bool, notified_7d/1d/today: Option<NaiveDate>, maturity_date: NaiveDate }`
  - `deposits::should_notify(d: &DepositLike, today: NaiveDate) -> Option<Tier>`
  - `#[derive(Serialize, FromRow)] deposits::Deposit`（全字段；`amount/rate: Decimal`）
  - `deposits::create/list/update/delete`（pool 函数）、`summary(pool) -> Vec<CurrencySummary>`、`due_in_30d(pool) -> Vec<Deposit>`
  - `deposits::api::routes() -> Router<AppState>`：GET/POST `/api/deposits`、PUT/DELETE `/api/deposits/:id`、GET `/api/deposits/summary`
  - 创建/更新请求体 `DepositBody { person, bank, currency(默认CNY), amount, rate, start_date, term_months, remind_7d/1d/today(默认true), note? }`；`maturity_date` 由服务端按 `compute_maturity` 计算，不接受客户端传入

- [ ] **Step 1: 写失败测试**

`backend/src/deposits/mod.rs` 内 `#[cfg(test)]`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn d(s: &str) -> Decimal { Decimal::from_str(s).unwrap() }
    fn date(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }

    #[test]
    fn interest_math() {
        // 10000 元年利率 2.5% 存 12 个月 → 250
        assert_eq!(maturity_interest(d("10000"), d("2.5"), 12), d("250"));
        // 10000 元年利率 2.0% 存 3 个月 → 50
        assert_eq!(maturity_interest(d("10000"), d("2.0"), 3), d("50"));
    }

    #[test]
    fn maturity_clamps_month_end() {
        assert_eq!(compute_maturity(date("2026-01-31"), 1), date("2026-02-28"));
        assert_eq!(compute_maturity(date("2026-11-10"), 36), date("2029-11-10"));
    }

    #[test]
    fn tier_only_at_7_1_0_days() {
        assert_eq!(tier_for(7), Some(Tier::D7));
        assert_eq!(tier_for(1), Some(Tier::D1));
        assert_eq!(tier_for(0), Some(Tier::D0));
        assert_eq!(tier_for(3), None);
        assert_eq!(tier_for(-1), None);
    }

    fn like(maturity: &str, today: &str, flags: (bool, bool, bool), notified: Option<NaiveDate>) -> DepositLike {
        DepositLike {
            remind_7d: flags.0, remind_1d: flags.1, remind_today: flags.2,
            notified_7d: notified, notified_1d: notified, notified_today: notified,
            maturity_date: date(maturity),
        }
    }

    #[test]
    fn should_notify_respects_tier_flag_and_dedupe() {
        let today = date("2026-11-10");
        // 7 天档
        assert_eq!(should_notify(&like("2026-11-17", "2026-11-10", (true, true, true), None), today), Some(Tier::D7));
        // 档位关闭
        assert_eq!(should_notify(&like("2026-11-17", "2026-11-10", (false, true, true), None), today), None);
        // 当天已推过 → 不再推
        assert_eq!(should_notify(&like("2026-11-10", "2026-11-10", (true, true, true), Some(today)), today), None);
        // 隔天重推同一档允许（notified 仅记日期）
        assert_eq!(should_notify(&like("2026-11-11", "2026-11-10", (true, true, true), Some(date("2026-11-09"))), today), Some(Tier::D1));
    }
}
```

`backend/tests/deposits_int.rs`：

```rust
use std::str::FromStr;
use rust_decimal::Decimal;

#[tokio::test]
#[ignore]
async fn crud_and_summary() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM deposits").execute(&pool).await.unwrap();

    let id = fintrack::deposits::create(&pool, &fintrack::deposits::NewDeposit {
        person: "张三".into(), bank: "中行".into(), currency: "CNY".into(),
        amount: Decimal::from_str("10000").unwrap(), rate: Decimal::from_str("2.5").unwrap(),
        start_date: chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(), term_months: 12,
        remind_7d: true, remind_1d: true, remind_today: true, note: None,
    }).await.unwrap();
    assert!(id > 0);

    let s = fintrack::deposits::summary(&pool).await.unwrap();
    let cny = s.iter().find(|x| x.currency == "CNY").unwrap();
    assert_eq!(cny.count, 1);
    assert_eq!(cny.total_maturity, Decimal::from_str("10250").unwrap()); // 10000+250
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test --lib deposits   # 期望：编译失败
```

- [ ] **Step 3: 实现 deposits/mod.rs**

```rust
pub mod api;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

pub fn maturity_interest(amount: Decimal, rate_pct: Decimal, term_months: i32) -> Decimal {
    amount * rate_pct / Decimal::from(100) * Decimal::from(term_months) / Decimal::from(12)
}

pub fn compute_maturity(start: NaiveDate, term_months: i32) -> NaiveDate {
    start.checked_add_months(chrono::Months::new(term_months as u32)).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { D7, D1, D0 }

pub fn tier_for(days_left: i64) -> Option<Tier> {
    match days_left {
        7 => Some(Tier::D7),
        1 => Some(Tier::D1),
        0 => Some(Tier::D0),
        _ => None,
    }
}

pub struct DepositLike {
    pub remind_7d: bool,
    pub remind_1d: bool,
    pub remind_today: bool,
    pub notified_7d: Option<NaiveDate>,
    pub notified_1d: Option<NaiveDate>,
    pub notified_today: Option<NaiveDate>,
    pub maturity_date: NaiveDate,
}

pub fn should_notify(d: &DepositLike, today: NaiveDate) -> Option<Tier> {
    let days = (d.maturity_date - today).num_days();
    match tier_for(days) {
        Some(Tier::D7) if d.remind_7d && d.notified_7d != Some(today) => Some(Tier::D7),
        Some(Tier::D1) if d.remind_1d && d.notified_1d != Some(today) => Some(Tier::D1),
        Some(Tier::D0) if d.remind_today && d.notified_today != Some(today) => Some(Tier::D0),
        _ => None,
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Deposit {
    pub id: i64,
    pub person: String,
    pub bank: String,
    pub currency: String,
    pub amount: Decimal,
    pub rate: Decimal,
    pub start_date: NaiveDate,
    pub term_months: i32,
    pub maturity_date: NaiveDate,
    pub remind_7d: bool,
    pub remind_1d: bool,
    pub remind_today: bool,
    pub notified_7d: Option<NaiveDate>,
    pub notified_1d: Option<NaiveDate>,
    pub notified_today: Option<NaiveDate>,
    pub note: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewDeposit {
    pub person: String,
    pub bank: String,
    #[serde(default = "d_cny")]
    pub currency: String,
    pub amount: Decimal,
    pub rate: Decimal,
    pub start_date: NaiveDate,
    pub term_months: i32,
    #[serde(default = "t")]
    pub remind_7d: bool,
    #[serde(default = "t")]
    pub remind_1d: bool,
    #[serde(default = "t")]
    pub remind_today: bool,
    pub note: Option<String>,
}

fn d_cny() -> String { "CNY".into() }
fn t() -> bool { true }

pub async fn create(pool: &PgPool, b: &NewDeposit) -> Result<i64, sqlx::Error> {
    let maturity = compute_maturity(b.start_date, b.term_months);
    sqlx::query_scalar(
        "INSERT INTO deposits (person, bank, currency, amount, rate, start_date, term_months, maturity_date, remind_7d, remind_1d, remind_today, note)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING id")
        .bind(&b.person).bind(&b.bank).bind(&b.currency).bind(b.amount).bind(b.rate)
        .bind(b.start_date).bind(b.term_months).bind(maturity)
        .bind(b.remind_7d).bind(b.remind_1d).bind(b.remind_today).bind(&b.note)
        .fetch_one(pool).await
}

pub async fn list(pool: &PgPool) -> Result<Vec<Deposit>, sqlx::Error> {
    sqlx::query_as::<_, Deposit>("SELECT * FROM deposits ORDER BY maturity_date").fetch_all(pool).await
}

pub async fn update(pool: &PgPool, id: i64, b: &NewDeposit) -> Result<bool, sqlx::Error> {
    let maturity = compute_maturity(b.start_date, b.term_months);
    let r = sqlx::query(
        "UPDATE deposits SET person=$2, bank=$3, currency=$4, amount=$5, rate=$6, start_date=$7,
                term_months=$8, maturity_date=$9, remind_7d=$10, remind_1d=$11, remind_today=$12,
                note=$13, updated_at=now()
         WHERE id=$1")
        .bind(id).bind(&b.person).bind(&b.bank).bind(&b.currency).bind(b.amount).bind(b.rate)
        .bind(b.start_date).bind(b.term_months).bind(maturity)
        .bind(b.remind_7d).bind(b.remind_1d).bind(b.remind_today).bind(&b.note)
        .execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM deposits WHERE id=$1").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CurrencySummary {
    pub currency: String,
    pub count: i64,
    pub total: Decimal,
    pub total_maturity: Decimal,
}

pub async fn summary(pool: &PgPool) -> Result<Vec<CurrencySummary>, sqlx::Error> {
    sqlx::query_as::<_, CurrencySummary>(
        "SELECT currency, count(*) AS count, sum(amount) AS total,
                sum(amount + maturity_interest(amount, rate, term_months)) AS total_maturity
         FROM deposits GROUP BY currency")
    .fetch_all(pool).await
}

pub async fn due_in_30d(pool: &PgPool) -> Result<Vec<Deposit>, sqlx::Error> {
    sqlx::query_as::<_, Deposit>(
        "SELECT * FROM deposits
         WHERE maturity_date <= (current_date + 30) AND maturity_date >= current_date
         ORDER BY maturity_date")
    .fetch_all(pool).await
}
```

注意 SQL：PG 无 `maturity_interest` 函数——`summary` 中直接内联表达式：

```sql
sum(amount + amount * rate / 100 * term_months / 12) AS total_maturity
```

- [ ] **Step 4: 实现 deposits/api.rs**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::error::{ApiResult, AppError};
use crate::AppState;
use super::{self as dep, NewDeposit};

fn validate(b: &NewDeposit) -> Result<(), AppError> {
    if b.amount <= rust_decimal::Decimal::ZERO || b.term_months <= 0 {
        return Err(AppError(StatusCode::BAD_REQUEST, "金额与期限必须为正".into()));
    }
    if b.currency.len() != 3 || !b.currency.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError(StatusCode::BAD_REQUEST, "币种须为 3 位大写字母".into()));
    }
    Ok(())
}

async fn list(State(st): State<AppState>) -> ApiResult<Json<Vec<dep::Deposit>>> {
    Ok(Json(dep::list(&st.pool).await?))
}

async fn create(State(st): State<AppState>, Json(b): Json<NewDeposit>) -> ApiResult<Json<serde_json::Value>> {
    validate(&b)?;
    let id = dep::create(&st.pool, &b).await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn update(State(st): State<AppState>, Path(id): Path<i64>, Json(b): Json<NewDeposit>) -> ApiResult<Json<serde_json::Value>> {
    validate(&b)?;
    if !dep::update(&st.pool, id, &b).await? {
        return Err(AppError(StatusCode::NOT_FOUND, format!("存款 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn delete(State(st): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<serde_json::Value>> {
    if !dep::delete(&st.pool, id).await? {
        return Err(AppError(StatusCode::NOT_FOUND, format!("存款 {id} 不存在")));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn summary(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let s = dep::summary(&st.pool).await?;
    let due = dep::due_in_30d(&st.pool).await?;
    Ok(Json(serde_json::json!({ "currencies": s, "due_30d": due })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/deposits", get(list).post(create))
        .route("/api/deposits/:id", axum::routing::put(update).delete(delete))
        .route("/api/deposits/summary", get(summary))
}
```

注意：`/api/deposits/summary` 必须注册在 `/api/deposits/:id` 之前或用更具体路径（axum 0.8 静态段优先于参数，顺序无碍）。`main.rs` 合并 `.merge(fintrack::deposits::api::routes())`。

- [ ] **Step 5: 运行 + Commit**

```bash
cargo test   # deposits 4 个单测 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test -- --ignored
git add -A && git commit -m "feat(deposits): 存款模块（利息/到期纯函数 + CRUD + 按币种汇总）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 12: 到期扫描任务（09:05 每日）

**Files:**
- Create: `backend/src/maturity.rs`、`backend/tests/maturity_int.rs`

**Interfaces:**
- Consumes: `deposits::{should_notify, Tier, DepositLike}`、表 `deposits`
- Produces:
  - `maturity::next_0905(now: DateTime<Utc>) -> DateTime<Utc>`（下一个北京时间 09:05，已过则明天）
  - `maturity::scan_and_mark(pool: &PgPool, today: NaiveDate) -> Result<Vec<DueNotice>, sqlx::Error>`
  - `maturity::DueNotice { deposit_id: i64, tier: Tier, person, bank, currency: String, amount: Decimal, maturity_date: NaiveDate }`
  - `maturity::notice_text(n: &DueNotice) -> String`

- [ ] **Step 1: 写失败测试**

`backend/src/maturity.rs` `#[cfg(test)]`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_0905_before_and_after() {
        // 北京时间 h:m == UTC h-8:m（同日）
        let t = |h, m| chrono::Utc.with_ymd_and_hms(2026, 11, 10, h - 8, m, 0).unwrap();
        // 北京 08:00 → 今天 09:05（= UTC 01:05）
        let n = next_0905(t(8, 0));
        assert_eq!(n, chrono::Utc.with_ymd_and_hms(2026, 11, 10, 1, 5, 0).unwrap());
        // 北京 10:00 → 明天 09:05
        let n = next_0905(t(10, 0));
        assert_eq!(n, chrono::Utc.with_ymd_and_hms(2026, 11, 11, 1, 5, 0).unwrap());
        // 恰好 09:05 → 明天
        let n = next_0905(t(9, 5));
        assert_eq!(n, chrono::Utc.with_ymd_and_hms(2026, 11, 11, 1, 5, 0).unwrap());
    }

    #[test]
    fn notice_text_contains_key_fields() {
        let n = DueNotice {
            deposit_id: 1, tier: crate::deposits::Tier::D7,
            person: "张三".into(), bank: "中行".into(), currency: "CNY".into(),
            amount: rust_decimal::Decimal::new(1000000, 2),
            maturity_date: chrono::NaiveDate::from_ymd_opt(2026, 11, 17).unwrap(),
        };
        let s = notice_text(&n);
        assert!(s.contains("张三") && s.contains("中行") && s.contains("2026-11-17") && s.contains("7"));
    }
}
```

`backend/tests/maturity_int.rs`：

```rust
#[tokio::test]
#[ignore]
async fn scan_marks_and_dedupes_same_day() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let pool = fintrack::db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM deposits").execute(&pool).await.unwrap();

    let today = chrono::Utc::now().date_naive();
    let mat = today + chrono::Duration::days(7);
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO deposits (person,bank,currency,amount,rate,start_date,term_months,maturity_date)
         VALUES ('张三','中行','CNY',10000,2.5,$1,12,$2) RETURNING id")
        .bind(today - chrono::Duration::days(30)).bind(mat)
        .fetch_one(&pool).await.unwrap();

    let n1 = fintrack::maturity::scan_and_mark(&pool, today).await.unwrap();
    assert_eq!(n1.len(), 1);
    let n2 = fintrack::maturity::scan_and_mark(&pool, today).await.unwrap();
    assert!(n2.is_empty(), "同日不重复推送");
    let notified: chrono::NaiveDate = sqlx::query_scalar("SELECT notified_7d FROM deposits WHERE id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(notified, today);
}
```

- [ ] **Step 2: 确认失败 → 实现 maturity.rs**

```rust
use crate::deposits::{should_notify, DepositLike, Tier};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Asia::Shanghai;
use rust_decimal::Decimal;
use sqlx::PgPool;

#[derive(Debug)]
pub struct DueNotice {
    pub deposit_id: i64,
    pub tier: Tier,
    pub person: String,
    pub bank: String,
    pub currency: String,
    pub amount: Decimal,
    pub maturity_date: NaiveDate,
}

pub fn notice_text(n: &DueNotice) -> String {
    let days = match n.tier {
        Tier::D7 => "7 天",
        Tier::D1 => "1 天",
        Tier::D0 => "今天",
    };
    format!("存款到期提醒（还有{days}）：{} · {} · {} · 金额 {} · 到期日 {}",
        n.person, n.bank, n.currency, n.amount, n.maturity_date)
}

pub fn next_0905(now: DateTime<Utc>) -> DateTime<Utc> {
    let sh = now.with_timezone(&Shanghai);
    let today = sh.date_naive();
    // 今日 09:05（北京时间）对应的 NaiveDateTime
    let target = today.and_hms_opt(9, 5, 0).unwrap();
    let next_naive = if sh.naive_local() < target {
        target
    } else {
        (today + chrono::Duration::days(1)).and_hms_opt(9, 5, 0).unwrap()
    };
    Shanghai.from_local_datetime(&next_naive).single().unwrap().with_timezone(&Utc)
}

pub async fn scan_and_mark(pool: &PgPool, today: NaiveDate) -> Result<Vec<DueNotice>, sqlx::Error> {
    let rows = sqlx::query_as::<_, crate::deposits::Deposit>(
        "SELECT * FROM deposits WHERE maturity_date BETWEEN $1 AND $1::date + 7 ORDER BY maturity_date")
        .bind(today)
        .fetch_all(pool).await?;

    let mut out = vec![];
    for d in rows {
        let like = DepositLike {
            remind_7d: d.remind_7d,
            remind_1d: d.remind_1d,
            remind_today: d.remind_today,
            notified_7d: d.notified_7d,
            notified_1d: d.notified_1d,
            notified_today: d.notified_today,
            maturity_date: d.maturity_date,
        };
        if let Some(tier) = should_notify(&like, today) {
            let col = match tier {
                Tier::D7 => "notified_7d",
                Tier::D1 => "notified_1d",
                Tier::D0 => "notified_today",
            };
            sqlx::query(&format!("UPDATE deposits SET {col} = $1 WHERE id = $2"))
                .bind(today).bind(d.id)
                .execute(pool).await?;
            out.push(DueNotice {
                deposit_id: d.id, tier,
                person: d.person, bank: d.bank, currency: d.currency,
                amount: d.amount, maturity_date: d.maturity_date,
            });
        }
    }
    Ok(out)
}
```

`lib.rs` 加 `pub mod maturity;`。

- [ ] **Step 3: 运行 + Commit**

```bash
cargo test   # maturity 2 个单测 PASS
TEST_DATABASE_URL=postgres://fx:fx@localhost:5432/fx cargo test --test maturity_int -- --ignored
git add -A && git commit -m "feat(maturity): 到期扫描（7天/1天/当天档位 + 防重推 + 09:05 计算）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 13: 调度器与后端总装

**Files:**
- Create: `backend/src/scheduler.rs`
- Modify: `backend/src/lib.rs`（AppState 扩展）、`backend/src/main.rs`（最终组装）

**Interfaces:**
- Consumes: 前面所有模块
- Produces:
  - `AppState` 扩展为：`{ pool, cfg: Arc<AppConfig>, auth: AuthState, health: Arc<HealthRegistry>, notifiers: Arc<Vec<Arc<dyn Notifier>>> }`
  - `GET /api/sources/status` → `Json<Vec<SourceSnapshot>>`（挂 protected）
  - `scheduler::spawn_all(state: AppState)`：spawn ① 银行抓取循环（`cfg.scrape_interval_secs`）② 基准 API 增量循环（每小时）③ 每日 09:05 任务（到期扫描 + ensure_partitions + drop_old_partitions(24)）
  - 采集→入库→告警评估→推送 的完整链路；健康事件推送"数据源失效/恢复"
  - 启动时立即执行一次：基准回填 + 两个银行抓取（快速见效）

- [ ] **Step 1: 实现 scheduler.rs**

```rust
use crate::alerts::engine;
use crate::notify::{self, Notifier};
use crate::rates::store;
use crate::sources::health::{HealthEvent, HealthRegistry};
use crate::{sources, AppState};
use chrono::Utc;
use std::sync::Arc;

async fn ingest_point_eval_alerts(state: &AppState, pts: &[crate::rates::RatePoint]) {
    if let Err(e) = store::insert_many(&state.pool, pts).await {
        tracing::error!("入库失败: {e}");
        return;
    }
    for pt in pts {
        match engine::evaluate_all(&state.pool, pt, Utc::now()).await {
            Ok(triggered) => {
                for t in triggered {
                    notify::dispatch(&state.pool, &state.notifiers, "汇率告警", &t.message()).await;
                }
            }
            Err(e) => tracing::error!("告警评估失败: {e}"),
        }
    }
}

async fn notify_health(state: &AppState, label: &str, ev: HealthEvent) {
    let body = match ev {
        HealthEvent::WentDown => format!("数据源【{label}】失效（连续 3 次抓取失败）"),
        HealthEvent::Recovered => format!("数据源【{label}】已恢复"),
    };
    notify::dispatch(&state.pool, &state.notifiers, "数据源状态", &body).await;
}

async fn run_bank_scraper<F, Fut>(
    state: &AppState,
    name: &'static str,
    label: &str,
    fetch: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<crate::rates::RatePoint>, String>>,
{
    match fetch().await {
        Ok(pts) => {
            if let Some(ev) = state.health.report(name, true) {
                notify_health(state, label, ev).await;
            }
            ingest_point_eval_alerts(state, &pts).await;
        }
        Err(e) => {
            tracing::warn!("{label} 抓取失败: {e}");
            if let Some(ev) = state.health.report(name, false) {
                notify_health(state, label, ev).await;
            }
        }
    }
}

async fn run_franks(state: &AppState) {
    let client = reqwest::Client::new();
    match sources::frankfurter::ingest_api_rates(&state.pool, &client, &state.cfg.currency_pairs).await {
        Ok(n) => {
            tracing::info!("基准汇率增量 {n} 条");
            state.health.report("api", true);
            // 基准价（source=api）上的规则评估：取每对最新一条评估
            for pair in &state.cfg.currency_pairs {
                if let Ok(Some(pt)) = store::latest_one(&state.pool, "api", pair).await {
                    if let Ok(triggered) = engine::evaluate_all(&state.pool, &pt, Utc::now()).await {
                        for t in triggered {
                            notify::dispatch(&state.pool, &state.notifiers, "汇率告警", &t.message()).await;
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("基准汇率拉取失败: {e}");
            state.health.report("api", false);
        }
    }
}

async fn run_daily(state: &AppState) {
    let today = Utc::now().date_naive();
    match crate::maturity::scan_and_mark(&state.pool, today).await {
        Ok(notices) => {
            for n in notices {
                notify::dispatch(&state.pool, &state.notifiers, "定期存款", &crate::maturity::notice_text(&n)).await;
            }
        }
        Err(e) => tracing::error!("到期扫描失败: {e}"),
    }
    if let Err(e) = crate::db::ensure_partitions(&state.pool).await {
        tracing::error!("分区维护失败: {e}");
    }
    if let Err(e) = crate::db::drop_old_partitions(&state.pool, 24).await {
        tracing::error!("分区清理失败: {e}");
    }
}

pub fn spawn_all(state: AppState) {
    // ① 银行抓取循环
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                let s = state.clone();
                run_bank_scraper(&s, "boc", "中行", || async {
                    let html = sources::boc::fetch(&reqwest::Client::new()).await?;
                    sources::boc::parse(&html, Utc::now())
                }).await;
                let s = state.clone();
                run_bank_scraper(&s, "icbc", "工行", || async {
                    let html = sources::icbc::fetch(&reqwest::Client::new()).await?;
                    sources::icbc::parse(&html, Utc::now())
                }).await;
                tokio::time::sleep(std::time::Duration::from_secs(state.cfg.scrape_interval_secs)).await;
            }
        });
    }
    // ② 基准 API 每小时增量 + 启动立即执行
    {
        let state = state.clone();
        tokio::spawn(async move {
            run_franks(&state).await;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                run_franks(&state).await;
            }
        });
    }
    // ③ 每日 09:05（北京时间）
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                let next = crate::maturity::next_0905(Utc::now());
                tokio::time::sleep_until(tokio::time::Instant::from_std(next.into())).await;
                run_daily(&state).await;
            }
        });
    }
}
```

需要补充 `store::latest_one`（`rates/mod.rs` 的 store 内）：

```rust
pub async fn latest_one(pool: &PgPool, source: &str, pair: &str) -> Result<Option<RatePoint>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct R {
        source: String, pair: String,
        fx_bid: Option<Decimal>, fx_ask: Option<Decimal>, mid: Option<Decimal>,
        recorded_at: chrono::DateTime<chrono::Utc>, note: Option<String>,
    }
    let r = sqlx::query_as::<_, R>(
        "SELECT source, pair, fx_bid, fx_ask, mid, recorded_at, note
         FROM exchange_rates WHERE source=$1 AND pair=$2
         ORDER BY recorded_at DESC LIMIT 1")
        .bind(source).bind(pair)
        .fetch_optional(pool).await?;
    Ok(r.map(|r| RatePoint {
        source: r.source, pair: r.pair, fx_bid: r.fx_bid, fx_ask: r.fx_ask,
        mid: r.mid, recorded_at: r.recorded_at, note: r.note,
    }))
}
```

（`run_bank_scraper` 的签名即上面代码中的 `F: FnOnce() -> Fut` 泛型闭包形式，调用处传 `|| async { ... }`。）

- [ ] **Step 2: main.rs 最终组装**

```rust
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

use fintrack::notify::{telegram::TelegramNotifier, wecom::WecomNotifier, Notifier};
use fintrack::sources::health::HealthRegistry;
use fintrack::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let cfg = Arc::new(fintrack::config::AppConfig::load()?);
    let pool = fintrack::db::connect(&cfg.database_url).await?;
    sqlx::migrate!().run(&pool).await?;
    fintrack::db::ensure_partitions(&pool).await?;

    let auth_state = fintrack::auth::AuthState {
        password_hash: Arc::new(cfg.admin_password_hash.clone()),
    };

    // 通知渠道：配置了哪个就启用哪个
    let mut notifiers: Vec<Arc<dyn Notifier>> = vec![];
    if let Some(url) = &cfg.wecom_webhook_url {
        notifiers.push(Arc::new(WecomNotifier { webhook_url: url.clone(), client: reqwest::Client::new() }));
    }
    if let (Some(token), Some(chat)) = (&cfg.telegram_bot_token, &cfg.telegram_chat_id) {
        notifiers.push(Arc::new(TelegramNotifier {
            bot_token: token.clone(), chat_id: chat.clone(),
            api_base: "https://api.telegram.org".into(),
            client: fintrack::notify::telegram::build_client(cfg.telegram_proxy.as_deref()),
        }));
    }
    if notifiers.is_empty() {
        tracing::warn!("未配置任何通知渠道（WECOM/TELEGRAM），告警与提醒将只入库不推送");
    }

    let state = AppState {
        pool: pool.clone(),
        cfg: cfg.clone(),
        auth: auth_state.clone(),
        health: Arc::new(HealthRegistry::new()),
        notifiers: Arc::new(notifiers),
    };

    let public = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/auth/login", post(fintrack::auth::login_handler));

    let protected = Router::new()
        .route("/api/auth/logout", post(fintrack::auth::logout_handler))
        .route("/api/auth/me", get(fintrack::auth::me_handler))
        .merge(fintrack::rates::api::routes())
        .merge(fintrack::alerts::api::routes())
        .merge(fintrack::notify::api::routes())
        .merge(fintrack::deposits::api::routes())
        .route("/api/sources/status", get(sources_status))
        .route_layer(axum::middleware::from_fn_with_state(auth_state, fintrack::auth::auth_mw));

    let app = public.merge(protected).with_state(state.clone());

    fintrack::scheduler::spawn_all(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn sources_status(
    axum::extract::State(st): axum::extract::State<fintrack::AppState>,
) -> axum::Json<Vec<fintrack::sources::health::SourceSnapshot>> {
    axum::Json(st.health.snapshots())
}
```

`lib.rs` 的 AppState 扩展为：

```rust
#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub cfg: Arc<AppConfig>,
    pub auth: crate::auth::AuthState,
    pub health: Arc<crate::sources::health::HealthRegistry>,
    pub notifiers: Arc<Vec<Arc<dyn notify::Notifier>>>,
}
```

注意：AppState 增加了 `Clone` 依赖的 `Notifier` trait object——`Vec<Arc<dyn Notifier>>` 可 Clone（Arc clone）✓。**此前任务中 AppState 无 health/notifiers 字段的构造处需同步补齐**（main.rs 是唯一构造点）。

- [ ] **Step 3: 端到端验证**

```bash
cargo test   # 全部单测 PASS
cargo build --release
# 手动端到端（两个终端）：
DATABASE_URL=postgres://fx:fx@localhost:5432/fx ADMIN_PASSWORD=$(cargo run --bin pwd_hash -- 密码暂不适用，此处直接填哈希) ./target/release/fintrack
# 观察日志：基准回填 N 条、boc/icbc 抓取结果、health 状态
curl -s localhost:8080/healthz    # ok
curl -s localhost:8080/api/rates/latest -H "Cookie: fx_session=bad"   # 401
```

（实际操作：`HASH=$(cargo run --bin pwd_hash -- test123 | tail -1)` 然后 `ADMIN_PASSWORD=$HASH ...`。）

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(scheduler): 采集调度 + 告警/提醒链路总装 + /api/sources/status

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 14: 前端脚手架（路由 + API client + 登录页）

**Files:**
- Create: `frontend/src/api/client.ts`、`frontend/src/utils/format.ts`、`frontend/src/stores/auth.ts`、`frontend/src/router/index.ts`、`frontend/src/views/Login.vue`（其余 5 个视图先建占位）、`frontend/src/utils/format.test.ts`
- Modify: `frontend/src/main.ts`、`frontend/src/App.vue`、`frontend/package.json`、`frontend/vite.config.ts`（加 vitest 配置）

**Interfaces:**
- Consumes: 后端 `/api/auth/*`（Task 3/4）
- Produces:
  - `api<T>(path, opts?)`：fetch 封装；401 → `location.href = '/login'`；非 2xx 抛 `Error(error字段)`
  - `apiGet<T>/apiPost<T>/apiPut<T>/apiDel<T>` 快捷方法
  - `formatMoney(v: string | number, mask?: boolean): string`（mask=true 返回 `'****'`；否则 `Intl.NumberFormat('zh-CN', {maximumFractionDigits: 2})`）
  - pinia `useAuthStore { ready: boolean; ensure(): Promise<void> }`
  - 路由：`/login` 公开；`/`(Overview)、`/charts`、`/rules`、`/notifications`、`/deposits` 受 `beforeEach` 保护（`ensure()` 失败跳 `/login`）
  - App.vue：登录页无侧边栏，其余页有 el-menu 导航

- [ ] **Step 1: 安装依赖**

```bash
cd frontend
npm i vue-router@4 pinia element-plus echarts vue-echarts
npm i -D vitest
```

- [ ] **Step 2: 写失败测试（format util）**

`frontend/src/utils/format.test.ts`：

```ts
import { describe, expect, it } from 'vitest'
import { formatMoney } from './format'

describe('formatMoney', () => {
  it('formats numbers with thousands separators', () => {
    expect(formatMoney('10000')).toBe('10,000')
  })
  it('masks when requested', () => {
    expect(formatMoney(10000, true)).toBe('****')
  })
})
```

```bash
npx vitest run   # 期望：FAIL（format.ts 不存在）
```

- [ ] **Step 3: 实现基础文件**

`frontend/src/utils/format.ts`：

```ts
export function formatMoney(v: string | number, mask = false): string {
  if (mask) return '****'
  return new Intl.NumberFormat('zh-CN', { maximumFractionDigits: 2 }).format(Number(v))
}
```

`frontend/src/api/client.ts`：

```ts
const BASE = '/api'

export async function api<T>(path: string, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(BASE + path, {
    headers: { 'Content-Type': 'application/json', ...(opts.headers ?? {}) },
    ...opts,
  })
  if (res.status === 401) {
    location.href = '/login'
    throw new Error('未登录')
  }
  if (!res.ok) {
    const body = await res.json().catch(() => ({}) as { error?: string })
    throw new Error(body.error ?? res.statusText)
  }
  return res.json() as Promise<T>
}

export const apiGet = <T>(p: string) => api<T>(p)
export const apiPost = <T>(p: string, body: unknown) =>
  api<T>(p, { method: 'POST', body: JSON.stringify(body) })
export const apiPut = <T>(p: string, body: unknown) =>
  api<T>(p, { method: 'PUT', body: JSON.stringify(body) })
export const apiDel = <T>(p: string) => api<T>(p, { method: 'DELETE' })
```

`frontend/src/stores/auth.ts`：

```ts
import { defineStore } from 'pinia'
import { api } from '../api/client'

export const useAuthStore = defineStore('auth', {
  state: () => ({ ready: false }),
  actions: {
    async ensure() {
      if (this.ready) return
      try {
        await api('/auth/me')
        this.ready = true
      } catch {
        /* 401 已由 client 跳转 /login */
      }
    },
    logout() {
      this.ready = false
    },
  },
})
```

`frontend/src/router/index.ts`：

```ts
import { createRouter, createWebHistory } from 'vue-router'

export const routes = [
  { path: '/login', component: () => import('../views/Login.vue') },
  { path: '/', component: () => import('../views/Overview.vue') },
  { path: '/charts', component: () => import('../views/Charts.vue') },
  { path: '/rules', component: () => import('../views/Rules.vue') },
  { path: '/notifications', component: () => import('../views/Notifications.vue') },
  { path: '/deposits', component: () => import('../views/Deposits.vue') },
]

const router = createRouter({ history: createWebHistory(), routes })

router.beforeEach(async (to) => {
  if (to.path === '/login') return true
  const { useAuthStore } = await import('../stores/auth')
  const auth = useAuthStore()
  await auth.ensure()
  return auth.ready || '/login'
})

export default router
```

`frontend/src/views/Login.vue`：

```vue
<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { apiPost } from '../api/client'
import { useAuthStore } from '../stores/auth'

const password = ref('')
const loading = ref(false)
const router = useRouter()
const auth = useAuthStore()

async function submit() {
  loading.value = true
  try {
    await apiPost('/auth/login', { password: password.value })
    auth.ready = true
    router.push('/')
  } catch (e) {
    ElMessage.error((e as Error).message)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div style="display: flex; justify-content: center; align-items: center; height: 100vh">
    <el-card style="width: 360px">
      <h2>汇率与定存管理</h2>
      <el-form @submit.prevent="submit">
        <el-form-item>
          <el-input v-model="password" type="password" placeholder="管理员密码" show-password @keyup.enter="submit" />
        </el-form-item>
        <el-button type="primary" :loading="loading" style="width: 100%" @click="submit">登录</el-button>
      </el-form>
    </el-card>
  </div>
</template>
```

其余 5 个视图先建占位（Task 15/16 实现）——`Overview.vue` 等文件内容：

```vue
<template><div>Overview（Task 15 实现）</div></template>
```

（Rules/Notifications/Deposits/Charts 同样一行占位。）

`frontend/src/App.vue`：

```vue
<script setup lang="ts">
import { useRoute } from 'vue-router'
const route = useRoute()
const menu = [
  { path: '/', label: '汇率总览' },
  { path: '/charts', label: '汇率图表' },
  { path: '/rules', label: '告警规则' },
  { path: '/notifications', label: '通知历史' },
  { path: '/deposits', label: '存款管理' },
]
</script>

<template>
  <el-container v-if="route.path !== '/login'" style="height: 100vh">
    <el-aside width="200px">
      <el-menu router :default-active="route.path">
        <el-menu-item v-for="m in menu" :key="m.path" :index="m.path">{{ m.label }}</el-menu-item>
      </el-menu>
    </el-aside>
    <el-main><router-view /></el-main>
  </el-container>
  <router-view v-else />
</template>
```

`frontend/src/main.ts`：

```ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import App from './App.vue'
import router from './router'

createApp(App).use(createPinia()).use(router).use(ElementPlus).mount('#app')
```

`frontend/vite.config.ts` 追加：

```ts
/// <reference types="vitest/config" />
export default defineConfig({
  plugins: [vue()],
  server: { proxy: { '/api': 'http://localhost:8080' } },
  test: { environment: 'node' },
})
```

`package.json` scripts 加：`"test": "vitest run"`。

- [ ] **Step 4: 运行验证**

```bash
npx vitest run            # formatMoney 2 个用例 PASS
npm run build             # 零错误
# 手动联调（后端先起）：
npm run dev               # 浏览器打开 http://localhost:5173 → 跳 /login → 输密码 → 进入总览占位页
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(frontend): 脚手架（路由守卫 + API client + 登录页）

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 15: 汇率总览页与图表页

**Files:**
- Create: `frontend/src/components/TrendChart.vue`、`frontend/src/components/SourceLight.vue`、`frontend/src/components/ManualEntryDialog.vue`
- Modify: `frontend/src/views/Overview.vue`、`frontend/src/views/Charts.vue`、`frontend/src/main.ts`（echarts 注册）

**Interfaces:**
- Consumes: `GET /api/rates/latest`、`GET /api/rates/history`、`GET /api/sources/status`、`POST /api/rates/manual`（Decimal 均为 JSON 字符串）
- Produces:
  - `TrendChart.vue`：props `{ pair: string; sources: string[]; from: string; to: string; field: 'mid'|'fx_bid'|'fx_ask'; height?: string }`，内部拉 history 并渲染多系列折线（vue-echarts）
  - `SourceLight.vue`：props `{ snapshots: {name:string; is_down:boolean; consecutive_fails:number; last_ok_at:string|null}[] }`
  - `ManualEntryDialog.vue`：v-model:visible + 提交后 emit('saved')

- [ ] **Step 1: echarts 注册（main.ts 追加）**

```ts
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { LineChart } from 'echarts/charts'
import { GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
use([CanvasRenderer, LineChart, GridComponent, LegendComponent, TooltipComponent])
```

- [ ] **Step 2: TrendChart.vue**

```vue
<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import VChart from 'vue-echarts'
import { apiGet } from '../api/client'

const props = withDefaults(defineProps<{
  pair: string; sources: string[]; from: string; to: string
  field?: 'mid' | 'fx_bid' | 'fx_ask'; height?: string
}>(), { field: 'mid', height: '360px' })

interface Bucket { source: string; t: string; mid: string | null; fx_bid: string | null; fx_ask: string | null }

const series = ref<Bucket[]>([])
const titleMap: Record<string, string> = { mid: '中间价', fx_bid: '现汇买入', fx_ask: '现汇卖出' }

async function load() {
  const q = `pair=${props.pair}&sources=${props.sources.join(',')}&from=${props.from}&to=${props.to}&points=300`
  const r = await apiGet<{ points: Bucket[] }>(`/rates/history?${q}`)
  series.value = r.points
}

watch(() => [props.pair, props.sources.join(','), props.from, props.to, props.field], load, { immediate: true })

const option = computed(() => {
  const bySource = new Map<string, [string, number][]>()
  for (const p of series.value) {
    const v = p[props.field]
    if (v != null) {
      const arr = bySource.get(p.source) ?? []
      arr.push([p.t, Number(v)])
      bySource.set(p.source, arr)
    }
  }
  const nameMap: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行', manual: '人工' }
  return {
    tooltip: { trigger: 'axis' },
    legend: {},
    xAxis: { type: 'time' },
    yAxis: { type: 'value', scale: true },
    series: [...bySource.entries()].map(([src, data]) => ({
      name: nameMap[src] ?? src, type: 'line', showSymbol: false, data,
    })),
  }
})
</script>

<template>
  <div>
    <VChart :option="option" :style="{ height, width: '100%' }" autoresize />
  </div>
</template>
```

- [ ] **Step 3: SourceLight.vue + Overview.vue**

`SourceLight.vue`：

```vue
<script setup lang="ts">
defineProps<{ snapshots: { name: string; is_down: boolean; consecutive_fails: number; last_ok_at: string | null }[] }>()
const label: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行' }
</script>

<template>
  <span v-for="s in snapshots" :key="s.name" style="margin-right: 12px; font-size: 13px">
    <el-tag :type="s.is_down ? 'danger' : 'success'" size="small" effect="dark">
      {{ label[s.name] ?? s.name }} {{ s.is_down ? '失效' : '正常' }}
    </el-tag>
  </span>
</template>
```

`Overview.vue`（替换占位）：

```vue
<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { apiGet } from '../api/client'
import TrendChart from '../components/TrendChart.vue'
import SourceLight from '../components/SourceLight.vue'

interface Latest { pair: string; source: string; fx_bid: string | null; fx_ask: string | null; mid: string | null; recorded_at: string }
interface Snapshot { name: string; is_down: boolean; consecutive_fails: number; last_ok_at: string | null }

const latest = ref<Latest[]>([])
const snapshots = ref<Snapshot[]>([])
const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const srcName: Record<string, string> = { api: '基准', boc: '中行', icbc: '工行', manual: '人工' }

async function load() {
  latest.value = await apiGet<Latest[]>('/rates/latest')
  snapshots.value = await apiGet<Snapshot[]>('/sources/status')
}

let timer: number
onMounted(() => {
  load()
  timer = window.setInterval(load, 60_000)
})
onUnmounted(() => clearInterval(timer))

function rowsOf(pair: string) {
  return latest.value.filter((r) => r.pair === pair)
}
function iso(offsetHours: number) {
  return new Date(Date.now() - offsetHours * 3600_000).toISOString()
}
</script>

<template>
  <div>
    <div style="margin-bottom: 12px"><SourceLight :snapshots="snapshots" /></div>
    <el-table :data="latest" size="small" border>
      <el-table-column prop="pair" label="货币对" width="110" />
      <el-table-column label="来源" width="80">
        <template #default="{ row }">{{ srcName[row.source] ?? row.source }}</template>
      </el-table-column>
      <el-table-column label="现汇买入">
        <template #default="{ row }">{{ row.fx_bid ? Number(row.fx_bid).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column label="现汇卖出">
        <template #default="{ row }">{{ row.fx_ask ? Number(row.fx_ask).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column label="中间价">
        <template #default="{ row }">{{ row.mid ? Number(row.mid).toFixed(4) : '-' }}</template>
      </el-table-column>
      <el-table-column prop="recorded_at" label="时间" width="180">
        <template #default="{ row }">{{ new Date(row.recorded_at).toLocaleString() }}</template>
      </el-table-column>
    </el-table>

    <h3>24 小时基准走势</h3>
    <el-row :gutter="12">
      <el-col v-for="p in pairs" :key="p" :span="8" style="margin-bottom: 12px">
        <el-card>
          <template #header>{{ p }}</template>
          <TrendChart :pair="p" :sources="['api']" :from="iso(24)" :to="new Date().toISOString()"
            field="mid" height="140px" />
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>
```

- [ ] **Step 4: ManualEntryDialog.vue + Charts.vue**

`ManualEntryDialog.vue`：

```vue
<script setup lang="ts">
import { ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { apiPost } from '../api/client'

const props = defineProps<{ visible: boolean; pairs: string[] }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void; (e: 'saved'): void }>()

const form = ref({ pair: 'USD/CNY', fx_bid: null as number | null, fx_ask: null as number | null, mid: null as number | null, note: '' })

watch(() => props.visible, (v) => { if (v) form.value = { pair: props.pairs[0], fx_bid: null, fx_ask: null, mid: null, note: '' } })

async function submit() {
  const f = form.value
  if (f.fx_bid == null && f.fx_ask == null && f.mid == null) {
    ElMessage.warning('至少填一个价格')
    return
  }
  try {
    await apiPost('/rates/manual', f)
    ElMessage.success('已录入')
    emit('update:visible', false)
    emit('saved')
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
</script>

<template>
  <el-dialog :model-value="visible" title="人工录入汇率" @update:model-value="emit('update:visible', $event)">
    <el-form label-width="90px">
      <el-form-item label="货币对">
        <el-select v-model="form.pair">
          <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
        </el-select>
      </el-form-item>
      <el-form-item label="现汇买入价"><el-input-number v-model="form.fx_bid" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="现汇卖出价"><el-input-number v-model="form.fx_ask" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="中间价"><el-input-number v-model="form.mid" :precision="4" :step="0.01" /></el-form-item>
      <el-form-item label="备注"><el-input v-model="form.note" /></el-form-item>
    </el-form>
    <template #footer>
      <el-button @click="emit('update:visible', false)">取消</el-button>
      <el-button type="primary" @click="submit">保存</el-button>
    </template>
  </el-dialog>
</template>
```

`Charts.vue`（替换占位）：

```vue
<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import TrendChart from '../components/TrendChart.vue'
import ManualEntryDialog from '../components/ManualEntryDialog.vue'

const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const sources = ['api', 'boc', 'icbc', 'manual']
const srcName: Record<string, string> = { api: '市场基准', boc: '中行', icbc: '工行', manual: '人工' }

const pair = ref('USD/CNY')
const picked = ref<string[]>(['api', 'boc'])
const range = ref<'24h' | '7d' | '30d' | '1y'>('7d')
const field = ref<'mid' | 'fx_bid' | 'fx_ask'>('mid')
const dialogVisible = ref(false)
const refreshKey = ref(0)

const fromTo = computed(() => {
  const hours = { '24h': 24, '7d': 168, '30d': 720, '1y': 8760 }[range.value]
  return { from: new Date(Date.now() - hours * 3600_000).toISOString(), to: new Date().toISOString() }
})

const timer = window.setInterval(() => (refreshKey.value++), 60_000)
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <div>
    <div style="display: flex; gap: 12px; margin-bottom: 12px; flex-wrap: wrap">
      <el-select v-model="pair" style="width: 130px">
        <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
      </el-select>
      <el-select v-model="picked" multiple style="width: 280px">
        <el-option v-for="s in sources" :key="s" :label="srcName[s]" :value="s" />
      </el-select>
      <el-radio-group v-model="range">
        <el-radio-button value="24h">24时</el-radio-button>
        <el-radio-button value="7d">7天</el-radio-button>
        <el-radio-button value="30d">30天</el-radio-button>
        <el-radio-button value="1y">1年</el-radio-button>
      </el-radio-group>
      <el-radio-group v-model="field">
        <el-radio-button value="mid">中间价</el-radio-button>
        <el-radio-button value="fx_bid">现汇买入</el-radio-button>
        <el-radio-button value="fx_ask">现汇卖出</el-radio-button>
      </el-radio-group>
      <el-button type="primary" plain @click="dialogVisible = true">人工录入</el-button>
    </div>
    <TrendChart v-if="picked.length" :key="`${pair}-${refreshKey}`" :pair="pair" :sources="picked"
      :from="fromTo.from" :to="fromTo.to" :field="field" height="480px" />
    <el-empty v-else description="请至少选择一个数据源" />
    <ManualEntryDialog v-model:visible="dialogVisible" :pairs="pairs" @saved="refreshKey++" />
  </div>
</template>
```

- [ ] **Step 5: 验证 + Commit**

```bash
cd frontend && npm run build   # 零错误
# 手动：npm run dev + 后端运行 → 总览页表格/迷你图、图表页联动、人工录入落库
git add -A && git commit -m "feat(frontend): 汇率总览页 + 多源趋势图 + 人工录入

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 16: 告警规则 / 通知历史 / 存款管理页

**Files:**
- Modify: `frontend/src/views/Rules.vue`、`frontend/src/views/Notifications.vue`、`frontend/src/views/Deposits.vue`（替换占位）

**Interfaces:**
- Consumes: `/api/alert-rules` CRUD、`/api/notifications`、`/api/deposits` CRUD + `/api/deposits/summary`

- [ ] **Step 1: Rules.vue**

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { apiDel, apiGet, apiPost, apiPut } from '../api/client'

interface Rule {
  id: number; source: string; pair: string; price_type: string; direction: string
  threshold: string; cooldown_minutes: number; enabled: boolean
}

const rows = ref<Rule[]>([])
const pairs = ['USD/CNY', 'EUR/CNY', 'JPY/CNY', 'HKD/CNY', 'GBP/CNY', 'AUD/CNY']
const sources = [
  { v: 'api', label: '市场基准' }, { v: 'boc', label: '中行' },
  { v: 'icbc', label: '工行' }, { v: 'manual', label: '人工' },
]
const priceTypes = [
  { v: 'fx_bid', label: '现汇买入价' }, { v: 'fx_ask', label: '现汇卖出价' }, { v: 'mid', label: '中间价' },
]
const srcLabel = Object.fromEntries(sources.map((s) => [s.v, s.label]))
const ptLabel = Object.fromEntries(priceTypes.map((s) => [s.v, s.label]))

const empty = { source: 'api', pair: 'USD/CNY', price_type: 'mid', direction: 'above', threshold: 7.3, cooldown_minutes: 60, enabled: true }
const dialog = ref(false)
const editId = ref<number | null>(null)
const form = ref({ ...empty })

async function load() {
  rows.value = await apiGet<Rule[]>('/alert-rules')
}
onMounted(load)

function openNew() {
  editId.value = null
  form.value = { ...empty }
  dialog.value = true
}
function openEdit(r: Rule) {
  editId.value = r.id
  form.value = { ...r, threshold: Number(r.threshold) }
  dialog.value = true
}
async function save() {
  try {
    if (editId.value == null) await apiPost('/alert-rules', form.value)
    else await apiPut(`/alert-rules/${editId.value}`, form.value)
    dialog.value = false
    load()
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
async function remove(r: Rule) {
  await ElMessageBox.confirm('确认删除该规则？', '删除')
  await apiDel(`/alert-rules/${r.id}`)
  load()
}
</script>

<template>
  <div>
    <el-button type="primary" @click="openNew">新建规则</el-button>
    <el-table :data="rows" border style="margin-top: 12px">
      <el-table-column prop="id" label="ID" width="60" />
      <el-table-column label="来源" width="90">
        <template #default="{ row }">{{ srcLabel[row.source] ?? row.source }}</template>
      </el-table-column>
      <el-table-column prop="pair" label="货币对" width="110" />
      <el-table-column label="价位" width="110">
        <template #default="{ row }">{{ ptLabel[row.price_type] }}</template>
      </el-table-column>
      <el-table-column label="条件" width="80">
        <template #default="{ row }">{{ row.direction === 'above' ? '上破' : '下破' }}</template>
      </el-table-column>
      <el-table-column prop="threshold" label="阈值" width="100" />
      <el-table-column prop="cooldown_minutes" label="冷却(分)" width="90" />
      <el-table-column label="启用" width="80">
        <template #default="{ row }">
          <el-switch v-model="row.enabled" @change="apiPut(`/alert-rules/${row.id}`, row)" />
        </template>
      </el-table-column>
      <el-table-column label="操作">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-dialog v-model="dialog" :title="editId == null ? '新建规则' : '编辑规则'">
      <el-form label-width="100px">
        <el-form-item label="数据源">
          <el-select v-model="form.source">
            <el-option v-for="s in sources" :key="s.v" :label="s.label" :value="s.v" />
          </el-select>
        </el-form-item>
        <el-form-item label="货币对">
          <el-select v-model="form.pair">
            <el-option v-for="p in pairs" :key="p" :label="p" :value="p" />
          </el-select>
        </el-form-item>
        <el-form-item label="价位类型">
          <el-select v-model="form.price_type">
            <el-option v-for="p in priceTypes" :key="p.v" :label="p.label" :value="p.v" />
          </el-select>
        </el-form-item>
        <el-form-item label="方向">
          <el-radio-group v-model="form.direction">
            <el-radio-button value="above">上破</el-radio-button>
            <el-radio-button value="below">下破</el-radio-button>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="阈值"><el-input-number v-model="form.threshold" :precision="4" :step="0.01" /></el-form-item>
        <el-form-item label="冷却(分钟)"><el-input-number v-model="form.cooldown_minutes" :min="1" /></el-form-item>
        <el-form-item label="启用"><el-switch v-model="form.enabled" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialog = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
```

- [ ] **Step 2: Notifications.vue**

```vue
<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { apiGet } from '../api/client'

interface Row {
  id: number; channel: string; title: string; body: string; status: string
  retries: number; error: string | null; created_at: string
}

const rows = ref<Row[]>([])
const total = ref(0)
const page = ref(1)
const channel = ref<string>('')
const status = ref<string>('')

async function load() {
  const p = new URLSearchParams({ page: String(page.value), page_size: '20' })
  if (channel.value) p.set('channel', channel.value)
  if (status.value) p.set('status', status.value)
  const r = await apiGet<{ rows: Row[]; total: number }>(`/notifications?${p}`)
  rows.value = r.rows
  total.value = r.total
}
onMounted(load)
</script>

<template>
  <div>
    <div style="display: flex; gap: 12px; margin-bottom: 12px">
      <el-select v-model="channel" clearable placeholder="渠道" style="width: 140px" @change="page = 1; load()">
        <el-option label="企业微信" value="wecom" />
        <el-option label="Telegram" value="telegram" />
      </el-select>
      <el-select v-model="status" clearable placeholder="状态" style="width: 140px" @change="page = 1; load()">
        <el-option label="已送达" value="sent" />
        <el-option label="失败" value="failed" />
      </el-select>
    </div>
    <el-table :data="rows" border>
      <el-table-column prop="id" label="ID" width="70" />
      <el-table-column label="渠道" width="110">
        <template #default="{ row }">{{ row.channel === 'wecom' ? '企业微信' : 'Telegram' }}</template>
      </el-table-column>
      <el-table-column prop="title" label="标题" width="120" />
      <el-table-column prop="body" label="内容" show-overflow-tooltip />
      <el-table-column label="状态" width="90">
        <template #default="{ row }">
          <el-tag :type="row.status === 'sent' ? 'success' : 'danger'">
            {{ row.status === 'sent' ? '已送达' : '失败' }}
          </el-tag>
        </template>
      </el-table-column>
      <el-table-column prop="retries" label="重试" width="70" />
      <el-table-column label="时间" width="180">
        <template #default="{ row }">{{ new Date(row.created_at).toLocaleString() }}</template>
      </el-table-column>
    </el-table>
    <el-pagination style="margin-top: 12px" layout="prev, pager, next" :total="total"
      :page-size="20" v-model:current-page="page" @current-change="load" />
  </div>
</template>
```

- [ ] **Step 3: Deposits.vue**

```vue
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { apiDel, apiGet, apiPost, apiPut } from '../api/client'
import { formatMoney } from '../utils/format'

interface Deposit {
  id: number; person: string; bank: string; currency: string; amount: string; rate: string
  start_date: string; term_months: number; maturity_date: string
  remind_7d: boolean; remind_1d: boolean; remind_today: boolean; note: string | null
}
interface Summary {
  currencies: { currency: string; count: number; total: string; total_maturity: string }[]
  due_30d: Deposit[]
}

const rows = ref<Deposit[]>([])
const summary = ref<Summary | null>(null)
const mask = ref(localStorage.getItem('mask') === '1')

const empty = {
  person: '', bank: '', currency: 'CNY', amount: 10000, rate: 2.5,
  start_date: new Date().toISOString().slice(0, 10), term_months: 12,
  remind_7d: true, remind_1d: true, remind_today: true, note: '',
}
const dialog = ref(false)
const editId = ref<number | null>(null)
const form = ref({ ...empty })

const interestPreview = computed(() => {
  const f = form.value
  return (f.amount * f.rate / 100 * f.term_months / 12).toFixed(2)
})
const maturityPreview = computed(() => {
  const d = new Date(form.value.start_date)
  d.setMonth(d.getMonth() + form.value.term_months)
  return d.toISOString().slice(0, 10)
})

async function load() {
  rows.value = await apiGet<Deposit[]>('/deposits')
  summary.value = await apiGet<Summary>('/deposits/summary')
}
onMounted(load)

function toggleMask() {
  mask.value = !mask.value
  localStorage.setItem('mask', mask.value ? '1' : '0')
}
function openNew() {
  editId.value = null
  form.value = { ...empty }
  dialog.value = true
}
function openEdit(d: Deposit) {
  editId.value = d.id
  form.value = { ...d, amount: Number(d.amount), rate: Number(d.rate), note: d.note ?? '' }
  dialog.value = true
}
async function save() {
  try {
    if (editId.value == null) await apiPost('/deposits', form.value)
    else await apiPut(`/deposits/${editId.value}`, form.value)
    dialog.value = false
    load()
  } catch (e) {
    ElMessage.error((e as Error).message)
  }
}
async function remove(d: Deposit) {
  await ElMessageBox.confirm(`确认删除 ${d.person} 的 ${d.bank} 存款？`, '删除')
  await apiDel(`/deposits/${d.id}`)
  load()
}
</script>

<template>
  <div v-if="summary">
    <el-row :gutter="12" style="margin-bottom: 12px">
      <el-col v-for="c in summary.currencies" :key="c.currency" :span="8">
        <el-card>
          <template #header>
            {{ c.currency }} · {{ c.count }} 笔
            <el-button link style="float: right" @click="toggleMask">
              {{ mask ? '显示金额' : '隐藏金额' }}
            </el-button>
          </template>
          <p>本金合计：{{ formatMoney(c.total, mask) }}</p>
          <p>到期本息：{{ formatMoney(c.total_maturity, mask) }}</p>
        </el-card>
      </el-col>
    </el-row>

    <h3>未来 30 天到期</h3>
    <el-table :data="summary.due_30d" size="small" border>
      <el-table-column prop="maturity_date" label="到期日" width="120" />
      <el-table-column prop="person" label="人员" width="100" />
      <el-table-column prop="bank" label="银行" width="120" />
      <el-table-column label="金额">
        <template #default="{ row }">{{ formatMoney(row.amount, mask) }}</template>
      </el-table-column>
    </el-table>

    <div style="display: flex; justify-content: space-between; margin: 16px 0 12px">
      <h3 style="margin: 0">全部存款</h3>
      <el-button type="primary" @click="openNew">新增存款</el-button>
    </div>
    <el-table :data="rows" border>
      <el-table-column prop="person" label="人员" width="90" />
      <el-table-column prop="bank" label="银行" width="110" />
      <el-table-column prop="currency" label="币种" width="70" />
      <el-table-column label="金额">
        <template #default="{ row }">{{ formatMoney(row.amount, mask) }}</template>
      </el-table-column>
      <el-table-column label="年利率" width="90">
        <template #default="{ row }">{{ Number(row.rate) }}%</template>
      </el-table-column>
      <el-table-column prop="start_date" label="起存日" width="110" />
      <el-table-column prop="term_months" label="期限(月)" width="85" />
      <el-table-column prop="maturity_date" label="到期日" width="110" />
      <el-table-column label="提醒" width="150">
        <template #default="{ row }">
          <el-tag v-if="row.remind_7d" size="small" style="margin-right: 4px">7天</el-tag>
          <el-tag v-if="row.remind_1d" size="small" style="margin-right: 4px">1天</el-tag>
          <el-tag v-if="row.remind_today" size="small">当天</el-tag>
        </template>
      </el-table-column>
      <el-table-column label="操作" width="130">
        <template #default="{ row }">
          <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
          <el-button link type="danger" @click="remove(row)">删除</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-dialog v-model="dialog" :title="editId == null ? '新增存款' : '编辑存款'">
      <el-form label-width="100px">
        <el-form-item label="人员"><el-input v-model="form.person" /></el-form-item>
        <el-form-item label="银行"><el-input v-model="form.bank" /></el-form-item>
        <el-form-item label="币种">
          <el-select v-model="form.currency" style="width: 120px">
            <el-option v-for="c in ['CNY', 'USD', 'EUR', 'JPY', 'HKD', 'GBP', 'AUD']" :key="c" :label="c" :value="c" />
          </el-select>
        </el-form-item>
        <el-form-item label="金额"><el-input-number v-model="form.amount" :min="0.01" :precision="2" /></el-form-item>
        <el-form-item label="年利率(%)"><el-input-number v-model="form.rate" :min="0" :precision="3" :step="0.1" /></el-form-item>
        <el-form-item label="起存日"><el-date-picker v-model="form.start_date" type="date" value-format="YYYY-MM-DD" /></el-form-item>
        <el-form-item label="期限(月)"><el-input-number v-model="form.term_months" :min="1" /></el-form-item>
        <el-form-item label="到期日"><span>{{ maturityPreview }}</span></el-form-item>
        <el-form-item label="预计利息"><span>{{ interestPreview }}</span></el-form-item>
        <el-form-item label="提醒档位">
          <el-checkbox v-model="form.remind_7d" label="7天" />
          <el-checkbox v-model="form.remind_1d" label="1天" />
          <el-checkbox v-model="form.remind_today" label="当天" />
        </el-form-item>
        <el-form-item label="备注"><el-input v-model="form.note" /></el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialog = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>
```

- [ ] **Step 4: 验证 + Commit**

```bash
npm run build   # 零错误
# 手动过一遍三个页面 CRUD（后端 + dev server 运行中）
git add -A && git commit -m "feat(frontend): 告警规则 / 通知历史 / 存款管理页

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 17: 部署（Docker 三容器 + 备份 + README）

**Files:**
- Create: `.dockerignore`、`deploy/Dockerfile.backend`、`deploy/Dockerfile.frontend`、`deploy/nginx.conf`、`deploy/docker-compose.prod.yml`、`deploy/.env.example`、`README.md`

**Interfaces:**
- Produces: `docker compose -f deploy/docker-compose.prod.yml up -d --build` 一键起全套；`http://<主机>/` 前端、`/api` 反代后端；每日 pg_dump 到 `data/backups/` 保留 30 天

- [ ] **Step 1: 根目录 .dockerignore**

```
target/
node_modules/
dist/
data/
.git/
frontend/node_modules/
backend/target/
```

- [ ] **Step 2: Dockerfile.backend**

```dockerfile
FROM rust:1.84-bookworm AS build
WORKDIR /app
COPY backend/ .
RUN cargo build --release --bin fintrack

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/fintrack /usr/local/bin/fintrack
WORKDIR /app
ENTRYPOINT ["fintrack"]
```

- [ ] **Step 3: Dockerfile.frontend + nginx.conf**

```dockerfile
FROM node:20-alpine AS build
WORKDIR /app
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build

FROM nginx:alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
```

`deploy/nginx.conf`：

```nginx
server {
    listen 80;

    location /api/ {
        proxy_pass http://backend:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location /healthz {
        proxy_pass http://backend:8080;
    }

    location / {
        root /usr/share/nginx/html;
        try_files $uri $uri/ /index.html;
    }
}
```

- [ ] **Step 4: docker-compose.prod.yml + .env.example**

`deploy/docker-compose.prod.yml`：

```yaml
services:
  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: fx
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-fx}
      POSTGRES_DB: fx
    volumes:
      - ../data/pg:/var/lib/postgresql/data
    restart: unless-stopped

  backend:
    build:
      context: ..
      dockerfile: deploy/Dockerfile.backend
    environment:
      DATABASE_URL: postgres://fx:${POSTGRES_PASSWORD:-fx}@db:5432/fx
      ADMIN_PASSWORD: ${ADMIN_PASSWORD}
      WECOM_WEBHOOK_URL: ${WECOM_WEBHOOK_URL:-}
      TELEGRAM_BOT_TOKEN: ${TELEGRAM_BOT_TOKEN:-}
      TELEGRAM_CHAT_ID: ${TELEGRAM_CHAT_ID:-}
      TELEGRAM_PROXY: ${TELEGRAM_PROXY:-}
      SCRAPE_INTERVAL_SECS: ${SCRAPE_INTERVAL_SECS:-180}
    depends_on:
      - db
    restart: unless-stopped

  frontend:
    build:
      context: ..
      dockerfile: deploy/Dockerfile.frontend
    ports:
      - "80:80"
    depends_on:
      - backend
    restart: unless-stopped

  backup:
    image: prodrigestivill/postgres-backup-local:16-alpine
    environment:
      POSTGRES_HOST: db
      POSTGRES_USER: fx
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD:-fx}
      POSTGRES_DB: fx
      SCHEDULE: "@daily"
      BACKUP_DIR: /backups
      BACKUP_KEEP_DAYS: 30
    volumes:
      - ../data/backups:/backups
    depends_on:
      - db
    restart: unless-stopped
```

`deploy/.env.example`：

```
POSTGRES_PASSWORD=改成强密码
ADMIN_PASSWORD=cargo run --bin pwd_hash -- 你的密码 输出的哈希
WECOM_WEBHOOK_URL=
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
TELEGRAM_PROXY=socks5://host.docker.internal:1080
SCRAPE_INTERVAL_SECS=180
```

- [ ] **Step 5: README.md**

```markdown
# 汇率监控与定存管理系统

Rust(axum) + Vue3 + PostgreSQL。多源汇率采集（frankfurter API / 中行 / 工行网页抓取 / 人工录入）、
阈值告警推送（企业微信 + Telegram）、图表、定期存款与到期提醒。详见 docs/SPEC.md。

## 开发
    docker compose -f deploy/docker-compose.yml up -d db
    cd backend && cargo run          # fintrack.toml 或环境变量（见 backend/fintrack.toml.example）
    cd frontend && npm i && npm run dev

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
- 工行抓取端点若核验失败会以"数据源失效"呈现，不影响其他功能（见 Task 10 说明）
- 基准汇率为 ECB 参考汇率（每日一次），非央行中间价（SPEC 附决策记录 #12）
```

- [ ] **Step 6: 验证 + Commit**

```bash
docker compose -f deploy/docker-compose.prod.yml up -d --build
curl -s http://localhost/healthz          # ok（经 nginx 反代）
curl -s http://localhost/api/rates/latest # 401（未登录，符合预期）
docker compose -f deploy/docker-compose.prod.yml down
git add -A && git commit -m "feat(deploy): 生产 compose（backend/frontend/db/backup）+ README

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

## Spec 覆盖对照（自查表）

| SPEC 章节 | 覆盖任务 |
|---|---|
| §1 定位与用户模型 | Task 3（单管理员认证）、14（登录页） |
| §2 技术栈/结构 | Task 1（脚手架）、17（compose） |
| §3.1 范围（6 货币） | Task 1（config 默认对）、9/10（币种映射） |
| §3.2 三层数据源 | Task 5（API）、9（BOC）、10（ICBC）、4（manual） |
| §3.3 频率 | Task 13（interval 可配置）、15（前端 60s 刷新） |
| §3.4 历史回填 | Task 5（ingest_api_rates 首次 365 天） |
| §3.5 抓取器健康 | Task 8（HealthRegistry）、13（状态灯 API + 失效/恢复推送） |
| §4.1 规则模型（冷却/再武装） | Task 6（evaluate 语义 + 测试） |
| §4.2 渠道 | Task 7（wecom/telegram + 代理） |
| §4.3 可靠性（入库/重试3次） | Task 7（dispatch_with_backoff） |
| §4.4 管理界面 | Task 6（CRUD API）、16（Rules.vue） |
| §5 存款字段/币种/汇总 | Task 11（模型 + summary）、16（Deposits.vue + 打码） |
| §5.2 到期提醒 | Task 12（档位/防重）、13（09:05 任务） |
| §6 数据模型 | Task 2（0001_init.sql，5 张表） |
| §7 API | Task 4/6/7/11/13（全部路由） |
| §8 页面（6 个） | Task 14/15/16 |
| §9 认证与安全 | Task 3（argon2 + 30 天 cookie） |
| §10 运维（备份/保留 2 年） | Task 2（drop_old_partitions）、13（每日维护）、17（backup 容器） |
| §11 环境变量 | Task 1（config）、17（.env.example） |
| §12 测试策略 | 各任务 TDD + 9/10 fixture 测试 |
| §13 用户自操事项 | Task 17（README 指引） |

