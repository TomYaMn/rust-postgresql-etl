# Rust ETL to PostgreSQL — Options & Fundamentals Pipeline
A high-performance, asynchronous ETL (Extract, Transform, Load) pipeline built with **Rust**, designed to collect, process, and store financial market data from Yahoo Finance.
The data will be present in reporting format using Apache Superset

The pipeline retrieves stock fundamentals, earnings estimates, historical financial metrics, and options chain data. It also performs financial calculations, including **Black-Scholes option Greeks**, before persisting the processed data into **PostgreSQL / TimescaleDB** for advanced financial analysis, quantitative research, and Apache Superset dashboarding.


# Docker Network Prerequisite

If Rust ETL and Apache Superset are deployed using separate Docker Compose projects, both projects must share a Docker network to communicate with each other. (I have include both docker compose file, refer docker-compose-superset.yml and docker-compose.yml)

Create a shared external Docker network before starting either Compose project.

### 1. Create Shared Docker Network

```bash
docker network create superset-network
```

Verify that the network was created:

```bash
docker network ls
```

### 2. Configure Docker Compose

Add the following network configuration to both Docker Compose files.

**For the Superset `docker-compose.yml`:**

Attach both `superset` and `superset_db` services to the shared network.

**For the Rust ETL `docker-compose.yml`:**

Attach the `rust_etl` service to the same shared network.

Add the following network declaration to the bottom of both Compose files:

```yaml
networks:
  superset-network:
    external: true
```

Each relevant service must also include:

```yaml
networks:
  - superset-network
```

### 3. Configure Database Connection

Containers connected to the same Docker network can communicate using Docker's service DNS names.

For example, if Rust ETL connects to the Superset PostgreSQL database:

```env
DATABASE_URL=postgresql://superset:superset_pass@superset_db:5432/superset
```

**Important:** Replace `localhost` with the appropriate Docker service name when connecting to another container.

If your ETL writes to a separate TimescaleDB instance, configure `DATABASE_URL` using that database's hostname and port instead.

### 4. Deploy Independent Compose Projects

Start the Superset stack:

```bash
cd /u02/docker/superset
docker compose up -d --build
```

Start the Rust ETL stack:

```bash
cd /u02/docker/rust-etl
docker compose up -d
```

### 5. Verify Network Connectivity

Inspect the shared network:

```bash
docker network inspect superset-network
```

Verify that both Compose projects' containers are connected to the network.

Test database hostname resolution from the Rust ETL container:

```bash
docker exec rust_etl_job getent hosts superset_db
```

A successful DNS lookup should return the PostgreSQL container's internal Docker IP address.

**Note:** The external network must exist before deploying either Compose project. It is created once and can be reused across subsequent deployments.


# Rust ETL to PostgreSQL (TimescaleDB extension)
## Architecture

The pipeline is organized into four logical data layers, with Layers 3 and 5 sharing the same options-processing domain.

| Layer | Data Domain | Description | Frequency |
|---|---|---|---|
| L1 | Fundamentals | Financial statements, growth metrics, margins, FCF, P/E, EV/EBITDA | Daily |
| L2 | Earnings Context | Earnings estimates, revenue guidance, reported actuals, earnings surprises | Daily / Post-earnings |
| L3 & L5 | Options Chains & Greeks | Call/Put options, implied volatility, Delta, Gamma, Theta, Vega | Intraday |
| L4 | Volatility Reality Check | Compares implied earnings moves against actual post-earnings price movements | Post-earnings |

### Data Flow

```text
                  Yahoo Finance
                        |
                        v
                 Rust ETL Pipeline
                        |
             +----------+----------+
             |          |          |
             v          v          v
        Fundamentals  Earnings   Options
           (L1)        (L2)      (L3/L5)
             |          |          |
             |          |     Black-Scholes
             |          |       Greeks
             |          |          |
             +----------+----------+
                        |
                        v
                 TimescaleDB
                        |
              +---------+---------+
              |                   |
              v                   v
         SQL Analysis       Apache Superset
              |                   |
              +---------+---------+
                        |
                        v
                 Financial Insights
```

## Core Features

