# MyExpenses

Local expense tracker. A checkbook register, paycheck names, category budgets, and QFX import, stored in one JSON file on this machine. No account and no network sync.

This is the [Rune](https://github.com/anomalyco/rune) rewrite of the Omarchy plugin. It reads and writes the same ledger the plugin used.

## Run

```bash
cargo run
```

The window opens on the current month. Summary, Register, Add, Import, Budget, and Settings are the same six pages as the plugin. Spent, Income, and Budget stay on screen while you move between them.

Ledger files:

- `~/.local/share/expenses/ledger.json` — live ledger, created on the first successful save
- `~/.local/share/expenses/ledger.json.bak` — previous live file, written before each save

If the live file is missing or damaged, MyExpenses tries the backup. It will not overwrite a damaged ledger with an empty one. Add an entry before the first save in that case.

QFX import accepts `.qfx` and `.ofx` files under `~/Downloads`, `~/Download`, `~/Documents`, `~/documents`, the old plugin fixtures folder, or this crate's `fixtures/` directory. `fixtures/synthetic.qfx` is a three-row sample.

```bash
cargo test
cargo run -- --snap /tmp/myexpenses.png --scene sample --size 960x720
```

`--scene sample` loads the synthetic file into memory and does not write your ledger.

Rune is pinned to a revision of `dev` in `Cargo.toml`. The app is a `view` function over a `State`, run with `rune::run(AppSpec::new(...))`, and the form controls come from `rune-kit`.
