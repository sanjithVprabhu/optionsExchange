# CONFIG VALIDATION RULES

## Purpose
This document defines ALL validation rules that must be enforced when loading the master configuration file. Any config that violates these rules should be REJECTED before the exchange starts.

---

## CRITICAL VALIDATIONS (Must Pass)

### 1. Exchange Metadata
- ✅ `exchange.name` must not be empty
- ✅ `exchange.mode` must be one of: "production", "virtual", "both"

### 2. Instrument Layer

#### 2.1 Assets
- ✅ At least ONE asset must be enabled (`enabled: true`)
- ✅ Each asset must have:
  - `symbol` (non-empty string)
  - `decimals` (0-18)
  - `contract_size` > 0
  - `min_order_size` >= 1
  - `tick_size` > 0
  - `price_decimals` (0-8)

#### 2.2 Settlement Currencies
- ✅ At least ONE settlement currency must be enabled
- ✅ Exactly ONE settlement currency must have `primary: true`
- ✅ Each settlement currency must have at least ONE enabled chain
- ✅ Chain IDs must be valid (1=Ethereum, 137=Polygon, etc.)
- ✅ Contract addresses must be valid hex (0x...)

#### 2.3 Market Data
- ✅ `primary_provider` must match one of the enabled providers
- ✅ At least ONE provider must be enabled
- ✅ For each enabled asset, corresponding stream must exist in primary provider
- ✅ `fallback_strategy` must be one of: "median", "average", "last_valid", "fail"
- ✅ `max_price_age_seconds` must be > 0

#### 2.4 Expiry Schedule
- ✅ At least ONE expiry type must be enabled
- ✅ If daily enabled: `count` must be 1-30
- ✅ If weekly enabled: `count` must be 1-12, `day_of_week` must be valid
- ✅ If monthly enabled: `count` must be 1-12
- ✅ All expiry times must be valid UTC time format (HH:MM)

#### 2.5 Storage
- ✅ `type` must be one of: "postgres", "supabase", "mysql", "cockroachdb", "inmemory"
- ✅ If type=postgres/mysql: `host`, `database`, `user`, `password` must not be empty
- ✅ If type=supabase: `url` and `anon_key` must not be empty
- ✅ `max_connections` must be 1-1000

### 3. OMS

#### 3.1 Order Types
- ✅ At least ONE order type must be enabled

#### 3.2 Time-in-Force
- ✅ At least ONE TIF option must be enabled

#### 3.3 Limits
- ✅ `max_open_orders_per_user` must be 1-10000
- ✅ `max_order_size_contracts` must be >= `min_order_size_contracts`
- ✅ `min_order_size_contracts` must be >= 1
- ✅ `max_price_deviation_percent` must be 0-100

#### 3.4 Order Book
- ✅ `depth_levels` must be 1-100
- ✅ `update_frequency_ms` must be 10-10000

### 4. Matching Engine

#### 4.1 Algorithm
- ✅ `algorithm` must be "price_time_priority"

#### 4.2 Performance
- ✅ `matching_frequency_ms` must be 1-1000
- ✅ `batch_size` must be 1-10000

#### 4.3 Storage
- ✅ If orderbook_store.type=redis: `host` must not be empty

#### 4.4 Circuit Breakers
- ✅ If price_movement enabled: `percent_threshold` must be 1-50
- ✅ If price_movement enabled: `time_window_seconds` must be 10-3600
- ✅ If liquidity enabled: `min_bid_ask_orders` must be 1-100
- ✅ If liquidity enabled: `max_spread_percent` must be 0.1-100

### 5. Risk Engine

#### 5.1 Margin Requirements
- ✅ For each enabled asset, initial_margin must exist
- ✅ For each enabled asset, maintenance_margin must exist
- ✅ `initial_margin` must be > `maintenance_margin`
- ✅ Margin values must be 0.01-1.0 (1%-100%)

#### 5.2 Liquidation
- ✅ `threshold` must be 0.5-1.0 (50%-100% of maintenance margin)
- ✅ `check_frequency_seconds` must be 1-60

