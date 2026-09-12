# MyExpenses

Standalone personal expense tracker for [Omarchy](https://omarchy.org). Local-only register, paychecks, categories, budgets, and QFX import.

Plugin id: `mybudget.expenses`

## Install

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
# fully restart if the panel still fails to open:
# omarchy restart shell
omarchy plugin enable mybudget.expenses
omarchy bar put mybudget.expenses --section right
```

The plugin directory name must match the id: `mybudget.expenses`.

### App menu (Walker) + fullscreen

The plugin is both a **bar widget** and a **fullscreen overlay**. After install, add the desktop entry so Walker can launch it:

```bash
mkdir -p ~/.local/share/applications
cp ~/.config/omarchy/plugins/mybudget.expenses/mybudget-expenses.desktop \
  ~/.local/share/applications/
# or from a checkout:
# cp ./mybudget-expenses.desktop ~/.local/share/applications/
```

Then search for **MyExpenses** in Walker. That toggles the fullscreen overlay.

## Use

- Click **MyExpenses** on the bar → anchored panel
- Walker / app menu → fullscreen overlay
- Escape (or Close) dismisses the open surface
- Toggle overlay: `omarchy-shell shell toggle mybudget.expenses '{}'`
- Hide overlay: `omarchy-shell shell hide mybudget.expenses`

Note: with both kinds declared, shell `summon` / `toggle` on this id opens the **overlay**, not the bar panel. The bar chip still opens the panel directly.

Ledger files:

- `~/.local/share/expenses/ledger.json` — live ledger (created on first successful save)
- `~/.local/share/expenses/ledger.json.bak` — previous copy, written before each save

If the live file is missing or corrupt, the plugin tries the `.bak` restore. It will not overwrite a damaged ledger with an empty file.

QFX import: on the Import page, enter a path on the Omarchy machine (for example `~/Downloads/export.qfx`). A sample file ships as `fixtures/synthetic.qfx`.

## Update

```bash
omarchy plugin update mybudget.expenses
```

Or, in a git checkout under `~/.config/omarchy/plugins/mybudget.expenses`:

```bash
git pull
omarchy-shell shell rescanPlugins
# if UI still looks stale:
omarchy restart shell
```

Re-copy the desktop file after updates if you installed it manually.

## Remove

```bash
omarchy plugin disable mybudget.expenses
omarchy plugin remove mybudget.expenses
rm -f ~/.local/share/applications/mybudget-expenses.desktop
```

## Features

- Month summary (spent, income, budget)
- Checkbook-style register
- Add expenses and income
- Category budgets
- QFX / OFX import
- Local JSON storage (no cloud, no account)
- Bar panel and fullscreen overlay

## Layout

```
manifest.json              Plugin contract (bar-widget + overlay)
BarWidget.qml              Bar entry
Panel.qml                  Anchored bar panel chrome
Overlay.qml                Fullscreen overlay chrome
ExpensesApp.qml            Shared ledger UI
Ledger.js                  Totals and persistence helpers
Qfx.js                     QFX/OFX parser
mybudget-expenses.desktop  Walker / app menu launcher
fixtures/                  Sample import file
```

If the bar label appears but clicking only shows a tiny menu/tooltip and no panel:

```bash
omarchy restart shell
qs log -p "$OMARCHY_PATH/shell" --tail 80
```

Look for `MyExpenses 0.1.4 panel ready`. A failed `Panel.qml` load leaves the bar button alive but with nothing to open.

## Verify install

After updating, the bar tooltip should say **Open MyExpenses 0.1.4**. If it still says an older tip:

```bash
omarchy plugin update mybudget.expenses
omarchy restart shell
```

## Troubleshooting

```bash
uname -m
omarchy plugin list
omarchy plugin validate ~/.config/omarchy/plugins/mybudget.expenses
omarchy-shell shell toggle mybudget.expenses '{}'
qs log -p "$OMARCHY_PATH/shell" --tail 80
```

Edits under `~/.config/omarchy/plugins/mybudget.expenses/` reload with `rescanPlugins`; prefer a full shell restart after structural changes (new overlay entry).
