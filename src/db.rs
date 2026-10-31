use crate::models::{EarningsRecord, FundamentalsRecord, OptionGreekSnapshot};
use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub struct UnreconciledL2Record {
    pub symbol: String,
    pub market_time: String,
    pub estimated_eps: Option<f64>,
    pub actual_eps: Option<f64>,
    pub estimated_revenue: Option<f64>,
}

pub struct UnreconciledL4Record {
    pub symbol: String,
    pub earnings_date: chrono::DateTime<chrono::Utc>,
    pub pre_er_stock_price: f64,
    pub implied_move_pct: f64,
}


pub async fn init_db_pool(db_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
}

pub async fn insert_layer1_fundamentals(pool: &PgPool, record: &FundamentalsRecord) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    let query = r#"
        INSERT INTO l1_fundamentals_quarterly (
            report_date, symbol, fiscal_period, revenue_growth_yoy, eps_growth_yoy,
            gross_margin, operating_margin, roic, free_cash_flow, fcf_margin,
            debt_to_equity, pe_ratio, ev_to_ebitda, shares_outstanding, screener_tag
        )
        VALUES (DATE_TRUNC('day', $1::TIMESTAMPTZ), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (symbol, fiscal_period, report_date)
        DO UPDATE SET 
            revenue_growth_yoy = EXCLUDED.revenue_growth_yoy,
            eps_growth_yoy = EXCLUDED.eps_growth_yoy,
            gross_margin = EXCLUDED.gross_margin,
            operating_margin = EXCLUDED.operating_margin,
            roic = EXCLUDED.roic,
            free_cash_flow = EXCLUDED.free_cash_flow,
            fcf_margin = EXCLUDED.fcf_margin,
            debt_to_equity = EXCLUDED.debt_to_equity,
            pe_ratio = EXCLUDED.pe_ratio,
            ev_to_ebitda = EXCLUDED.ev_to_ebitda,
            shares_outstanding = EXCLUDED.shares_outstanding,
            screener_tag = EXCLUDED.screener_tag
    "#;

    sqlx::query(query)
        .bind(now).bind(&record.symbol).bind("CURRENT").bind(record.revenue_growth)
        .bind(record.eps_growth).bind(record.gross_margin).bind(record.operating_margin)
        .bind(record.roic).bind(record.free_cash_flow).bind(record.fcf_margin)
        .bind(record.debt_to_equity).bind(record.pe_ratio).bind(record.ev_to_ebitda)
        .bind(record.shares_outstanding).bind(&record.screener_tag) // <--- ADD THIS BIND
        .execute(pool).await?;

    Ok(())
}

pub async fn insert_layer2_earnings(pool: &PgPool, record: &EarningsRecord) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    let query = r#"
        INSERT INTO l2_earnings_context (
            earnings_date, symbol, market_time,
            estimated_eps, actual_eps, eps_surprise_pct,
            estimated_revenue, actual_revenue, revenue_surprise_pct,
            forward_revenue_guidance, guidance_revision_sentiment
        )
        VALUES (DATE_TRUNC('day', $1::TIMESTAMPTZ), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (symbol, market_time, earnings_date) 
        DO UPDATE SET 
            estimated_eps = EXCLUDED.estimated_eps,
            actual_eps = EXCLUDED.actual_eps,
            eps_surprise_pct = COALESCE(EXCLUDED.eps_surprise_pct, l2_earnings_context.eps_surprise_pct),
            estimated_revenue = EXCLUDED.estimated_revenue,
            actual_revenue = COALESCE(EXCLUDED.actual_revenue, l2_earnings_context.actual_revenue),
            forward_revenue_guidance = EXCLUDED.forward_revenue_guidance,
            guidance_revision_sentiment = EXCLUDED.guidance_revision_sentiment
    "#;

    sqlx::query(query)
        .bind(now).bind(&record.symbol).bind(&record.market_time).bind(record.estimated_eps)
        .bind(record.actual_eps).bind(record.eps_surprise_pct).bind(record.estimated_revenue)
        .bind(record.actual_revenue).bind(record.revenue_surprise_pct)
        .bind(record.forward_revenue_guidance).bind(record.guidance_revision_sentiment)
        .execute(pool).await?;

    Ok(())
}

pub async fn insert_layer3_5_greeks(pool: &PgPool, snapshot: &OptionGreekSnapshot) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    let query = r#"
        INSERT INTO l3_l5_options_greeks_snapshots (
            snapshot_time, symbol, contract_symbol, expiration_date, days_to_expiration,
            strike, option_type, bid_price, ask_price, last_price, volume, open_interest,
            implied_volatility, delta, gamma, theta, vega
        )
        VALUES (DATE_TRUNC('minute', $1::TIMESTAMPTZ), $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        ON CONFLICT (symbol, contract_symbol, snapshot_time)
        DO UPDATE SET
            bid_price = EXCLUDED.bid_price,
            ask_price = EXCLUDED.ask_price,
            last_price = EXCLUDED.last_price,
            volume = EXCLUDED.volume,
            open_interest = EXCLUDED.open_interest,
            implied_volatility = EXCLUDED.implied_volatility,
            delta = EXCLUDED.delta,
            gamma = EXCLUDED.gamma,
            theta = EXCLUDED.theta,
            vega = EXCLUDED.vega
    "#;

    sqlx::query(query)
        .bind(now).bind(&snapshot.symbol).bind(&snapshot.contract_symbol).bind(snapshot.expiration_date)
        .bind(snapshot.days_to_expiration).bind(snapshot.strike).bind(&snapshot.option_type)
        .bind(snapshot.bid).bind(snapshot.ask).bind(snapshot.last_price).bind(snapshot.volume)
        .bind(snapshot.open_interest).bind(snapshot.implied_volatility).bind(snapshot.delta)
        .bind(snapshot.gamma).bind(snapshot.theta).bind(snapshot.vega)
        .execute(pool).await?;

    Ok(())
}

