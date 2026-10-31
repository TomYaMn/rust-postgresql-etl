use crate::db;
use crate::yahoo::YahooClient;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;

pub async fn run_etl_for_ticker(
    db_pool: &PgPool,
    yahoo_client: &YahooClient,
    symbol: &str,
    options_only: bool,
    screener_tag: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::log_info!("--------------------------------------------------");
    crate::log_info!("Starting ETL Pipeline for: {} (Options Only: {})", symbol, options_only);

    // ONLY RUN L1 & L2 IF WE ARE DOING A FULL RUN
    if !options_only {
        // 1. Layer 1 Execution (Fundamentals)
        if let Some(record) = yahoo_client.fetch_layer1_fundamentals(symbol, screener_tag).await? { // <--- Pass tag here            db::insert_layer1_fundamentals(db_pool, &record).await?;
            db::insert_layer1_fundamentals(db_pool, &record).await?;
            crate::log_info!("  [L1] Fundamentals inserted.");
        } else {
            crate::log_info!("  [L1] Skipped (No fundamental data returned).");
        }

        // 2. Layer 2 Execution (Earnings History)
        let earnings_records = yahoo_client.fetch_layer2_earnings(symbol).await?;
        for record in &earnings_records {
            db::insert_layer2_earnings(db_pool, record).await?;
        }
        crate::log_info!("  [L2] Saved {} earnings records.", earnings_records.len());
    } else {
        crate::log_info!("  [L1/L2] Skipped (Options-Only Mode).");
    }

    // ALWAYS RUN L3/L5 & L4
    // 3. Layer 3 & 5 Execution (Options Chains & Greeks)
    let (current_stock_price, greeks_snapshots) = yahoo_client
        .fetch_layer3_5_options_greeks(symbol, 5)
        .await?;

    for snapshot in &greeks_snapshots {
        db::insert_layer3_5_greeks(db_pool, snapshot).await?;
    }
    crate::log_info!("  [L3/L5] Saved {} Option Greek snapshots.", greeks_snapshots.len());

    // 4. Layer 4 Execution (Volatility Reality Check)
    if let Some(first_contract) = greeks_snapshots.first() {
        let straddle_price = first_contract.last_price * 2.0;
        let iv_rank = Some(first_contract.implied_volatility);

        db::calculate_and_insert_layer4(
            db_pool,
            symbol,
            current_stock_price,
            straddle_price,
            iv_rank,
        )
        .await?;
        crate::log_info!("  [L4] Volatility check calculated.");
    }

    crate::log_info!("Completed ETL for {}", symbol);
    Ok(())
}

pub async fn reconcile_all_unreconciled(
    db_pool: &PgPool,
    yahoo_client: &YahooClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::log_info!("--------------------------------------------------");
    crate::log_info!("Starting Global L2 & L4 Reconciliation");

    let l2_symbols = db::get_unreconciled_symbols(db_pool).await?;
    crate::log_info!("  [Reconcile] Found {} symbols pending L2/L4 reconciliation.", l2_symbols.len());

    for symbol in l2_symbols {
        if let Err(e) = reconcile_l2_earnings(db_pool, yahoo_client, &symbol).await {
            crate::log_err!("  ❌ L2 Reconciliation failed for {}: {}", symbol, e);
        }
        
        if let Err(e) = reconcile_l4_volatility(db_pool, yahoo_client, &symbol).await {
            crate::log_err!("  ❌ L4 Reconciliation failed for {}: {}", symbol, e);
        }
        
        sleep(Duration::from_millis(1500)).await;
    }

    crate::log_info!("Global Reconciliation Complete.");
    Ok(())
}

pub async fn reconcile_l2_earnings(
    db_pool: &PgPool,
    yahoo_client: &YahooClient,
    symbol: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::log_info!("Running L2 Earnings Reconciliation for: {}", symbol);

    let records = db::get_unreconciled_l2_records(db_pool, symbol).await?;
    if records.is_empty() {
        crate::log_info!("  [L2 Reconcile] All records up to date.");
        return Ok(());
    }

    let actual_revenues = yahoo_client.fetch_quarterly_actual_revenues(symbol).await?;

    for rec in records {
        // Calculate EPS Surprise % if missing: (actual - est) / |est|
        let calculated_eps_surprise = match (rec.actual_eps, rec.estimated_eps) {
            (Some(act), Some(est)) if est != 0.0 => Some((act - est) / est.abs()),
            _ => None,
        };

        // Match reported revenue from Yahoo
        let matched_revenue = actual_revenues
            .iter()
            .find(|a| rec.market_time.contains(&a.date_str) || a.date_str.contains(&rec.market_time))
            .map(|a| a.actual_revenue);

        // Calculate Revenue Surprise %: (actual - est) / est
        let calculated_rev_surprise = match (matched_revenue, rec.estimated_revenue) {
            (Some(act), Some(est)) if est > 0.0 => Some((act - est) / est),
            _ => None,
        };

        db::update_layer2_actuals(
            db_pool,
            &rec.symbol,
            &rec.market_time,
            matched_revenue,
            calculated_rev_surprise,
            calculated_eps_surprise,
        )
        .await?;

        crate::log_info!(
            "  [L2 Reconcile] Updated {} ({}): Actual Rev = {:?}, Rev Surprise = {:?}, EPS Surprise = {:?}",
            rec.symbol, rec.market_time, matched_revenue, calculated_rev_surprise, calculated_eps_surprise
        );
    }

    Ok(())
}


pub async fn reconcile_l4_volatility(
    db_pool: &PgPool,
    yahoo_client: &YahooClient,
    symbol: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    crate::log_info!("Running L4 Volatility Reconciliation for: {}", symbol);

    let records = db::get_unreconciled_l4_records(db_pool, symbol).await?;
    if records.is_empty() {
        crate::log_info!("  [L4 Reconcile] All records up to date.");
        return Ok(());
    }

    // Fetch latest stock price from Yahoo
    let (current_price, _) = yahoo_client.fetch_layer3_5_options_greeks(symbol, 1).await?;
    if current_price <= 0.0 {
        crate::log_info!("  [L4 Reconcile] Skipped (Could not fetch current stock price).");
        return Ok(());
    }

    for rec in records {
        // Calculate Actual Move %: |Post_Price - Pre_Price| / Pre_Price
        let actual_move_pct = (current_price - rec.pre_er_stock_price).abs() / rec.pre_er_stock_price;

        // Calculate Volatility Ratio: Actual_Move % / Implied_Move %
        let volatility_ratio = if rec.implied_move_pct > 0.0 {
            actual_move_pct / rec.implied_move_pct
        } else {
            0.0
        };

        db::update_layer4_post_earnings(
            db_pool,
            &rec.symbol,
            rec.earnings_date,
            current_price,
            actual_move_pct,
            volatility_ratio,
        )
        .await?;

        crate::log_info!(
            "  [L4 Reconcile] Updated {} (Pre: ${:.2} -> Post: ${:.2}): Actual Move = {:.2}%, Vol Ratio = {:.2}",
            rec.symbol, rec.pre_er_stock_price, current_price, actual_move_pct * 100.0, volatility_ratio
        );
    }

    Ok(())
}