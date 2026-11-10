# 汇率监控与定存管理系统 — 需求规格说明书

- **版本**：1.0（共识版）
- **日期**：2026-11-10
- **状态**：已经三轮需求拷问（grilling）逐项确认，可进入规划/实现
- **来源**：`需求.txt` + 三轮问答共识（全部采用推荐方案）

---

## 1. 定位与用户模型

- **单管理员**工具，家庭财务场景（汇率监控 + 定期存款管理）
- 无多账号、无注册、无权限体系；存款记录的"人员"仅为**标签字段**
- 部署于内网设备（NAS / 小主机 / VPS），通过 **Tailscale** 远程访问，**不暴露公网端口**

## 2. 技术栈

| 层 | 选型 |
|---|---|
| 后端 | Rust + **axum** + **sqlx**（PostgreSQL）+ tokio；定时任务自写 interval 循环，不引任务框架 |
| 前端 | **Vite + Vue3 + TypeScript + Element Plus + vue-echarts** |
| 数据库 | PostgreSQL，原生**按月分区表**，不引 TimescaleDB |
| 部署 | **docker-compose 三容器**（postgres / backend / nginx+前端静态文件），`data/` 挂载数据与备份，密钥全走环境变量 |
| 配置 | 后端 `config` crate 读 TOML + 环境变量覆盖 |

### 项目结构

```
金融/
  backend/      # Rust workspace（axum 服务）
  frontend/     # Vite + Vue3
  deploy/       # docker-compose.yml、Dockerfile.backend、Dockerfile.frontend、nginx.conf
  docs/         # SPEC.md 等文档
```

## 3. 汇率采集

### 3.1 范围

- **货币对**：USD、EUR、JPY、HKD、GBP、AUD 兑 CNY（共 6 个）；后续新增货币仅改配置，不改表结构
- **价格类型**：现汇买入价（fx_bid）、现汇卖出价（fx_ask）、中间价/基准价（mid）

### 3.2 数据源（三层混合）

1. **市场基准汇率**：免费 API（frankfurter / ECB 参考汇率），每日更新一次，稳定不依赖抓取
   - ⚠️ ECB 参考汇率 ≠ 央行（CFETS）中间价，数值略有差异；第一版以 ECB 参考汇率为基准源（已确认）。若需严格央行中间价，后续可追加抓取中国货币网
2. **银行牌价**：网页抓取，第一版接**中国银行、工商银行**；每家银行一个独立抓取器模块，新增银行不改表结构
3. **人工录入**：兜底与"实际成交价记录"，`source='manual'`，与抓取数据**同表存储**；图表可显示、可与银行价对比；告警规则**默认不**基于人工数据评估（除非建规则时显式选择）

### 3.3 频率与实时性

- "实时" = **准实时**：抓取间隔 **1~5 分钟可配置**（银行牌价物理上限即分钟级）
- 前端图表 **30s~1min 自动刷新**（轮询）
- 基准 API 汇率每日抓一次（或每小时的幂等刷新）

### 3.4 历史回填

- 首次启动自动回填**近 1 年基准汇率历史**（API 支持）
- 银行历史牌价无法回溯，从部署日起积累

### 3.5 抓取器健康

- 连续失败 **3 次** → 推送"数据源失效"；恢复后推送"已恢复"；两者均进通知历史
- 总览页顶部常驻**数据源状态灯**（`GET /api/sources/status`）

## 4. 告警与推送

### 4.1 规则模型

```
规则 = 汇率源(哪家银行哪个价位 / 基准API)
     + 货币对
     + 价格类型(fx_bid | fx_ask | mid)
     + 方向(above 上破 | below 下破)
     + 阈值(绝对值)
     + 冷却期(默认 60 分钟)
     + 启用开关
```

- **触发后冷却**：冷却期内不重推；价格**恢复正常后再武装**（防止阈值附近抖动轰炸）
- **评估时机**：每收到新数据点立即评估所有启用规则（无独立评估循环）

### 4.2 推送渠道

- **企业微信群机器人 webhook**（用户自行注册企业微信建群获取 URL）
- **Telegram Bot API**（@BotFather 创建，容器环境变量配 SOCKS5/HTTP 代理）
- 后端 `Notifier` trait 抽象渠道，实现 wecom / telegram 两个实现

### 4.3 可靠性

- 每条通知**入库**（时间、渠道、标题、内容、状态、重试次数、错误信息）
- 失败**指数退避重试 3 次**，最终失败在通知历史中标红

### 4.4 管理界面

- 告警规则**前端 UI 增删改查**（存 DB），每条规则可启用/停用

## 5. 存款管理

### 5.1 字段

| 字段 | 说明 |
|---|---|
| person | 人员（标签） |
| bank | 银行 |
| currency | 币种，默认 `CNY`；**支持外币定存，不做自动折算** |
| amount | 金额 |
| rate | 年利率（%） |
| start_date | 起存日 |
| term_months | 期限（月） |
| maturity_date | 到期日 |
| remind_7d / remind_1d / remind_today | 提醒档位开关（默认全开，可单档关闭） |
| notified_7d / notified_1d / notified_today | 已推送日期，防重复推送 |
| note | 备注 |

- 到期利息 = `amount × rate × term_months / 12`，到期本息自动计算展示
- 汇总卡**按币种分组统计**（总额、预计到期利息、未来 30 天到期），**不跨币种加总**

### 5.2 到期提醒

- 到期前 **7 天、1 天、当天**各推一次
- 通道复用告警推送（企业微信 + Telegram），统一发给管理员
- 每日 **09:05（Asia/Shanghai）** 定时扫描存款表

## 6. 数据模型（5 张业务表）

