# MyExpenses

Standalone personal expense tracker for **Omarchy**, developed in:

`/Users/warren/Documents/Projects/MyBudget`

Local-only: register, paychecks, categories, budgets, and QFX import from the Omarchy bar. Own Omarchy shell plugin — not tied to any other app.

Plugin id: `mybudget.expenses`

## Features

- Month summary (spent, income, budget)
- Checkbook-style register
- Add expenses and income
- Category budgets
- QFX / OFX file import
- Data stored on disk as JSON (no cloud, no account)

## Install on Omarchy (ARM or x86)

This plugin is QML — no compile step. Works on `aarch64` and `x86_64`.

Copy this project into the plugins directory (folder name must match the plugin id):

```bash
rsync -av --exclude .git \
  /Users/warren/Documents/Projects/MyBudget/ \
  USER@OMARCHY_HOST:~/.config/omarchy/plugins/mybudget.expenses/
```

On the Omarchy machine:

```bash
omarchy plugin validate ~/.config/omarchy/plugins/mybudget.expenses
omarchy-shell shell rescanPlugins
omarchy plugin enable mybudget.expenses
omarchy bar put mybudget.expenses --section right
```

Or, from a checkout already on the Omarchy machine:

```bash
omarchy plugin add /Users/warren/Documents/Projects/MyBudget --enable
```

## Use

- Click **MyExpenses** on the bar to open the panel
- Escape closes it
- Summon from a terminal: `omarchy-shell shell summon mybudget.expenses '{}'`
- Hide: `omarchy-shell shell hide mybudget.expenses`

Ledger file (created on first open):

`~/.local/share/expenses/ledger.json`

For QFX import, use a path on the Omarchy machine (for example `~/Downloads/export.qfx`). A sample file is in `fixtures/synthetic.qfx`.

## Layout

```
manifest.json     Plugin contract
BarWidget.qml     Bar entry
Panel.qml         Main UI
Ledger.js         Totals and persistence helpers
Qfx.js            QFX/OFX parser
fixtures/         Sample import file
```

## Troubleshooting

```bash
uname -m
omarchy plugin list
qs log -p "$OMARCHY_PATH/shell" --tail 80
```

Edits under `~/.config/omarchy/plugins/mybudget.expenses/` reload automatically.