#### 5.3 Position Limits
- ✅ All position limits must be > 0

#### 5.4 Greeks
- ✅ If enabled: `calculation_frequency_seconds` must be 1-60
- ✅ `risk_free_rate` must be 0-1.0 (0%-100%)
- ✅ `volatility.type` must be one of: "implied", "historical", "manual"
- ✅ If type=historical: `historical_days` must be 1-365
- ✅ If type=manual: volatility values must exist for ALL enabled assets

### 6. Settlement

#### 6.1 Timing
- ✅ `trade_settlement_delay_seconds` must be 0-60
- ✅ `expiry_settlement_delay_seconds` must be 0-300

#### 6.2 Method
- ✅ `method` must be "cash_settled"

#### 6.3 Mark Price
- ✅ `calculation_method` must be one of: "spot", "index", "twap"
- ✅ If method=twap: `window_seconds` must be 60-3600
- ✅ If method=index: source weights must sum to 1.0

#### 6.4 Blockchain
- ✅ If enabled: `primary_chain` must match one of the enabled chains in settlement_currencies
- ✅ If wallet.type=multisig: threshold must be valid (e.g., "2/3")
- ✅ `gas.max_gas_price_gwei` must be 1-1000
- ✅ `confirmations.deposits` must be 1-100
- ✅ `confirmations.withdrawals` must be 1-100

### 7. Wallet

#### 7.1 Hot Wallet
- ✅ If enabled: `max_balance_usdt` must be > 0
- ✅ `auto_sweep_threshold` must be < `max_balance_usdt`

#### 7.2 Deposits
- ✅ `min_deposit_usdt` must be > 0
- ✅ `max_deposit_usdt` must be > `min_deposit_usdt`

#### 7.3 Withdrawals
- ✅ `min_withdrawal_usdt` must be > 0
- ✅ `max_withdrawal_usdt` must be >= `min_withdrawal_usdt`
- ✅ `limits.daily_limit_usdt` must be > 0
- ✅ `limits.monthly_limit_usdt` must be >= `limits.daily_limit_usdt`
- ✅ `approval.auto_approve_under_usdt` must be <= `approval.manual_review_over_usdt`

#### 7.4 Fees
- ✅ If type=flat: `flat_fee_usdt` must be > 0
- ✅ `min_fee_usdt` must be <= `max_fee_usdt`

#### 7.5 Collateral
- ✅ `ratios.over_collateralization` must be >= 1.0

#### 7.6 Storage
- ✅ If type=postgres: `isolation_level` must be "serializable" (critical!)

### 8. Market Data

#### 8.1 Feeds
- ✅ At least ONE feed must be enabled
- ✅ `orderbook.depth_levels` must be 1-100
- ✅ `orderbook.update_frequency_ms` must be 10-10000
- ✅ All update frequencies must be > 0

#### 8.2 Channels
- ✅ At least ONE distribution channel must be enabled
- ✅ If websocket enabled: `max_connections` must be 100-100000

### 9. Service Discovery

#### 9.1 Registry
- ✅ `type` must be one of: "static", "consul", "etcd", "kubernetes"
- ✅ If type=static: ALL service hosts/ports must be defined
- ✅ Port numbers must be 1-65535
- ✅ Port numbers must be unique across services (unless hosts differ)

#### 9.2 Communication
- ✅ `default_protocol` must be one of: "grpc", "http", "websocket"
- ✅ If grpc.tls_enabled=true: cert_path and key_path must exist
- ✅ If http.tls_enabled=true: cert_path and key_path must exist

#### 9.3 Authentication
- ✅ `type` must be one of: "bearer_token", "mutual_tls", "api_key"
- ✅ If type=bearer_token: ALL service tokens must be non-empty

#### 9.4 Routing
- ✅ All referenced services must exist in service registry
- ✅ All endpoints must start with "/"
- ✅ Timeout values must be > 0
- ✅ Retry max_attempts must be 0-10

### 10. External APIs

