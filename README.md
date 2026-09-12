# MyExpenses

Standalone personal expense tracker for [Omarchy](https://omarchy.org). Local-only register, paychecks, categories, budgets, and QFX import from the Omarchy bar.

Plugin id: `mybudget.expenses`

## Install

On an Omarchy machine (`aarch64` or `x86_64`). This is a QML shell plugin — nothing to compile.

### From GitHub

```bash
omarchy plugin add https://github.com/wcoy7/MyBudget.Omarchy.git --enable
omarchy bar put mybudget.expenses --section right
```

### From a local checkout

```bash
git clone https://github.com/wcoy7/MyBudget.Omarchy.git
omarchy plugin add ./MyBudget.Omarchy --enable
omarchy bar put mybudget.expenses --section right
```

### Manual copy

```bash
git clone https://github.com/wcoy7/MyBudget.Omarchy.git
mkdir -p ~/.config/omarchy/plugins
cp -R MyBudget.Omarchy ~/.config/omarchy/plugins/mybudget.expenses

omarchy plugin validate ~/.config/omarchy/plugins/mybudget.expenses
omarchy-shell shell rescanPlugins
omarchy plugin enable mybudget.expenses
omarchy bar put mybudget.expenses --section right
```

The plugin directory name must match the id: `mybudget.expenses`.

## Use

- Click **MyExpenses** on the bar to open the panel
- Press Escape to close
- Summon: `omarchy-shell shell summon mybudget.expenses '{}'`
- Hide: `omarchy-shell shell hide mybudget.expenses`

Ledger file (created on first open):

`~/.local/share/expenses/ledger.json`

QFX import: on the Import page, enter a path on the Omarchy machine (for example `~/Downloads/export.qfx`). A sample file ships as `fixtures/synthetic.qfx`.

## Update

```bash
omarchy plugin update mybudget.expenses
```

Or, in a git checkout under `~/.config/omarchy/plugins/mybudget.expenses`:

```bash
git pull
omarchy-shell shell rescanPlugins
```

## Remove

```bash
omarchy plugin disable mybudget.expenses
omarchy plugin remove mybudget.expenses
```

## Features

- Month summary (spent, income, budget)
- Checkbook-style register
- Add expenses and income
- Category budgets
- QFX / OFX import
- Local JSON storage (no cloud, no account)

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
omarchy plugin validate ~/.config/omarchy/plugins/mybudget.expenses
qs log -p "$OMARCHY_PATH/shell" --tail 80
```

Edits under `~/.config/omarchy/plugins/mybudget.expenses/` reload automatically.
