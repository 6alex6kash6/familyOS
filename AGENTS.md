# FamilyOS Agent Notes

FamilyOS is a Rust terminal app. The product surface is a `clap` CLI plus a `ratatui` terminal UI.

Project rules:

- Keep the app local-first; default data lives in local SQLite at `data/familyos.sqlite`.
- Keep integrations read-only. Do not add buy, sell, trade, withdraw, transfer, payment execution, or Whitebird flows.
- Treat USD and USDT as one annual-income/planned-burn contour; keep BYN as the native spending currency.
- Preserve CSV import semantics: derive covered months from dated rows and replace only matching source/month rows.
- Preserve OKX current-year history window semantics: `before=<start>&after=<end>`.
- Priobank API remains planned until official API access exists; CSV import is supported.
- Product-facing labels should stay Russian-first and operational, not landing-page copy.

Use Cargo for verification:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
