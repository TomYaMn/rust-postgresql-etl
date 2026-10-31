use crate::models::{
    EarningsRecord, FundamentalsRecord, OptionGreekSnapshot, YahooOptionsResponse,
    YahooQuoteSummaryResponse, YahooScreenerResponse, ActualRevenueItem, 
};
use blackscholes::{Greeks, Inputs, OptionType};
use chrono::{DateTime, Utc};
use reqwest::{header, Client};

pub struct YahooClient {
    client: Client,
    crumb: String,
}

impl YahooClient {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );

        let client = Client::builder()
            .cookie_store(true)
            .default_headers(headers)
            .build()?;

        let _ = client.get("https://fc.yahoo.com").send().await?;
        let crumb_res = client
            .get("https://query1.finance.yahoo.com/v1/test/getcrumb")
            .send()
            .await?;
        let crumb = crumb_res.text().await?;

        if crumb.is_empty() || crumb.contains("html") {
            return Err("Failed to obtain a valid Yahoo crumb token.".into());
        }

        Ok(Self { client, crumb })
    }

    pub async fn fetch_screener(&self, scr_id: &str, count: usize) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://query1.finance.yahoo.com/v1/finance/screener/predefined/saved?scrIds={}&count={}&crumb={}",
            scr_id, count, self.crumb
        );

        let res = self.client.get(&url).send().await?;
        if !res.status().is_success() {
            return Ok(vec![]);
        }

        let data: YahooScreenerResponse = res.json().await?;
        let mut symbols = Vec::new();

        if let Some(first_res) = data.finance.result.first() {
            for quote in &first_res.quotes {
                symbols.push(quote.symbol.clone());
            }
        }

        Ok(symbols)
    }

    pub async fn fetch_layer1_fundamentals(&self, symbol: &str) -> Result<Option<FundamentalsRecord>, Box<dyn std::error::Error + Send + Sync>> {
            let url = format!(
                "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=defaultKeyStatistics,financialData&crumb={}",
                symbol, self.crumb
            );

            let response = self.client.get(&url).send().await?;
            if !response.status().is_success() { return Ok(None); }

            let data: YahooQuoteSummaryResponse = response.json().await?;
            if let Some(results) = data.quote_summary.result {
                if let Some(modules) = results.first() {
                    let stats = modules.default_key_statistics.as_ref();
                    let fin = modules.financial_data.as_ref();

                let free_cash_flow = fin.and_then(|f| f.free_cashflow.as_ref()).and_then(|f| f.raw);
                let mut total_revenue = fin.and_then(|f| f.total_revenue.as_ref()).and_then(|r| r.raw);
                
                // FALLBACK: If Yahoo omits totalRevenue, calculate it dynamically
                if total_revenue.is_none() {
                    let rev_per_share = fin.and_then(|f| f.revenue_per_share.as_ref()).and_then(|r| r.raw);
                    let shares = stats.and_then(|s| s.shares_outstanding.as_ref()).and_then(|s| s.raw);
                    
                    if let (Some(rps), Some(shrs)) = (rev_per_share, shares) {
                        total_revenue = Some(rps * shrs);
                    }
                }
                
                // Calculate FCF Margin: (Free Cash Flow / Total Revenue)
                let fcf_margin = match (free_cash_flow, total_revenue) {
                    (Some(fcf), Some(rev)) if rev > 0.0 => Some(fcf / rev),
                    _ => None,
                };

                let eps_growth = fin.and_then(|f| f.earnings_growth.as_ref()).and_then(|g| g.raw);
                let roic = fin.and_then(|f| f.return_on_assets.as_ref()).and_then(|r| r.raw);

                return Ok(Some(FundamentalsRecord {
                    symbol: symbol.to_string(),
                    revenue_growth: fin.and_then(|f| f.revenue_growth.as_ref()).and_then(|r| r.raw),
                    eps_growth,
                    gross_margin: fin.and_then(|f| f.gross_margins.as_ref()).and_then(|m| m.raw),
                    operating_margin: fin.and_then(|f| f.operating_margins.as_ref()).and_then(|m| m.raw),
                    roic,
                    free_cash_flow,
                    fcf_margin,
                    debt_to_equity: fin.and_then(|f| f.debt_to_equity.as_ref()).and_then(|d| d.raw),
                    pe_ratio: stats.and_then(|s| s.forward_p_e.as_ref()).and_then(|p| p.raw),
                    ev_to_ebitda: stats.and_then(|s| s.enterprise_to_ebitda.as_ref()).and_then(|e| e.raw),
                    shares_outstanding: stats.and_then(|s| s.shares_outstanding.as_ref()).and_then(|s| s.raw),
                }));
                }
            }
            Ok(None)
        }

    pub async fn fetch_layer2_earnings(&self, symbol: &str) -> Result<Vec<EarningsRecord>, Box<dyn std::error::Error + Send + Sync>> {
        // Request BOTH earningsHistory and earningsTrend
        let url = format!(
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=earningsHistory,earningsTrend&crumb={}",
            symbol, self.crumb
        );

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() { return Ok(vec![]); }

        let mut records = Vec::new();
        let data: YahooQuoteSummaryResponse = response.json().await?;

        let mut fwd_revenue_guidance = None;
        let mut estimated_revenue = None;
        let mut sentiment = None;

        if let Some(results) = data.quote_summary.result.as_ref() {
            if let Some(modules) = results.first() {
                
                // 1. Extract Revenue and Sentiment from earningsTrend
                if let Some(trend_wrapper) = &modules.earnings_trend {
                    if let Some(trends) = &trend_wrapper.trend {
                        for t in trends {
                            let period = t.period.as_deref().unwrap_or("");
                            
                            if period == "+1q" {
                                fwd_revenue_guidance = t.revenue_estimate.as_ref()
                                    .and_then(|e| e.avg.as_ref())
                                    .and_then(|a| a.raw);
                                
                                let up = t.eps_revisions.as_ref().and_then(|r| r.up_last_30_days.as_ref()).and_then(|v| v.raw).unwrap_or(0.0);
                                let down = t.eps_revisions.as_ref().and_then(|r| r.down_last_30_days.as_ref()).and_then(|v| v.raw).unwrap_or(0.0);
                                sentiment = Some(up - down);
                            }
                            
                            if period == "0q" {
                                estimated_revenue = t.revenue_estimate.as_ref()
                                    .and_then(|e| e.avg.as_ref())
                                    .and_then(|a| a.raw);
                            }
                        }
                    }
                }

                // 2. Build the records from earningsHistory
                if let Some(history_wrapper) = &modules.earnings_history {
                    if let Some(history_items) = &history_wrapper.history {
                        let total_items = history_items.len();
                        
                        for (i, item) in history_items.iter().enumerate() {
                            // Only attach the forward guidance to the most recent historical quarter
                            let is_latest = i == total_items - 1;

                            records.push(EarningsRecord {
                                symbol: symbol.to_string(),
                                market_time: item.quarter.as_ref().and_then(|q| q.fmt.clone()).unwrap_or_else(|| "NQ".to_string()),
                                estimated_eps: item.eps_estimate.as_ref().and_then(|v| v.raw),
                                actual_eps: item.eps_actual.as_ref().and_then(|v| v.raw),
                                eps_surprise_pct: item.eps_surprise_percent.as_ref().and_then(|v| v.raw),
                                estimated_revenue: if is_latest { estimated_revenue } else { None },
                                actual_revenue: None, 
                                revenue_surprise_pct: None,
                                forward_revenue_guidance: if is_latest { fwd_revenue_guidance } else { None },
                                guidance_revision_sentiment: if is_latest { sentiment } else { None },
                            });
                        }
                    }
                }
            }
        }
        Ok(records)
    }

