use crate::yahoo::YahooClient;
use std::fs;
use chrono::Utc; 

#[derive(Clone, Debug)]
pub enum ScreenerMode {
    Sp500Tech,
    TopGainers,
    TopLosers,
    MostActive,
    MostVolatile,
    UnusualVolume,
    New52WeekHigh,
    EarningsToday,
    Custom(Vec<String>),
    CustomFile(String), // Add this new mode for custom JSON files
}

// ---> THIS IS THE NEW BLOCK WE NEED TO ADD <---
impl ScreenerMode {
    pub fn as_tag(&self) -> String {
        // Get today's date in YYYY-MM-DD format
        let today = Utc::now().format("%Y-%m-%d").to_string();

        let base_tag = match self {
            ScreenerMode::Sp500Tech => "SP500_TECH",
            ScreenerMode::TopGainers => "TOP_GAINERS",
            ScreenerMode::TopLosers => "TOP_LOSERS",
            ScreenerMode::MostActive => "MOST_ACTIVE",
            ScreenerMode::MostVolatile => "MOST_VOLATILE",
            ScreenerMode::UnusualVolume => "UNUSUAL_VOLUME",
            ScreenerMode::New52WeekHigh => "52_WEEK_HIGH",
            ScreenerMode::EarningsToday => "EARNINGS_TODAY",
            ScreenerMode::Custom(_) => "CUSTOM_CLI",
            ScreenerMode::CustomFile(_) => "CUSTOM_FILE",
        };

        // Output example: "TOP_GAINERS_2026-10-31"
        format!("{}_{}", base_tag, today)
    }
}
// -----------------------------------------------

pub struct Screener;

impl Screener {
    pub async fn resolve_tickers(
        client: &YahooClient,
        mode: ScreenerMode,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        match mode {
            ScreenerMode::Sp500Tech => {
                crate::log_info!("Loading predefined S&P 500 Tech json file");
                let content = fs::read_to_string("sp500_tech.json")?;
                let tickers: Vec<String> = serde_json::from_str(&content)?;
                Ok(tickers)
            }
            ScreenerMode::CustomFile(filepath) => {
                crate::log_info!("Loading custom JSON file: {}", filepath);
                let content = fs::read_to_string(&filepath)
                    .map_err(|e| format!("CRITICAL ERROR: Failed to read JSON file '{}' - {}", filepath, e))?;
                let tickers: Vec<String> = serde_json::from_str(&content)
                    .map_err(|e| format!("CRITICAL ERROR: Failed to parse JSON in '{}' - {}", filepath, e))?;
                Ok(tickers)
            }
            ScreenerMode::TopGainers => {
                crate::log_info!("Querying Yahoo Screener for Top Gainers...");
                Ok(client.fetch_screener("day_gainers", 25).await?)
            }
            ScreenerMode::TopLosers => {
                crate::log_info!("Querying Yahoo Screener for Top Losers...");
                Ok(client.fetch_screener("day_losers", 25).await?)
            }
            ScreenerMode::MostActive => {
                crate::log_info!("Querying Yahoo Screener for Most Active Volume...");
                Ok(client.fetch_screener("most_actives", 25).await?)
            }
            ScreenerMode::MostVolatile => {
                crate::log_info!("Querying Yahoo Screener for Most Volatile...");
                Ok(client.fetch_screener("aggressive_small_caps", 25).await?)
            }
            ScreenerMode::UnusualVolume => {
                crate::log_info!("Querying Yahoo Screener for Unusual Volume...");
                Ok(client.fetch_screener("unusual_volume", 25).await?)
            }
            ScreenerMode::New52WeekHigh => {
                crate::log_info!("Querying Yahoo Screener for New 52-Week Highs...");
                Ok(client.fetch_screener("52_week_gainers", 25).await?)
            }
            ScreenerMode::EarningsToday => {
                crate::log_info!("Querying Earnings Today...");
                Ok(client.fetch_screener("earnings_today", 25).await?)
            }
            ScreenerMode::Custom(list) => {
                crate::log_info!("Loading Custom Watchlist...");
                Ok(list)
            }
        }
    }
}