### 1. Financial Fundamentals (L1)
- Quarterly financial statements and company fundamentals.
- Year-over-year revenue and EPS growth.
- Gross profit and operating margins.
- Free cash flow (FCF) and FCF margins.
- Debt and leverage metrics.
- Valuation metrics, including P/E and EV/EBITDA.
- Historical fundamental data for financial trend analysis.

### 2. Earnings Intelligence (L2)
- Forward-looking EPS and revenue estimates.
- Earnings calendar and reporting dates.
- Revenue guidance and earnings expectations.
- Reported earnings actuals.
- Earnings surprises and historical performance reconciliation.

### 3. Options Chains & Greeks (L3 / L5)
- Call and Put options chain collection.
- Intraday options market snapshots.
- Strike prices and expiration dates.
- Implied volatility (IV).
- Black-Scholes Greeks calculations:
  - Delta
  - Gamma
  - Theta
  - Vega
- Options data collection for earnings-related stocks and market scanners.

### 4. Volatility Reality Check (L4)
Evaluates whether the options market accurately priced expected earnings movements.

- Calculates implied earnings moves using ATM straddles.
- Tracks actual underlying stock price movements after earnings announcements.
- Compares implied volatility against realized volatility.
- Calculates earnings-related volatility ratios.
- Tracks post-earnings IV crush.

## Data Storage

The pipeline uses **PostgreSQL with TimescaleDB** to store financial data.

Database operations use `ON CONFLICT DO UPDATE` to support idempotent ingestion and prevent duplicate records.

### Database Schema

| Schema | Responsibility |
|---|---|
| `l1` | Financial fundamentals |
| `l2` | Earnings estimates and actuals |
| `l3_l5` | Options chains and calculated Greeks |
| `l4` | Implied vs. realized earnings volatility |

Time-series tables are designed to use TimescaleDB hypertables where appropriate.

The database supports historical analysis, intraday snapshots, and financial performance comparisons.

## Scheduling & Automation

The pipeline supports both manual execution and automated daemon scheduling.

Key scheduling features:

- **Smart Scheduling:** Separates computationally intensive fundamental collection from lightweight intraday options collection.
- **Market-Hours Awareness:** Schedules options collection around US market hours.
- **Dynamic Session Management:** Retrieves and maintains Yahoo Finance session cookies and crumb tokens.
- **Post-Earnings Reconciliation:** Automatically retrieves reported earnings actuals and updates earnings-surprise and volatility metrics.
- **Timezone Awareness:** Uses `America/New_York` to handle US market hours and daylight saving time automatically.

### Scheduling Strategy

| Job | Schedule | Purpose |
|---|---|---|
| Fundamentals | Daily | Collect company financial metrics |
| Earnings Context | Daily | Update estimates and earnings information |
| Options Sweeps | Hourly during market hours | Capture options chains and Greeks |
| Earnings Reconciliation | Nightly at 2:00 AM | Retrieve actual earnings and calculate post-earnings metrics |

---

# Setup & Installation

## 1. Environment Variables

Create a `.env` file in the project root directory.

For local development, you can alternatively use `.env.dev`.

```env
DATABASE_URL=postgresql://superset:superset_pass@localhost:5432/superset
ENABLE_SCHEDULER=false
```

| Variable | Description |
|---|---|
| `DATABASE_URL` | PostgreSQL / TimescaleDB connection string |
| `ENABLE_SCHEDULER` | Enables or disables automated scheduling |

Set `ENABLE_SCHEDULER=false` for manual CLI execution and `true` for daemon mode.

## 2. Database Preparation

Ensure PostgreSQL is running and the TimescaleDB extension is installed and enabled.

The pipeline expects the following database schemas:

```sql
CREATE SCHEMA IF NOT EXISTS l1;
CREATE SCHEMA IF NOT EXISTS l2;
CREATE SCHEMA IF NOT EXISTS l3_l5;
CREATE SCHEMA IF NOT EXISTS l4;
```

Ensure the required tables, indexes, unique constraints, and hypertables are initialized before running the ETL pipeline.

---

# Manual CLI Usage

Set:

```env
ENABLE_SCHEDULER=false
```