#### 10.1 REST API
- ✅ If enabled: port must be 1-65535
- ✅ If tls_enabled: cert_path and key_path must be non-empty
- ✅ `rate_limit.authenticated.requests_per_minute` must be > 0
- ✅ `rate_limit.anonymous.requests_per_minute` must be > 0

#### 10.2 WebSocket API
- ✅ If enabled: port must be 1-65535
- ✅ Port must be different from REST API port
- ✅ `max_connections` must be 1-1000000
- ✅ `ping_interval_seconds` must be 10-300
- ✅ `pong_timeout_seconds` must be > 0 and < `ping_interval_seconds`

#### 10.3 Authentication
- ✅ If type=jwt: `secret` must be at least 32 characters
- ✅ If type=jwt: `expiry_seconds` must be 300-86400 (5min-24hr)

### 11. Virtual Trading

#### 11.1 Configuration
- ✅ `port_offset` must be 1-10000
- ✅ Port conflicts must not occur: (real_port + offset) must be unique
- ✅ All virtual databases must have different names from production

#### 11.2 Virtual Users
- ✅ `initial_balance_usdt` must be > 0

### 12. Fees

#### 12.1 Trading Fees
- ✅ `maker_fee_bps` must be 0-1000 (0%-10%)
- ✅ `taker_fee_bps` must be >= `maker_fee_bps`
- ✅ If volume_tiers enabled: tiers must be sorted by volume ascending
- ✅ If volume_tiers enabled: fees must decrease with volume

#### 12.2 Settlement Fees
- ✅ If type=flat: `flat_fee_usdt` must be > 0

### 13. Compliance

#### 13.1 KYC
- ✅ If enabled: `provider` must be one of: "sumsub", "onfido", "jumio"
- ✅ If enabled: provider credentials must not be empty
- ✅ Tier limits must be in ascending order

#### 13.2 Geo Restrictions
- ✅ `blocked_countries` must be valid ISO 3166-1 alpha-2 codes

#### 13.3 Audit
- ✅ If enabled: `retention_years` must be 1-10

### 14. Monitoring

#### 14.1 Metrics
- ✅ `provider` must be one of: "prometheus", "datadog", "cloudwatch"
- ✅ If prometheus: port must be 1-65535

#### 14.2 Logging
- ✅ `level` must be one of: "debug", "info", "warn", "error"
- ✅ `format` must be one of: "json", "text"
- ✅ If file output enabled: `path` must not be empty

#### 14.3 Tracing
- ✅ If enabled: `provider` must be one of: "jaeger", "zipkin", "datadog"
- ✅ `sample_rate` must be 0.0-1.0

### 15. Security

#### 15.1 Encryption
- ✅ `at_rest.algorithm` must be one of: "AES-256-GCM", "AES-256-CBC"
- ✅ `at_rest.key_rotation_days` must be 1-365
- ✅ `in_transit.tls_version` must be "1.2" or "1.3"

#### 15.2 Secrets
- ✅ `provider` must be one of: "aws_secrets_manager", "hashicorp_vault", "env"

### 16. Deployment

#### 16.1 Environment
- ✅ `environment` must be one of: "development", "staging", "production"

#### 16.2 Infrastructure
- ✅ `type` must be one of: "docker", "kubernetes", "ec2", "serverless"
- ✅ If type=kubernetes: `namespace` must not be empty
- ✅ If type=kubernetes: All replica counts must be >= 1
- ✅ If autoscaling enabled: `max_replicas` must be >= `min_replicas`

#### 16.3 Health Checks
- ✅ If enabled: `interval_seconds` must be 1-60
- ✅ If enabled: `timeout_seconds` must be 1-30 and < `interval_seconds`

---

## CROSS-MODULE VALIDATIONS (Dependencies)

### 1. Asset Consistency
- ✅ ALL enabled assets in `instrument_layer.supported_assets` must have:
  - Market data streams configured in `instrument_layer.market_data.providers`
  - Initial margin defined in `risk_engine.initial_margin`
  - Maintenance margin defined in `risk_engine.maintenance_margin`
  - If greeks.enabled: volatility configured (if manual mode)

