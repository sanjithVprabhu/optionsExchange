# WHITE-LABEL OPTIONS EXCHANGE - PROJECT STRUCTURE

## Root Directory Structure

```
options-exchange/
├── config/
│   ├── master_config.yaml          # Main configuration
│   ├── .env                         # Environment variables (gitignored)
│   ├── examples/
│   │   ├── minimal_dev.yaml
│   │   ├── production_aws.yaml
│   │   └── supabase.yaml
│   └── schema/
│       └── config_schema.json
│
├── core/                            # Pure business logic (NO infrastructure)
│   ├── primitives/
│   │   ├── asset.rs
│   │   ├── currency.rs
│   │   └── types.rs
│   │
│   ├── instrument/
│   │   ├── domain.rs                # OptionInstrument, Market
│   │   ├── registry.rs              # Business logic
│   │   ├── validation.rs
│   │   ├── lifecycle.rs
│   │   └── traits.rs                # InstrumentStore trait
│   │
│   ├── oms/
│   │   ├── order.rs                 # Order types
│   │   ├── orderbook.rs             # In-memory orderbook logic
│   │   ├── lifecycle.rs
│   │   └── traits.rs                # OrderStore trait
│   │
│   ├── matching/
│   │   ├── engine.rs                # Price-time priority
│   │   ├── execution.rs             # Trade generation
│   │   └── traits.rs
│   │
│   ├── risk/
│   │   ├── margin.rs                # Margin calculations
│   │   ├── greeks.rs                # Option Greeks
│   │   ├── liquidation.rs           # Liquidation logic
│   │   ├── portfolio.rs             # Portfolio margin
│   │   └── traits.rs
│   │
│   ├── settlement/
│   │   ├── clearing.rs              # Continuous clearing
│   │   ├── expiry.rs                # Terminal settlement
│   │   ├── payoff.rs                # Payoff calculations
│   │   └── traits.rs
│   │
│   ├── wallet/
│   │   ├── balance.rs               # Balance management
│   │   ├── collateral.rs            # Collateral locking
│   │   └── traits.rs
│   │
│   └── market_data/
│       ├── pricing/
│       │   ├── black_scholes.rs     # BS model
│       │   ├── vol_surface.rs       # Volatility surface
│       │   └── mark_price.rs        # Mark price calculation
│       ├── feeds/
│       │   ├── orderbook.rs
│       │   ├── trades.rs
│       │   └── ticker.rs
│       └── traits.rs
│
├── adapters/                        # Infrastructure implementations
│   ├── storage/
│   │   ├── postgres/
│   │   │   ├── instruments.rs
│   │   │   ├── orders.rs
│   │   │   ├── wallets.rs
│   │   │   └── events.rs
│   │   ├── supabase/
│   │   │   └── ... (same structure)
│   │   ├── redis/
│   │   │   ├── orderbook.rs
│   │   │   └── cache.rs
│   │   └── inmemory/
│   │       └── ... (for testing)
│   │
│   ├── blockchain/
│   │   ├── ethereum.rs
│   │   ├── polygon.rs
│   │   └── traits.rs
│   │
│   ├── market_data_providers/
│   │   ├── binance.rs
│   │   ├── coinbase.rs
│   │   ├── websocket_generic.rs
│   │   └── grpc_generic.rs
│   │
│   └── messaging/
│       ├── grpc.rs
│       ├── http.rs
│       └── websocket.rs
│
├── services/                        # Service layer (orchestration)
│   ├── instrument_service/
│   │   ├── main.rs
│   │   ├── api.rs
│   │   └── handlers.rs
│   │
│   ├── oms_service/
│   │   ├── main.rs
│   │   ├── api.rs
│   │   └── handlers.rs
│   │
│   ├── matching_service/
│   │   ├── main.rs
│   │   ├── engine_runtime.rs
│   │   └── api.rs
│   │
│   ├── risk_service/
│   │   ├── main.rs
│   │   ├── calculator.rs
│   │   └── api.rs
│   │
│   ├── settlement_service/
│   │   ├── main.rs
│   │   ├── clearing_loop.rs
│   │   └── expiry_handler.rs
│   │
│   ├── wallet_service/
│   │   ├── main.rs
│   │   ├── balance_manager.rs
│   │   └── api.rs
│   │
│   └── market_data_service/
│       ├── main.rs
│       ├── feed_aggregator.rs
│       └── api.rs
│
├── config_loader/                   # Config parsing and validation
│   ├── parser.rs
│   ├── validator.rs
│   ├── builder.rs                   # Wires services based on config
│   └── env_substitution.rs
│
├── runtime/                         # Main orchestrator
│   ├── main.rs                      # Entry point
│   ├── service_registry.rs
│   └── shutdown.rs
│
├── cli/                             # Management tools
│   ├── validate_config.rs
│   ├── init_system.rs
│   ├── migrate_db.rs
│   └── admin.rs
│
├── tests/
│   ├── integration/
│   │   ├── end_to_end.rs
│   │   ├── order_flow.rs
│   │   └── settlement.rs
│   ├── blackswan/
│   │   ├── liquidation_cascade.rs
│   │   ├── price_shock.rs
│   │   └── replay_determinism.rs
│   └── property/                    # Property-based tests
│       ├── invariants.rs
│       └── scenarios.rs
│
├── docker/
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── docker-compose.dev.yml
│
├── k8s/
│   ├── deployments/
│   ├── services/
│   └── configmaps/
│
├── docs/
│   ├── architecture/
│   ├── api/
│   └── deployment/
│
├── scripts/
│   ├── setup_dev.sh
│   ├── run_tests.sh
│   └── deploy.sh
│
├── Cargo.toml                       # Workspace configuration
└── README.md
```

## Module Dependency Graph

```
┌─────────────────────────────────────────────────────────────┐
│                     CONFIG LAYER                            │
│  (Parsed once at startup, drives all service creation)     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   ADAPTER LAYER                             │
│  (Implements traits, chosen by config)                     │
│  - Storage (Postgres/Supabase/Redis)                       │
│  - Blockchain (Ethereum/Polygon)                           │
│  - Market Data (Binance/Coinbase)                          │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    CORE LAYER                               │
│  (Pure business logic, no infrastructure knowledge)        │
│                                                             │
│  Instrument ──> OMS ──> Risk ──> Matching ──> Settlement   │
│       │                   │                        │        │
│       └───────────────────┴────────────────────────┘        │
│                          Wallet                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   SERVICE LAYER                             │
│  (gRPC/HTTP/WebSocket endpoints)                           │
│  Each service runs independently, communicates via config  │
└─────────────────────────────────────────────────────────────┘
```

## Key Principles

1. **Core is infrastructure-agnostic**: Never imports postgres, redis, etc.
2. **Adapters implement traits**: All storage/blockchain/data providers implement traits defined in core
3. **Config drives wiring**: Builder pattern constructs services with chosen adapters
4. **Services are independent**: Can run monolith or distributed based on config
5. **Event-driven**: All state changes emit events for replay/audit

## Cargo Workspace Structure

```toml
[workspace]
members = [
    "core/primitives",
    "core/instrument",
    "core/oms",
    "core/matching",
    "core/risk",
    "core/settlement",
    "core/wallet",
    "core/market_data",
    "adapters/storage",
    "adapters/blockchain",
    "adapters/market_data_providers",
    "services/*",
    "config_loader",
    "runtime",
    "cli",
]
```
