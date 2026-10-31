#![allow(dead_code)]

use serde::Deserialize;

// --- Yahoo Screener API Models ---
#[derive(Deserialize, Debug)]
pub struct YahooScreenerResponse {
    pub finance: ScreenerFinance,
}

#[derive(Deserialize, Debug)]
pub struct ScreenerFinance {
    pub result: Vec<ScreenerResult>,
}

#[derive(Deserialize, Debug)]
pub struct ScreenerResult {
    pub quotes: Vec<ScreenerQuote>,
}

#[derive(Deserialize, Debug)]
pub struct ScreenerQuote {
    pub symbol: String,
}

// --- Yahoo Summary API Models ---
#[derive(Deserialize, Debug)]
pub struct YahooQuoteSummaryResponse {
    #[serde(rename = "quoteSummary")]
    pub quote_summary: QuoteSummaryResult,
}

#[derive(Deserialize, Debug)]
pub struct QuoteSummaryResult {
    pub result: Option<Vec<SummaryModules>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SummaryModules {
    pub default_key_statistics: Option<DefaultKeyStatistics>,
    pub financial_data: Option<FinancialData>,
    pub earnings_history: Option<EarningsHistoryWrapper>,
    pub earnings_trend: Option<EarningsTrendWrapper>,
    pub earnings: Option<YahooEarningsModule>, // <--- ADD THIS
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DefaultKeyStatistics {
    pub forward_p_e: Option<F64Value>,
    pub enterprise_to_ebitda: Option<F64Value>,
    pub shares_outstanding: Option<F64Value>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FinancialData {
    pub gross_margins: Option<F64Value>,
    pub operating_margins: Option<F64Value>,
    pub free_cashflow: Option<F64Value>,
    pub debt_to_equity: Option<F64Value>,
    pub revenue_growth: Option<F64Value>,
    pub earnings_growth: Option<F64Value>,     // <--- ADD THIS (eps_growth_yoy)
    pub return_on_assets: Option<F64Value>,    // <--- ADD THIS (used for roic estimate)
    pub total_revenue: Option<F64Value>,
    pub revenue_per_share: Option<F64Value>,
}

#[derive(Deserialize, Debug)]
pub struct EarningsHistoryWrapper {
    pub history: Option<Vec<EarningsHistoryItem>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EarningsHistoryItem {
    pub max_age: Option<i64>,
    pub eps_actual: Option<F64Value>,
    pub eps_estimate: Option<F64Value>,
    pub eps_surprise_percent: Option<F64Value>,
    pub quarter: Option<QuarterValue>,
}

#[derive(Deserialize, Debug)]
pub struct QuarterValue {
    pub fmt: Option<String>,
}


#[derive(Deserialize, Debug)]
pub struct EarningsTrendWrapper {
    pub trend: Option<Vec<EarningsTrendItem>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EarningsTrendItem {
    pub period: Option<String>,
    pub revenue_estimate: Option<EstimateData>,
    pub eps_revisions: Option<EpsRevisions>,
}

#[derive(Deserialize, Debug)]
pub struct EstimateData {
    pub avg: Option<F64Value>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EpsRevisions {
    #[serde(rename = "upLast30days")]
    pub up_last_30_days: Option<F64Value>,
    #[serde(rename = "downLast30days")]
    pub down_last_30_days: Option<F64Value>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct YahooEarningsModule {
    pub financials_chart: Option<FinancialsChart>,
}

#[derive(Deserialize, Debug)]
pub struct FinancialsChart {
    pub quarterly: Option<Vec<QuarterlyFinancialItem>>,
}

#[derive(Deserialize, Debug)]
pub struct QuarterlyFinancialItem {
    pub date: Option<String>,      // e.g., "2Q2026" or "2026-07-31"
    pub revenue: Option<F64Value>, // Actual quarterly revenue
    pub earnings: Option<F64Value>,// Actual quarterly net income
}

// Data container for actual revenue lookup
pub struct ActualRevenueItem {
    pub date_str: String,
    pub actual_revenue: f64,
}


#[derive(Deserialize, Debug)]
pub struct F64Value {
    pub raw: Option<f64>,
    pub fmt: Option<String>,
}


// --- Yahoo Options API Models ---
#[derive(Deserialize, Debug)]
pub struct YahooOptionsResponse {
    #[serde(rename = "optionChain")]
    pub option_chain: OptionChain,
}

#[derive(Deserialize, Debug)]
pub struct OptionChain {
    pub result: Vec<ResultItem>,
}

#[derive(Deserialize, Debug)]
pub struct ResultItem {
    pub quote: Quote,
    pub options: Vec<OptionData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub regular_market_price: f64,
}

#[derive(Deserialize, Debug)]
pub struct OptionData {
    pub calls: Vec<Contract>,
    pub puts: Vec<Contract>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub contract_symbol: String,
    pub strike: f64,
    pub last_price: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub volume: Option<i32>,
    pub open_interest: Option<i32>,
    pub implied_volatility: Option<f64>,
    pub expiration: i64,
}

// --- Internal Domain Containers ---
pub struct FundamentalsRecord {
    pub symbol: String,
    pub screener_tag: String,
    pub revenue_growth: Option<f64>,
    pub eps_growth: Option<f64>,               // <--- ADD THIS
    pub gross_margin: Option<f64>,
    pub operating_margin: Option<f64>,
    pub roic: Option<f64>,                     // <--- ADD THIS
    pub free_cash_flow: Option<f64>,
    pub fcf_margin: Option<f64>,
    pub debt_to_equity: Option<f64>,
    pub pe_ratio: Option<f64>,
    pub ev_to_ebitda: Option<f64>,
    pub shares_outstanding: Option<f64>,
}

pub struct EarningsRecord {
    pub symbol: String,
    pub market_time: String,
    pub estimated_eps: Option<f64>,
    pub actual_eps: Option<f64>,
    pub eps_surprise_pct: Option<f64>,
    pub estimated_revenue: Option<f64>,
    pub actual_revenue: Option<f64>, // Requires deep income statement parsing (stays None for now)
    pub revenue_surprise_pct: Option<f64>, 
    pub forward_revenue_guidance: Option<f64>,
    pub guidance_revision_sentiment: Option<f64>,
}

pub struct OptionGreekSnapshot {
    pub symbol: String,
    pub contract_symbol: String,
    pub expiration_date: chrono::NaiveDate,
    pub days_to_expiration: i32,
    pub strike: f64,
    pub option_type: String,
    pub bid: f64,
    pub ask: f64,
    pub last_price: f64,
    pub volume: Option<i32>,
    pub open_interest: Option<i32>,
    pub implied_volatility: f64,
    pub delta: f64,
    pub gamma: f64,
    pub theta: f64,
    pub vega: f64,
}