### 2. Settlement Currency Consistency
- ✅ Primary settlement currency in `instrument_layer.settlement_currencies` must match:
  - `settlement.blockchain.primary_chain` must have this currency enabled
  - All fee configurations use this currency

### 3. Service Routing Consistency
- ✅ All services referenced in `services.routing` must exist in `services.registry`
- ✅ All endpoints in routing must be valid paths

### 4. Port Conflicts
- ✅ NO two services can use the same (host, port) combination
- ✅ Virtual mode ports (port + offset) must not conflict with production ports
- ✅ API ports (rest, websocket, grpc) must all be unique

### 5. Storage Consistency
- ✅ If production mode enabled: all storage types must be persistent (no inmemory)
- ✅ If virtual mode enabled: virtual storage must be separate from production

### 6. Authentication Consistency
- ✅ If services.auth.type = "bearer_token": ALL service tokens must be defined
- ✅ If api.authentication.type = "jwt": JWT secret must be defined

### 7. TLS Consistency
- ✅ If ANY tls_enabled = true: corresponding cert_path and key_path must be non-empty
- ✅ In production environment: TLS should be enabled for ALL external APIs

### 8. Blockchain Consistency
- ✅ If settlement.blockchain.enabled = true:
  - At least one settlement currency must have at least one enabled chain
  - Settlement wallet must be configured
  - Gas settings must be defined

---

## WARNINGS (Non-blocking but should be logged)

### 1. Performance Warnings
- ⚠️ `matching_engine.performance.matching_frequency_ms` < 10: May cause high CPU usage
- ⚠️ `oms.orderbook.update_frequency_ms` < 50: May cause network congestion
- ⚠️ `api.websocket.max_connections` > 50000: May require infrastructure scaling

### 2. Security Warnings
- ⚠️ `security.encryption.in_transit.tls_enabled` = false in production
- ⚠️ `compliance.kyc.enabled` = false in production
- ⚠️ `api.authentication.jwt.expiry_seconds` > 3600 in production
- ⚠️ ANY service auth token is short (<32 chars)

### 3. Financial Warnings
- ⚠️ `risk_engine.initial_margin` < 0.10 (10%): Very low margin requirement
- ⚠️ `risk_engine.liquidation.threshold` < 0.50: Aggressive liquidation
- ⚠️ `wallet.hot_wallet.max_balance_usdt` > 1000000: Large hot wallet exposure

### 4. Operational Warnings
- ⚠️ `monitoring.metrics.enabled` = false in production
- ⚠️ `monitoring.logging.level` = "debug" in production
- ⚠️ `monitoring.alerting.enabled` = false in production
- ⚠️ `deployment.health_checks.enabled` = false

---

## VALIDATION FLOW

```
1. Load config.yaml
2. Substitute environment variables (${VAR_NAME})
3. Parse YAML → Config struct
4. Run CRITICAL validations
   - If ANY fail → REJECT config, exit with error
5. Run CROSS-MODULE validations
   - If ANY fail → REJECT config, exit with error
6. Run WARNING validations
   - Log warnings but continue
7. Config is VALID → proceed with system startup
```

---

## ERROR MESSAGES

### Format
```
CONFIG VALIDATION FAILED: {module}.{field}
Reason: {specific violation}
Expected: {what is expected}
Actual: {what was provided}
```

### Example
```
CONFIG VALIDATION FAILED: risk_engine.initial_margin.BTC
Reason: Initial margin must be greater than maintenance margin
Expected: initial_margin (0.15) > maintenance_margin (0.20)
Actual: 0.15 <= 0.20
```

---

## IMPLEMENTATION CHECKLIST

For Claude Code to implement config validation:

1. ✅ Create `ConfigValidator` struct
2. ✅ Implement validation methods for each module
3. ✅ Implement cross-module validation
4. ✅ Create clear error types
5. ✅ Write comprehensive tests (valid & invalid configs)
6. ✅ Document all validation rules in code comments
7. ✅ Create CLI tool to validate config without starting system
