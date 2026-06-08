# FamilyOS

Local-first terminal dashboard for private household finance visibility. FamilyOS is now a Rust binary with both direct CLI commands and an interactive terminal UI.

The app stores data in a local SQLite file, keeps integrations read-only, and does not implement buy, sell, trade, withdraw, payment, Whitebird connector, or a standalone FX/rates module.

## Stack

- Rust 1.89+
- Cargo
- SQLite via `rusqlite`
- `clap` CLI
- `ratatui` + `crossterm` terminal UI
- `reqwest` read-only OKX/Freedom sync

## Run

```bash
cargo test
cargo run -- status
cargo run -- tui
```

Default database:

```bash
file:data/familyos.sqlite
```

The app creates the SQLite schema and base settings/integration rows on first run.

## Commands

```bash
cargo run -- status
cargo run -- status --json
cargo run -- tui

cargo run -- expenses list
cargo run -- expenses add --date 2026-06-08 --amount 30 --currency BYN --category "Кофе" --note "cafe"
cargo run -- expenses delete <id>

cargo run -- goals list
cargo run -- goals add --name "Trip" --target 2000 --currency USDT --deadline 2026-09-30
cargo run -- goals delete <id>

cargo run -- settings show
cargo run -- settings set --monthly-income 5500 --monthly-taxable-base 3000 --tax-rate 0.13

cargo run -- import csv --source csv_manual --file ./report.csv
cargo run -- import csv --source csv_priobank --file ./priorbank.csv

cargo run -- integrations list
cargo run -- integrations sync okx
cargo run -- integrations sync freedom

cargo run -- reports daily-brief
cargo run -- reports monthly-snapshot
cargo run -- reports monthly-close

cargo run -- taxes
cargo run -- assets
```

## Environment

Create `.env.local` only on the local machine if you load variables through your shell. It is ignored by git.

```bash
export OKX_API_KEY=
export OKX_API_SECRET=
export OKX_API_PASSPHRASE=
export OKX_BASE_URL=https://www.okx.com

export FREEDOM_PUBLIC_KEY=
export FREEDOM_PRIVATE_KEY=
export FREEDOM_BASE_URL=https://freedom24.com
export FREEDOM_API_DOCS_URL=https://freedom24.com/tradernet-api/auth-login
export FREEDOM_TRADERNET_API_URL=https://tradernet.ru/api

export DATABASE_URL=file:data/familyos.sqlite
```

Without usable OKX/Freedom credentials, sync stays `not_connected` and does not inject sample financial data.

## Product Rules

- First screen is the operational dashboard: `Обзор`.
- UI/CLI copy is Russian-first where it represents the product surface.
- USD and USDT are treated as one contour for annual income and planned burn.
- BYN remains a native spending currency.
- CSV import derives covered months from dated rows in the file.
- OKX current-year history windows intentionally use `before=<start>&after=<end>`.
- Priobank API stays planned until official API access exists; CSV import is supported.