The pipeline can then be executed manually using Cargo.

## 1. Full Data Sweeps (L1–L5)

Fetches fundamentals, earnings context, options chains, and calculated Greeks.

```bash
cargo run -- --sp500           # Default S&P 500 list
cargo run -- --gainers         # Yahoo Top Gainers
cargo run -- --losers          # Yahoo Top Losers
cargo run -- --active          # Yahoo Most Active
cargo run -- --volatile        # Yahoo Most Volatile
cargo run -- --unusual-volume  # Yahoo Unusual Volume
cargo run -- --highs           # New 52-Week Highs
cargo run -- --earnings        # Companies reporting earnings today
```

## 2. Options-Only Sweeps (L3–L5)

Append `--options-only` to skip fundamental and earnings collection, focusing exclusively on options data.

```bash
cargo run -- --volatile --options-only
cargo run -- --earnings --options-only
```

This is particularly useful for frequent intraday collection without repeatedly fetching relatively static financial fundamentals.

## 3. Custom Tickers & Watchlists

Execute the pipeline against specific stocks.

```bash
cargo run -- --custom AAPL
cargo run -- --custom AAPL,MSFT,NVDA
cargo run -- --custom TSLA --options-only
```

## 4. Custom JSON Watchlist

You can also provide a custom watchlist using a JSON file.

Example: `my_portfolio.json`

```json
[
    "AAPL",
    "MSFT",
    "NVDA"
]
```

Execute the pipeline:

```bash
cargo run -- --file my_portfolio.json
cargo run -- --file my_portfolio.json --options-only
```

## 5. Post-Earnings Reconciliation

Reconciliation jobs retrieve missing reported earnings actuals and update post-earnings metrics.

```bash
cargo run -- --reconcile AAPL     # Reconcile a single stock
cargo run -- --reconcile-all      # Reconcile all pending stocks
```

---

# Docker Deployment & Daemon Scheduling

The pipeline supports continuous background execution through Docker.

The scheduler configuration is defined in `config.toml`.

## 1. Enable Scheduler

Set the following environment variable in `.env`:

```env
ENABLE_SCHEDULER=true
```

## 2. Build and Start

Deploy the pipeline using Docker Compose:

```bash
docker compose up -d --build
```

## 3. Monitor Logs

Check the ETL container logs:

```bash
docker compose logs -f
```

Check running containers:

```bash
docker ps
```

## 4. Scheduler Configuration

The `config.toml` file controls automated execution schedules.

The scheduler is optimized for US market hours using the `America/New_York` timezone.

Recommended scheduling strategy:

- **Options Sweeps:** Hourly, staggered between 10:00 AM and 4:00 PM.
- **Full Fundamentals Sweeps:** Once daily, starting at 4:30 PM after market close.
- **Earnings Reconciliation:** Nightly at 2:00 AM.

The scheduler automatically accounts for US daylight saving time through its configured timezone.

---

# Technology Stack

| Technology | Purpose |
|---|---|
| Rust | Asynchronous ETL processing |
| PostgreSQL | Relational database |
| TimescaleDB | Time-series storage and querying |
| Apache Superset | Financial dashboards and visualization |
| Docker | Containerization and deployment |
| Yahoo Finance | Financial market data source |
| Black-Scholes Model | Options pricing and Greeks calculations |

## Project Goals

The primary objective is to build a personal financial data warehouse that consolidates company fundamentals, earnings intelligence, and options market information into a unified analytical platform.

By combining Rust's performance and asynchronous processing capabilities with TimescaleDB's time-series storage and Apache Superset's visualization capabilities, the project provides a foundation for:

- Historical financial analysis.
- Earnings performance tracking.
- Options market monitoring.
- Implied vs. realized volatility analysis.
- Quantitative research and strategy development.
- Custom financial dashboards and automated market screening.

---

**Disclaimer:** This project is intended for educational, research, and personal analytical purposes. Yahoo Finance data availability, accuracy, access restrictions, and permitted usage are subject to Yahoo's applicable terms and data-provider licensing conditions. Options Greeks and volatility calculations are model-derived estimates and should not be treated as guaranteed market values or investment advice.