```sql
-- 汇率明细（按月分区）
exchange_rates (
  id           bigserial,
  source       text,        -- 'api'(基准) | 'boc' | 'icbc' | 'manual'
  pair         text,        -- 'USD/CNY'
  fx_bid       numeric,     -- 现汇买入价（银行源）
  fx_ask       numeric,     -- 现汇卖出价
  mid          numeric,     -- 中间价/基准价
  recorded_at  timestamptz,
  note         text         -- manual 专用备注
)

-- 告警规则（纯配置）
alert_rules (
  id, source, pair, price_type,   -- fx_bid | fx_ask | mid
  direction,                      -- above | below
  threshold    numeric,
  cooldown_minutes int default 60,
  enabled      bool,
  created_at
)

-- 规则运行状态（每规则一行）
alert_rule_state (
  rule_id  pk fk,
  armed    bool,             -- 是否武装（冷却/恢复后重新武装）
  last_triggered_at,
  last_value
)

-- 通知记录
notifications (
  id, channel,               -- wecom | telegram
  title, body,
  status,                    -- sent | failed
  retries, error, created_at
)

-- 定期存款
deposits ( …见 5.1… )
```

## 7. API 设计

```
POST /api/auth/login            # 登录
POST /api/auth/logout
GET  /api/auth/me

GET  /api/rates/latest                       # 今日各行牌价对比
GET  /api/rates/history                      # ?pair&sources&from&to&points
                                             # 服务端分桶聚合，返回几百个点
POST /api/rates/manual                       # 人工录入

GET  /api/sources/status                     # 抓取器健康

GET|POST|PUT|DELETE /api/alert-rules[/:id]

GET  /api/notifications                      # ?channel&status&page

GET|POST|PUT|DELETE /api/deposits[/:id]
GET  /api/deposits/summary                   # 按币种汇总 + 30天到期
```

- 除 login 外全部经过认证中间件
- 图表聚合在**服务端**做，不把原始明细行丢给前端

## 8. 页面（6 个）

| 页面 | 内容 |
|---|---|
| 登录 | 单管理员密码登录 |
| 汇率总览（首页） | 今日各行牌价对比表 + 各货币 24h 迷你趋势 + 数据源状态灯 |
| 汇率图表 | 单货币多源叠加趋势，时间范围 24h/7d/30d/1y/自定义；含人工录入按钮 |
| 告警规则 | 规则增删改 + 启用开关 |
| 通知历史 | 已推记录，失败标红 |
| 存款管理 | 列表 + 录入/编辑 + 汇总卡 |

- **不做**设置页：管理员密码、webhook 地址等全走环境变量

## 9. 认证与安全

- `ADMIN_PASSWORD` 环境变量存 **argon2 哈希**；代码提供生成哈希的小命令
- 登录后签发 **30 天长效 signed cookie**（axum session）；改密码 = 改环境变量重启
- 存款数据**不做字段级加密**（Tailscale 链路加密 + 单管理员，字段加密会毁掉统计查询）；前端金额列提供**打码显示**开关
- 无 HTTPS（内网），Tailscale 自带加密链路

## 10. 运维

- **备份**：每日自动 `pg_dump`，保留 30 份，存 `data/backups/`
- **数据保留**：汇率原始数据保留 **2 年**，超期自动 drop 最老分区（分区机制天然支持）
- 时区统一 **Asia/Shanghai**，时间戳存 `timestamptz`

## 11. 环境变量清单

| 变量 | 说明 |
|---|---|
| `DATABASE_URL` | PG 连接串 |
| `ADMIN_PASSWORD` | argon2 哈希 |
| `WECOM_WEBHOOK_URL` | 企业微信机器人 webhook |
| `TELEGRAM_BOT_TOKEN` | TG bot token |
| `TELEGRAM_CHAT_ID` | TG 接收会话 ID |
| `TELEGRAM_PROXY` | TG 代理（socks5://…，大陆环境必需） |
| `SCRAPE_INTERVAL_SECS` | 抓取间隔（默认 180） |

## 12. 测试策略

- **纯函数单测**：告警规则引擎（触发/冷却/再武装）、到期日期计算（7天/1天/当天档位）、告警去重
- **抓取器 fixture 测试**：每家银行保存一份真实 HTML 快照做解析测试——页面改版时测试立刻红，而不是线上静默断数据

## 13. 用户自行操作事项（代码之外）

1. 注册企业微信 → 建群 → 获取群机器人 webhook URL
2. @BotFather 创建 Telegram Bot → 拿 token / chat_id → 准备代理
3. 部署设备与访问设备安装 Tailscale 并组网

---

## 附：已确认的关键决策记录

| # | 决策 | 结论 |
|---|---|---|
| 1 | 用户模型 | 单管理员，"人员"为标签字段 |
| 2 | 范围 | 6 货币兑 CNY + 中行/工行起步，可配置扩展 |
| 3 | 实时性 | 准实时（1~5 分钟抓取），秒级无意义 |
| 4 | 采集策略 | 混合：API（基准）+ 抓取（银行）+ 人工兜底 |
| 5 | 告警语义 | 阈值 + 冷却 + 恢复再武装，不持续推送 |
| 6 | 微信通道 | 企业微信群机器人 webhook（个人微信无官方推送） |
| 7 | 到期提醒 | 7天/1天/当天，档位可关，复用推送通道 |
| 8 | 部署 | Tailscale 内网组网，不暴露公网 |
| 9 | 时序存储 | 原生 PG 按月分区，保留 2 年，不用 TimescaleDB |
| 10 | 历史回填 | 基准汇率回填 1 年，银行牌价从零积累 |
| 11 | 外币定存 | currency 字段支持，不自动折算 |
| 12 | 基准源 | ECB 参考汇率（API），非央行中间价（已知情选择） |