pub async fn calculate_and_insert_layer4(
    pool: &PgPool,
    symbol: &str,
    stock_price: f64,
    atm_straddle_price: f64,
    iv_rank: Option<f64>,
) -> Result<(), sqlx::Error> {
    let now = Utc::now();
    let implied_move_pct = if stock_price > 0.0 { atm_straddle_price / stock_price } else { 0.0 };

    let query = r#"
        INSERT INTO l4_volatility_reality_check (
            earnings_date, symbol, pre_er_stock_price, pre_er_iv_rank,
            atm_straddle_price, implied_move_pct
        )
        VALUES (DATE_TRUNC('day', $1::TIMESTAMPTZ), $2, $3, $4, $5, $6)
        ON CONFLICT (symbol, earnings_date)
        DO UPDATE SET 
            pre_er_stock_price = EXCLUDED.pre_er_stock_price,
            pre_er_iv_rank = EXCLUDED.pre_er_iv_rank,
            atm_straddle_price = EXCLUDED.atm_straddle_price,
            implied_move_pct = EXCLUDED.implied_move_pct
    "#;

    sqlx::query(query)
        .bind(now).bind(symbol).bind(stock_price).bind(iv_rank)
        .bind(atm_straddle_price).bind(implied_move_pct)
        .execute(pool).await?;

    Ok(())
}

// Fetch rows with missing actual_revenue or missing surprise percentages
pub async fn get_unreconciled_l2_records(
    pool: &PgPool,
    symbol: &str,
) -> Result<Vec<UnreconciledL2Record>, sqlx::Error> {
    
    // Notice the ::FLOAT8 casts added to the 3 numeric columns
    let query = r#"
        SELECT 
            symbol, 
            market_time, 
            estimated_eps::FLOAT8, 
            actual_eps::FLOAT8, 
            estimated_revenue::FLOAT8
        FROM l2_earnings_context
        WHERE symbol = $1 AND (actual_revenue IS NULL OR eps_surprise_pct IS NULL)
    "#;

    let rows = sqlx::query_as::<_, (String, String, Option<f64>, Option<f64>, Option<f64>)>(query)
        .bind(symbol)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| UnreconciledL2Record {
            symbol: r.0,
            market_time: r.1,
            estimated_eps: r.2,
            actual_eps: r.3,
            estimated_revenue: r.4,
        })
        .collect())
}

// Update the l2 record with actuals and calculated surprise %
pub async fn update_layer2_actuals(
    pool: &PgPool,
    symbol: &str,
    market_time: &str,
    actual_revenue: Option<f64>,
    revenue_surprise_pct: Option<f64>,
    eps_surprise_pct: Option<f64>,
) -> Result<(), sqlx::Error> {
    let query = r#"
        UPDATE l2_earnings_context
        SET 
            actual_revenue = COALESCE($1, actual_revenue),
            revenue_surprise_pct = COALESCE($2, revenue_surprise_pct),
            eps_surprise_pct = COALESCE($3, eps_surprise_pct)
        WHERE symbol = $4 AND market_time = $5
    "#;

    sqlx::query(query)
        .bind(actual_revenue)
        .bind(revenue_surprise_pct)
        .bind(eps_surprise_pct)
        .bind(symbol)
        .bind(market_time)
        .execute(pool)
        .await?;

    Ok(())
}


// Fetch L4 records where post_er_stock_price is still NULL
pub async fn get_unreconciled_l4_records(
    pool: &PgPool,
    symbol: &str,
) -> Result<Vec<UnreconciledL4Record>, sqlx::Error> {
    let query = r#"
        SELECT 
            symbol, 
            earnings_date, 
            pre_er_stock_price::FLOAT8, 
            implied_move_pct::FLOAT8
        FROM l4_volatility_reality_check
        WHERE symbol = $1 AND post_er_stock_price IS NULL
    "#;

    let rows = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>, f64, f64)>(query)
        .bind(symbol)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| UnreconciledL4Record {
            symbol: r.0,
            earnings_date: r.1,
            pre_er_stock_price: r.2,
            implied_move_pct: r.3,
        })
        .collect())
}

// Update L4 record with post-earnings price and calculated move metrics
pub async fn update_layer4_post_earnings(
    pool: &PgPool,
    symbol: &str,
    earnings_date: chrono::DateTime<chrono::Utc>,
    post_er_price: f64,
    actual_move_pct: f64,
    volatility_ratio: f64,
) -> Result<(), sqlx::Error> {
    let query = r#"
        UPDATE l4_volatility_reality_check
        SET 
            post_er_stock_price = $1,
            actual_move_pct = $2,
            volatility_ratio = $3
        WHERE symbol = $4 AND earnings_date = $5
    "#;

    sqlx::query(query)
        .bind(post_er_price)
        .bind(actual_move_pct)
        .bind(volatility_ratio)
        .bind(symbol)
        .bind(earnings_date)
        .execute(pool)
        .await?;

    Ok(())
}


// Fetch all unique symbols that have unreconciled records
pub async fn get_unreconciled_symbols(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let query = r#"
        SELECT DISTINCT symbol
        FROM l2_earnings_context
        WHERE actual_revenue IS NULL OR eps_surprise_pct IS NULL
    "#;

    sqlx::query_scalar::<_, String>(query)
        .fetch_all(pool)
        .await
}