mod db;
mod models;
mod pipeline;
mod screener;
mod yahoo;

// ==========================================
// CUSTOM LOGGING MACROS WITH TIMESTAMPS
// ==========================================
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        // This MUST be println!, not crate::log_info!
        println!("[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        // This MUST be eprintln!, not crate::log_err!
        eprintln!("[{}] ❌ ERROR: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format_args!($($arg)*));
    };
}
// ==========================================

use config::{Config, File};
use serde::Deserialize;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_cron_scheduler::{Job, JobScheduler};
use screener::{Screener, ScreenerMode};
use yahoo::YahooClient;
use sqlx::PgPool;

#[derive(Debug, Deserialize)]
struct AppConfig {
    jobs: Vec<JobConfig>,
}

#[derive(Debug, Deserialize)]
struct JobConfig {
    name: String,
    cron: String,
    mode: String,
}

// Helper to convert string to ScreenerMode
fn parse_mode(mode_str: &str) -> ScreenerMode {
    let mode_str = mode_str.trim();

    if mode_str.starts_with("--file") {
        let filename = mode_str.trim_start_matches("--file").trim().to_string();
        return ScreenerMode::CustomFile(filename);
    }

    if mode_str.starts_with("--custom") {
        let tickers: Vec<String> = mode_str
            .trim_start_matches("--custom")
            .split(|c| c == ',' || c == ' ')
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty())
            .collect();

        return ScreenerMode::Custom(tickers);
    }

    if mode_str.starts_with("--reconcile") {
        let ticker = mode_str.trim_start_matches("--reconcile").trim().to_uppercase();
        return ScreenerMode::Custom(vec![ticker]);
    }

    match mode_str {
        "--gainers" => ScreenerMode::TopGainers,
        "--losers" => ScreenerMode::TopLosers,
        "--active" => ScreenerMode::MostActive,
        "--volatile" => ScreenerMode::MostVolatile,
        "--unusual-volume" => ScreenerMode::UnusualVolume,
        "--highs" => ScreenerMode::New52WeekHigh,
        "--earnings" => ScreenerMode::EarningsToday,
        "--sp500" => ScreenerMode::Sp500Tech,
        _ => ScreenerMode::Sp500Tech,
    }
}

// Extracted reusable job execution logic
async fn execute_job(
    job_name: &str,
    mode: ScreenerMode,
    db_pool: Arc<PgPool>,
    yahoo_client: Arc<YahooClient>,
    options_only: bool, 
) {
    crate::log_info!("Starting Job: {}", job_name);
    let tag = mode.as_tag(); 
    
    let tickers = match Screener::resolve_tickers(&yahoo_client, mode.clone()).await {        
        Ok(t) => t,
        Err(e) => {
            crate::log_err!("Failed to resolve tickers for {}: {}", job_name, e);
            return;
        }
    };

    for symbol in tickers {
        // Pass the tag string down to the pipeline
        if let Err(e) = pipeline::run_etl_for_ticker(&db_pool, &yahoo_client, &symbol, options_only, &tag).await {
            crate::log_err!("  ❌ ETL failed for {}: {}", symbol, e);
        }
        sleep(Duration::from_millis(1500)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv::from_filename(".env.dev").ok();
    
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db_pool = Arc::new(db::init_db_pool(&db_url).await?);
    let yahoo_client = Arc::new(YahooClient::new().await?);

    let enable_scheduler = env::var("ENABLE_SCHEDULER").unwrap_or_else(|_| "false".to_string()) == "true";

    if enable_scheduler {
        crate::log_info!("ENABLE_SCHEDULER=true. Starting in daemon mode...");
        
        let settings = Config::builder()
            .add_source(File::with_name("config.toml"))
            .build()?;
        let app_config: AppConfig = settings.try_deserialize()?;

        let sched = JobScheduler::new().await?;

        for job_cfg in app_config.jobs {
            let db_pool = Arc::clone(&db_pool);
            let yahoo_client = Arc::clone(&yahoo_client);
            let job_name = job_cfg.name.clone();
            let mode_str = job_cfg.mode.clone();
            
            let job = Job::new_async(job_cfg.cron.as_str(), move |_uuid, mut _l| {
                let db_pool = Arc::clone(&db_pool);
                let yahoo_client = Arc::clone(&yahoo_client);
                let job_name = job_name.clone();
                let mode_str = mode_str.clone();

                Box::pin(async move {
                    crate::log_info!("Starting Scheduled Job: {}", job_name);
                    
                    // Inside the scheduled job closure:
                    let is_options_only = mode_str.contains("--options-only");
                    let clean_mode_str = mode_str.replace("--options-only", "").trim().to_string();
                    let mode = parse_mode(&clean_mode_str);

                    // Route the job based on the mode string
                    if mode_str == "--reconcile-all" {
                        let _ = pipeline::reconcile_all_unreconciled(&db_pool, &yahoo_client).await;
                    } else if mode_str.starts_with("--reconcile") {
                        let symbol = mode_str.trim_start_matches("--reconcile").trim();
                        let _ = pipeline::reconcile_l2_earnings(&db_pool, &yahoo_client, symbol).await;
                    } else {
                        execute_job(&job_name, mode, db_pool, yahoo_client, is_options_only).await;                    
                    }
                })
            })?;

            sched.add(job).await?;
            crate::log_info!("Scheduled '{}' with cron: {}", job_cfg.name, job_cfg.cron);
        }

        sched.start().await?;
        crate::log_info!("Scheduler started. Application is running in daemon mode.");
        
        tokio::signal::ctrl_c().await?;
        crate::log_info!("Shutdown signal received. Exiting daemon...");

    } else {
        // MANUAL RUN MODE via CLI args
        let args: Vec<String> = env::args().skip(1).collect();
        let target_mode = if args.is_empty() {
            "--sp500".to_string()
        } else {
            args.join(" ")
        };

        if target_mode == "--reconcile-all" {
            crate::log_info!("ENABLE_SCHEDULER=false. Running global reconciliation...");
            if let Err(e) = pipeline::reconcile_all_unreconciled(&db_pool, &yahoo_client).await {
                crate::log_err!("Global reconciliation failed: {}", e);
            }
        } else if target_mode.starts_with("--reconcile") {
            let symbol = target_mode.trim_start_matches("--reconcile").trim();
            crate::log_info!("ENABLE_SCHEDULER=false. Running single reconciliation for {}...", symbol);
            if let Err(e) = pipeline::reconcile_l2_earnings(&db_pool, &yahoo_client, symbol).await {
                crate::log_err!("Reconciliation failed: {}", e);
            }
        } else {
            // FIXED MANUAL RUN MODE
            crate::log_info!("ENABLE_SCHEDULER=false. Running in manual CLI mode...");
            
            // Extract the flag for manual runs
            let is_options_only = target_mode.contains("--options-only");
            let clean_mode_str = target_mode.replace("--options-only", "").trim().to_string();
            
            let mode = parse_mode(&clean_mode_str);
            let job_name = format!("Manual Run ({})", target_mode);
            
            // Pass the flag here
            execute_job(&job_name, mode, db_pool, yahoo_client, is_options_only).await;
        }
    }

    Ok(())
}