pub async fn fetch_layer3_5_options_greeks(
        &self,
        symbol: &str,
        contract_limit: usize,
    ) -> Result<(f64, Vec<OptionGreekSnapshot>), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://query1.finance.yahoo.com/v7/finance/options/{}?crumb={}",
            symbol, self.crumb
        );

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            return Ok((0.0, vec![]));
        }

        let mut snapshots = Vec::new();
        let raw_data: YahooOptionsResponse = response.json().await?;
        
        if raw_data.option_chain.result.is_empty() {
            return Ok((0.0, vec![]));
        }

        let result_item = &raw_data.option_chain.result[0];
        let current_stock_price = result_item.quote.regular_market_price;
        
        if result_item.options.is_empty() {
            return Ok((current_stock_price, vec![]));
        }

        let options_data = &result_item.options[0];
        let snapshot_time = Utc::now();

        // Helper closure to process both Calls and Puts dynamically
        let mut process_contracts = |contracts: &[crate::models::Contract], opt_type_str: &str, bs_opt_type: OptionType| {
            for contract in contracts.iter().take(contract_limit) {
                let expiration_date = DateTime::from_timestamp(contract.expiration, 0).unwrap_or_else(Utc::now);
                let days_to_expiration = (expiration_date.date_naive() - snapshot_time.date_naive()).num_days() as i32;

                if days_to_expiration <= 0 {
                    continue; 
                }

                let t_years = (days_to_expiration as f32) / 365.25;
                let last_price = contract.last_price.unwrap_or(0.0);
                let bid = contract.bid.unwrap_or(0.0);
                let ask = contract.ask.unwrap_or(0.0);
                let iv = contract.implied_volatility.unwrap_or(0.0);

                let (delta, gamma, theta, vega) = if iv > 0.0 && t_years > 0.0 {
                    let bs_inputs = Inputs::new(
                        bs_opt_type.clone(), // <--- ADD .clone() HERE
                        current_stock_price as f32,
                        contract.strike as f32,
                        Some(last_price as f32),
                        0.045,
                        0.0,
                        t_years,
                        Some(iv as f32),
                    );

                    (
                        bs_inputs.calc_delta().unwrap_or(0.0) as f64,
                        bs_inputs.calc_gamma().unwrap_or(0.0) as f64,
                        (bs_inputs.calc_theta().unwrap_or(0.0) / 365.25) as f64,
                        (bs_inputs.calc_vega().unwrap_or(0.0) / 100.0) as f64,
                    )
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };

                snapshots.push(OptionGreekSnapshot {
                    symbol: symbol.to_string(),
                    contract_symbol: contract.contract_symbol.clone(),
                    expiration_date: expiration_date.date_naive(),
                    days_to_expiration,
                    strike: contract.strike,
                    option_type: opt_type_str.to_string(), // Passes "CALL" or "PUT"
                    bid,
                    ask,
                    last_price,
                    volume: contract.volume,
                    open_interest: contract.open_interest,
                    implied_volatility: iv,
                    delta,
                    gamma,
                    theta,
                    vega,
                });
            }
        };

        // Process both arrays
        process_contracts(&options_data.calls, "CALL", OptionType::Call);
        process_contracts(&options_data.puts, "PUT", OptionType::Put);

        Ok((current_stock_price, snapshots))
    }


pub async fn fetch_quarterly_actual_revenues(
        &self,
        symbol: &str,
    ) -> Result<Vec<ActualRevenueItem>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "https://query1.finance.yahoo.com/v10/finance/quoteSummary/{}?modules=earnings&crumb={}",
            symbol, self.crumb
        );

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() { return Ok(vec![]); }

        let mut actuals = Vec::new();
        let data: YahooQuoteSummaryResponse = response.json().await?;

        if let Some(results) = data.quote_summary.result {
            if let Some(modules) = results.first() {
                if let Some(earnings_mod) = &modules.earnings {
                    if let Some(chart) = &earnings_mod.financials_chart {
                        if let Some(quarters) = &chart.quarterly {
                            for q in quarters {
                                if let (Some(d), Some(rev)) = (&q.date, &q.revenue) {
                                    if let Some(raw_rev) = rev.raw {
                                        actuals.push(ActualRevenueItem {
                                            date_str: d.clone(),
                                            actual_revenue: raw_rev,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(actuals)
    }
}