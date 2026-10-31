-- Enable TimescaleDB extension (Required for hypertables)
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- LAYER 1
CREATE TABLE l1_fundamentals_quarterly (
    report_date TIMESTAMPTZ NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    fiscal_period VARCHAR(10),
    revenue_growth_yoy NUMERIC(10, 4),
    eps_growth_yoy NUMERIC(10, 4),
    gross_margin NUMERIC(10, 4),
    operating_margin NUMERIC(10, 4),
    roic NUMERIC(10, 4),
    fcf_margin NUMERIC(10, 4),
    free_cash_flow NUMERIC,
    debt_to_equity NUMERIC(10, 4),
    pe_ratio NUMERIC(10, 4),
    ev_to_ebitda NUMERIC(10, 4),
    shares_outstanding NUMERIC
);
SELECT create_hypertable('l1_fundamentals_quarterly', 'report_date');
CREATE INDEX idx_l1_symbol_date ON l1_fundamentals_quarterly (symbol, report_date DESC);

-- LAYER 2
CREATE TABLE l2_earnings_context (
    earnings_date TIMESTAMPTZ NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    market_time VARCHAR(10),
    estimated_eps NUMERIC(10, 4),
    actual_eps NUMERIC(10, 4),
    eps_surprise_pct NUMERIC(10, 4),
    estimated_revenue NUMERIC,
    actual_revenue NUMERIC,
    revenue_surprise_pct NUMERIC(10, 4),
    forward_revenue_guidance NUMERIC,
    guidance_revision_sentiment VARCHAR(20)
);
SELECT create_hypertable('l2_earnings_context', 'earnings_date');
CREATE INDEX idx_l2_symbol_date ON l2_earnings_context (symbol, earnings_date DESC);

-- LAYER 3 & 5
CREATE TABLE l3_l5_options_greeks_snapshots (
    snapshot_time TIMESTAMPTZ NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    contract_symbol VARCHAR(30) NOT NULL,
    expiration_date DATE NOT NULL,
    days_to_expiration INT NOT NULL,
    strike NUMERIC(10, 2) NOT NULL,
    option_type VARCHAR(4) NOT NULL,
    bid_price NUMERIC(10, 2),
    ask_price NUMERIC(10, 2),
    last_price NUMERIC(10, 2),
    volume INT,
    open_interest INT,
    implied_volatility NUMERIC(10, 4),
    delta NUMERIC(10, 4),
    gamma NUMERIC(10, 4),
    theta NUMERIC(10, 4),
    vega NUMERIC(10, 4)
);
SELECT create_hypertable('l3_l5_options_greeks_snapshots', 'snapshot_time');
CREATE INDEX idx_l3_l5_lookup ON l3_l5_options_greeks_snapshots (symbol, expiration_date, snapshot_time DESC);

-- LAYER 4
CREATE TABLE l4_volatility_reality_check (
    earnings_date TIMESTAMPTZ NOT NULL,
    symbol VARCHAR(10) NOT NULL,
    pre_er_stock_price NUMERIC(10, 2),
    pre_er_iv_rank NUMERIC(10, 4),
    atm_straddle_price NUMERIC(10, 2),
    implied_move_pct NUMERIC(10, 4),
    post_er_stock_price NUMERIC(10, 2),
    actual_move_pct NUMERIC(10, 4),
    volatility_ratio NUMERIC(10, 4)
);
SELECT create_hypertable('l4_volatility_reality_check', 'earnings_date');
CREATE INDEX idx_l4_symbol_date ON l4_volatility_reality_check (symbol, earnings_date DESC);


-- 4th Oct - remove duplicate entry when api called multiple time feature
-- 1. LAYER 1: Fundamentals
DELETE FROM l1_fundamentals_quarterly WHERE ctid IN (
    SELECT ctid FROM (SELECT ctid, ROW_NUMBER() OVER(PARTITION BY symbol, fiscal_period ORDER BY report_date DESC) as rn FROM l1_fundamentals_quarterly) t WHERE t.rn > 1
);
UPDATE l1_fundamentals_quarterly SET report_date = DATE_TRUNC('day', report_date);
ALTER TABLE l1_fundamentals_quarterly ADD CONSTRAINT uq_l1 UNIQUE (symbol, fiscal_period, report_date);

-- 2. LAYER 2: Earnings Context
DELETE FROM l2_earnings_context WHERE ctid IN (
    SELECT ctid FROM (SELECT ctid, ROW_NUMBER() OVER(PARTITION BY symbol, market_time ORDER BY earnings_date DESC) as rn FROM l2_earnings_context) t WHERE t.rn > 1
);
UPDATE l2_earnings_context SET earnings_date = DATE_TRUNC('day', earnings_date);
ALTER TABLE l2_earnings_context ADD CONSTRAINT uq_l2 UNIQUE (symbol, market_time, earnings_date);

-- 3. LAYER 3 & 5: Options Greeks
DELETE FROM l3_l5_options_greeks_snapshots WHERE ctid IN (
    SELECT ctid FROM (SELECT ctid, ROW_NUMBER() OVER(PARTITION BY symbol, contract_symbol, DATE_TRUNC('minute', snapshot_time) ORDER BY snapshot_time DESC) as rn FROM l3_l5_options_greeks_snapshots) t WHERE t.rn > 1
);
UPDATE l3_l5_options_greeks_snapshots SET snapshot_time = DATE_TRUNC('minute', snapshot_time);
ALTER TABLE l3_l5_options_greeks_snapshots ADD CONSTRAINT uq_l3_l5 UNIQUE (symbol, contract_symbol, snapshot_time);

-- 4. LAYER 4: Volatility Check
DELETE FROM l4_volatility_reality_check WHERE ctid IN (
    SELECT ctid FROM (SELECT ctid, ROW_NUMBER() OVER(PARTITION BY symbol, DATE(earnings_date) ORDER BY earnings_date DESC) as rn FROM l4_volatility_reality_check) t WHERE t.rn > 1
);
UPDATE l4_volatility_reality_check SET earnings_date = DATE_TRUNC('day', earnings_date);
ALTER TABLE l4_volatility_reality_check ADD CONSTRAINT uq_l4 UNIQUE (symbol, earnings_date);



-- 31th Oct - add tag to the extracted symbol/companies
ALTER TABLE l1_fundamentals_quarterly ADD COLUMN screener_tag VARCHAR(50);