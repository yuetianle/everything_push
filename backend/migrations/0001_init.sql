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
