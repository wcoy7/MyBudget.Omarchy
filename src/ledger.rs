//! Ledger shape, totals, edits, and QFX classification.
//!
//! Load is lenient in the same places as `Ledger.js`: an empty file is a valid
//! empty ledger, entries that are not integer amounts are dropped, and unknown
//! settings keys are kept.

use std::collections::BTreeMap;

use serde_json::{Map, Number, Value};

use crate::qfx::Txn;

const EM: &str = "\u{2014}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Expense,
    Income,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Expense => "expense",
            Kind::Income => "income",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub first_paycheck_name: String,
    pub second_paycheck_name: String,
    pub payroll_keywords: Vec<String>,
    pub min_import_cents: i64,
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first_paycheck_name: "First paycheck".into(),
            second_paycheck_name: "Second paycheck".into(),
            payroll_keywords: vec![
                "PAYROLL".into(),
                "DIRECT DEP".into(),
                "DIRECT DEPOSIT".into(),
                "SALARY".into(),
            ],
            min_import_cents: 0,
            extra: Map::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    pub id: String,
    pub year: i64,
    pub month: i64,
    pub category_id: String,
    pub amount_cents: i64,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub date: String,
    pub amount_cents: i64,
    pub title: String,
    pub note: String,
    pub original_note: String,
    pub category_id: String,
    pub account_id: String,
    pub parent_id: String,
    pub paycheck_half: String,
    pub import_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ledger {
    pub settings: Settings,
    pub categories: Vec<Category>,
    pub accounts: Vec<Value>,
    pub entries: Vec<Entry>,
    pub budgets: Vec<Budget>,
    pub title_mappings: Map<String, Value>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::empty()
    }
}

impl Ledger {
    pub fn empty() -> Self {
        Self {
            settings: Settings::default(),
            categories: default_categories(),
            accounts: Vec::new(),
            entries: Vec::new(),
            budgets: Vec::new(),
            title_mappings: Map::new(),
        }
    }

    pub fn is_effectively_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn default_categories() -> Vec<Category> {
    [
        ("food", "Food", "🍽"),
        ("transport", "Transport", "🚗"),
        ("entertainment", "Entertainment", "🎬"),
        ("shopping", "Shopping", "🛒"),
        ("utilities", "Utilities", "⚡"),
        ("income", "Income", "💵"),
        ("general", "General", "🏷"),
    ]
    .into_iter()
    .map(|(id, name, icon)| Category {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        extra: Map::new(),
    })
    .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadOutcome {
    pub ok: bool,
    pub state: Ledger,
    pub empty_file: bool,
    pub error: String,
}

pub fn load_strict(text: &str) -> LoadOutcome {
    if text.trim().is_empty() {
        return LoadOutcome {
            ok: true,
            state: Ledger::empty(),
            empty_file: true,
            error: String::new(),
        };
    }
    let data: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return load_error("Invalid JSON ledger"),
    };
    let Some(obj) = data.as_object() else {
        return load_error("Ledger root must be a JSON object");
    };
    let mut state = Ledger::empty();
    if let Some(settings) = obj.get("settings") {
        state.settings = merge_settings(settings);
    }
    if let Some(categories) = obj.get("categories").and_then(Value::as_array) {
        if !categories.is_empty() {
            let parsed: Vec<_> = categories.iter().filter_map(load_category).collect();
            if !parsed.is_empty() {
                state.categories = parsed;
            }
        }
    }
    if let Some(accounts) = obj.get("accounts").and_then(Value::as_array) {
        state.accounts = accounts.clone();
    }
    if let Some(entries) = obj.get("entries").and_then(Value::as_array) {
        state.entries = entries.iter().filter_map(normalize_entry).collect();
    }
    if let Some(budgets) = obj.get("budgets").and_then(Value::as_array) {
        state.budgets = budgets.iter().filter_map(load_budget).collect();
    }
    if let Some(mappings) = obj.get("titleMappings").and_then(Value::as_object) {
        state.title_mappings = mappings.clone();
    }
    LoadOutcome {
        ok: true,
        state,
        empty_file: false,
        error: String::new(),
    }
}

pub fn load_strict_bytes(bytes: &[u8]) -> LoadOutcome {
    match std::str::from_utf8(bytes) {
        Ok(text) => load_strict(text),
        Err(_) => load_error("Invalid JSON ledger"),
    }
}

fn load_error(error: &str) -> LoadOutcome {
    LoadOutcome {
        ok: false,
        state: Ledger::empty(),
        empty_file: false,
        error: error.into(),
    }
}

pub fn dump(state: &Ledger) -> String {
    let mut text =
        serde_json::to_string_pretty(&state_value(state)).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

fn state_value(state: &Ledger) -> Value {
    object([
        ("schemaVersion", Value::from(1)),
        ("settings", settings_value(&state.settings)),
        (
            "categories",
            Value::Array(state.categories.iter().map(category_value).collect()),
        ),
        ("accounts", Value::Array(state.accounts.clone())),
        (
            "entries",
            Value::Array(state.entries.iter().map(entry_value).collect()),
        ),
        (
            "budgets",
            Value::Array(state.budgets.iter().map(budget_value).collect()),
        ),
        ("titleMappings", Value::Object(state.title_mappings.clone())),
    ])
}

fn settings_value(settings: &Settings) -> Value {
    let mut map = object_map([
        (
            "firstPaycheckName",
            Value::String(settings.first_paycheck_name.clone()),
        ),
        (
            "secondPaycheckName",
            Value::String(settings.second_paycheck_name.clone()),
        ),
        (
            "payrollKeywords",
            Value::Array(
                settings
                    .payroll_keywords
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "minImportCents",
            Value::Number(Number::from(settings.min_import_cents)),
        ),
    ]);
    for (key, value) in &settings.extra {
        map.entry(key.clone()).or_insert_with(|| value.clone());
    }
    Value::Object(map)
}

fn category_value(category: &Category) -> Value {
    let mut map = object_map([
        ("id", Value::String(category.id.clone())),
        ("name", Value::String(category.name.clone())),
        ("icon", Value::String(category.icon.clone())),
    ]);
    for (key, value) in &category.extra {
        map.entry(key.clone()).or_insert_with(|| value.clone());
    }
    Value::Object(map)
}

fn entry_value(entry: &Entry) -> Value {
    object([
        ("id", Value::String(entry.id.clone())),
        ("kind", Value::String(entry.kind.as_str().into())),
        ("date", Value::String(entry.date.clone())),
        (
            "amountCents",
            Value::Number(Number::from(entry.amount_cents)),
        ),
        ("title", Value::String(entry.title.clone())),
        ("note", Value::String(entry.note.clone())),
        ("originalNote", Value::String(entry.original_note.clone())),
        ("categoryId", Value::String(entry.category_id.clone())),
        ("accountId", Value::String(entry.account_id.clone())),
        ("parentId", Value::String(entry.parent_id.clone())),
        ("paycheckHalf", Value::String(entry.paycheck_half.clone())),
        ("importId", Value::String(entry.import_id.clone())),
    ])
}

fn budget_value(budget: &Budget) -> Value {
    let mut map = object_map([
        ("id", Value::String(budget.id.clone())),
        ("year", Value::Number(Number::from(budget.year))),
        ("month", Value::Number(Number::from(budget.month))),
        ("categoryId", Value::String(budget.category_id.clone())),
        (
            "amountCents",
            Value::Number(Number::from(budget.amount_cents)),
        ),
    ]);
    for (key, value) in &budget.extra {
        map.entry(key.clone()).or_insert_with(|| value.clone());
    }
    Value::Object(map)
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(object_map(fields))
}

fn object_map<const N: usize>(fields: [(&str, Value); N]) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in fields {
        map.insert(key.to_string(), value);
    }
    map
}

fn merge_settings(value: &Value) -> Settings {
    let mut settings = Settings::default();
    let Some(obj) = value.as_object() else {
        return settings;
    };
    for (key, item) in obj {
        match key.as_str() {
            "firstPaycheckName" => {
                settings.first_paycheck_name = js_present_string(item).unwrap_or_default()
            }
            "secondPaycheckName" => {
                settings.second_paycheck_name = js_present_string(item).unwrap_or_default()
            }
            "payrollKeywords" => {
                settings.payroll_keywords = if let Some(list) = item.as_array() {
                    list.iter().map(js_string).collect()
                } else {
                    Settings::default().payroll_keywords
                };
            }
            "minImportCents" => {
                settings.min_import_cents = match item {
                    Value::Number(number) => number
                        .as_i64()
                        .or_else(|| number.as_f64().and_then(trunc_i64))
                        .unwrap_or(0),
                    _ => 0,
                };
            }
            _ => {
                settings.extra.insert(key.clone(), item.clone());
            }
        }
    }
    settings
}

fn load_category(value: &Value) -> Option<Category> {
    let obj = value.as_object()?;
    Some(Category {
        id: obj.get("id").map(js_string).unwrap_or_default(),
        name: obj.get("name").map(js_string).unwrap_or_default(),
        icon: obj.get("icon").map(js_string).unwrap_or_default(),
        extra: extras(obj, &["id", "name", "icon"]),
    })
}

fn load_budget(value: &Value) -> Option<Budget> {
    let obj = value.as_object()?;
    Some(Budget {
        id: obj.get("id").map(js_string).unwrap_or_default(),
        year: obj.get("year").and_then(json_number_i64).unwrap_or(0),
        month: obj.get("month").and_then(json_number_i64).unwrap_or(0),
        category_id: obj.get("categoryId").map(js_string).unwrap_or_default(),
        amount_cents: obj
            .get("amountCents")
            .and_then(json_i64)
            .map(|n| n)
            .unwrap_or(0),
        extra: extras(obj, &["id", "year", "month", "categoryId", "amountCents"]),
    })
}

fn extras(obj: &Map<String, Value>, known: &[&str]) -> Map<String, Value> {
    let mut extra = Map::new();
    for (key, value) in obj {
        if !known.contains(&key.as_str()) {
            extra.insert(key.clone(), value.clone());
        }
    }
    extra
}

fn normalize_entry(value: &Value) -> Option<Entry> {
    let obj = value.as_object()?;
    let amount = json_i64(obj.get("amountCents")?)?;
    let kind = if obj.get("kind").and_then(Value::as_str) == Some("income") {
        Kind::Income
    } else {
        Kind::Expense
    };
    let date = js_or_string(obj.get("date"), &today());
    let category_default = if kind == Kind::Income {
        "income"
    } else {
        "general"
    };
    Some(Entry {
        id: js_or_string(obj.get("id"), &uuid()),
        kind,
        date,
        amount_cents: amount.abs(),
        title: js_or_string(obj.get("title"), ""),
        note: js_or_string(obj.get("note"), ""),
        original_note: obj
            .get("originalNote")
            .and_then(js_present_string)
            .or_else(|| obj.get("note").and_then(js_present_string))
            .unwrap_or_default(),
        category_id: js_or_string(obj.get("categoryId"), category_default),
        account_id: js_or_string(obj.get("accountId"), ""),
        parent_id: js_or_string(obj.get("parentId"), ""),
        paycheck_half: match obj.get("paycheckHalf").and_then(Value::as_str) {
            Some("second") => "second".into(),
            Some("first") => "first".into(),
            _ => String::new(),
        },
        import_id: js_or_string(obj.get("importId"), ""),
    })
}

/// Fields the Add form and the importer pass into [`add_entry`].
#[derive(Clone, Debug)]
pub struct NewEntry {
    pub id: String,
    pub kind: Kind,
    pub date: String,
    pub amount_cents: i64,
    pub title: String,
    pub note: String,
    pub original_note: String,
    pub category_id: String,
    pub account_id: String,
    pub parent_id: String,
    pub paycheck_half: String,
    pub is_payroll: bool,
    pub import_id: String,
}

impl NewEntry {
    pub fn manual(
        kind: Kind,
        date: String,
        amount_cents: i64,
        title: String,
        note: String,
        category_id: String,
    ) -> Self {
        Self {
            id: String::new(),
            kind,
            date,
            amount_cents,
            title,
            note: note.clone(),
            original_note: note,
            category_id,
            account_id: String::new(),
            parent_id: String::new(),
            paycheck_half: String::new(),
            is_payroll: false,
            import_id: String::new(),
        }
    }
}

pub fn add_entry(state: &Ledger, fields: NewEntry) -> Ledger {
    let mut next = state.clone();
    let kind = fields.kind;
    let date = if parse_date(&fields.date).is_some() {
        fields.date
    } else {
        today()
    };
    let category_id = if fields.category_id.is_empty() {
        if kind == Kind::Income {
            "income"
        } else {
            "general"
        }
        .into()
    } else {
        fields.category_id
    };
    let paycheck_half = if !fields.paycheck_half.is_empty() {
        fields.paycheck_half
    } else if kind == Kind::Expense || fields.is_payroll {
        paycheck_half_for_date(&date)
    } else {
        String::new()
    };
    next.entries.push(Entry {
        id: if fields.id.is_empty() {
            uuid()
        } else {
            fields.id
        },
        kind,
        date,
        amount_cents: fields.amount_cents.abs(),
        title: fields.title,
        note: fields.note.clone(),
        original_note: if fields.original_note.is_empty() {
            fields.note
        } else {
            fields.original_note
        },
        category_id,
        account_id: fields.account_id,
        parent_id: fields.parent_id,
        paycheck_half,
        import_id: fields.import_id,
    });
    next
}

pub fn delete_entry(state: &Ledger, id: &str) -> Ledger {
    let mut next = state.clone();
    next.entries
        .retain(|entry| entry.id != id && entry.parent_id != id);
    next
}

pub fn upsert_budget(
    state: &Ledger,
    year: i64,
    month: i64,
    category_id: &str,
    cents: i64,
) -> Ledger {
    let mut next = state.clone();
    let amount = cents.abs();
    if let Some(existing) = next.budgets.iter_mut().find(|budget| {
        budget.year == year && budget.month == month && budget.category_id == category_id
    }) {
        existing.amount_cents = amount;
    } else {
        next.budgets.push(Budget {
            id: uuid(),
            year,
            month,
            category_id: category_id.into(),
            amount_cents: amount,
            extra: Map::new(),
        });
    }
    next
}

pub fn is_leaf(entry: &Entry, entries: &[Entry]) -> bool {
    !entries.iter().any(|other| other.parent_id == entry.id)
}

pub fn month_expense_cents(state: &Ledger, year: i64, month: i64) -> i64 {
    state
        .entries
        .iter()
        .filter(|entry| {
            entry.kind == Kind::Expense
                && is_leaf(entry, &state.entries)
                && in_month(&entry.date, year, month)
        })
        .map(|entry| entry.amount_cents.abs())
        .fold(0, i64::saturating_add)
}

pub fn month_income_cents(state: &Ledger, year: i64, month: i64) -> i64 {
    state
        .entries
        .iter()
        .filter(|entry| entry.kind == Kind::Income && in_month(&entry.date, year, month))
        .map(|entry| entry.amount_cents.abs())
        .fold(0, i64::saturating_add)
}

pub fn month_budget_cents(state: &Ledger, year: i64, month: i64) -> i64 {
    state
        .budgets
        .iter()
        .filter(|budget| {
            budget.year == year && budget.month == month && budget.category_id != "income"
        })
        .map(|budget| budget.amount_cents.abs())
        .fold(0, i64::saturating_add)
}

pub fn category_month_cents(state: &Ledger, year: i64, month: i64, category_id: &str) -> i64 {
    state
        .entries
        .iter()
        .filter(|entry| {
            entry.kind == Kind::Expense
                && is_leaf(entry, &state.entries)
                && entry.category_id == category_id
                && in_month(&entry.date, year, month)
        })
        .map(|entry| entry.amount_cents.abs())
        .fold(0, i64::saturating_add)
}

pub fn month_entries(state: &Ledger, year: i64, month: i64) -> Vec<Entry> {
    let mut out: Vec<Entry> = state
        .entries
        .iter()
        .filter(|entry| in_month(&entry.date, year, month))
        .filter(|entry| entry.kind != Kind::Expense || is_leaf(entry, &state.entries))
        .cloned()
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.title.cmp(&a.title)));
    out
}

pub fn category_name(state: &Ledger, id: &str) -> String {
    state
        .categories
        .iter()
        .find(|category| category.id == id)
        .map(|category| category.name.clone())
        .unwrap_or_else(|| EM.into())
}

pub fn budget_field_text(state: &Ledger, year: i64, month: i64, category_id: &str) -> String {
    state
        .budgets
        .iter()
        .find(|budget| {
            budget.year == year && budget.month == month && budget.category_id == category_id
        })
        .map(|budget| format_plain_cents(budget.amount_cents))
        .unwrap_or_default()
}

pub fn format_plain_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if neg { "-" } else { "" },
        abs / 100,
        abs % 100
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    pub state: Ledger,
    pub added: usize,
    pub skipped: usize,
    pub lines: Vec<String>,
}

pub fn import_txns(state: &Ledger, txns: &[Txn]) -> ImportReport {
    let mut next = state.clone();
    let mut seen = BTreeMap::<String, ()>::new();
    for entry in &next.entries {
        if !entry.import_id.is_empty() {
            seen.insert(entry.import_id.clone(), ());
        }
    }
    let keywords: Vec<String> = next
        .settings
        .payroll_keywords
        .iter()
        .map(|keyword| keyword.to_uppercase())
        .collect();
    let min_cents = next.settings.min_import_cents;
    let mut added = 0;
    let mut skipped = 0;
    let mut lines = Vec::new();
    for txn in txns {
        if !txn.fitid.is_empty() && seen.contains_key(&txn.fitid) {
            skipped += 1;
            continue;
        }
        let abs = txn.amount_cents.abs();
        if abs < min_cents || abs == 0 {
            continue;
        }
        let mapped = next
            .title_mappings
            .get(&txn.name.to_lowercase())
            .and_then(Value::as_object);
        let mapped_title = mapped
            .and_then(|item| item.get("title"))
            .and_then(js_present_string);
        let title = mapped_title.unwrap_or_else(|| {
            if txn.name.is_empty() {
                "Imported".into()
            } else {
                txn.name.clone()
            }
        });
        let classified = classify_txn(txn, &keywords);
        let category_id = mapped
            .and_then(|item| item.get("categoryId"))
            .and_then(js_present_string)
            .unwrap_or_else(|| match classified.kind {
                Kind::Income => "income".into(),
                Kind::Expense => match_category_id(&next, &classified.hay),
            });
        let half = if classified.is_payroll || classified.kind == Kind::Expense {
            paycheck_half_for_date(&txn.date)
        } else {
            String::new()
        };
        next.entries.push(Entry {
            id: uuid(),
            kind: classified.kind,
            date: txn.date.clone(),
            amount_cents: abs,
            title: title.clone(),
            note: txn.memo.clone(),
            original_note: txn.memo.clone(),
            category_id,
            account_id: String::new(),
            parent_id: String::new(),
            paycheck_half: half,
            import_id: txn.fitid.clone(),
        });
        if !txn.fitid.is_empty() {
            seen.insert(txn.fitid.clone(), ());
        }
        added += 1;
        lines.push(format!(
            "Added {}: {} {EM} {title}",
            classified.kind.as_str(),
            format_cents(abs)
        ));
    }
    let summary = if skipped > 0 {
        format!("Imported {added} transactions ({skipped} duplicates skipped)")
    } else {
        format!("Imported {added} transactions")
    };
    lines.insert(0, summary);
    ImportReport {
        state: next,
        added,
        skipped,
        lines,
    }
}

struct Classified {
    kind: Kind,
    is_payroll: bool,
    hay: String,
}

fn classify_txn(txn: &Txn, keywords: &[String]) -> Classified {
    let trntype = txn.trntype.to_ascii_uppercase();
    let amount = txn.amount_cents;
    let hay = format!("{} {}", txn.name, txn.memo).to_uppercase();
    // Amount sign wins. A zero amount falls back to the credit transaction types.
    let is_credit = if amount > 0 {
        true
    } else if amount < 0 {
        false
    } else {
        trntype == "CREDIT" || trntype == "DEP" || trntype == "DIRECTDEP"
    };
    let kind = if is_credit {
        Kind::Income
    } else {
        Kind::Expense
    };
    let is_payroll = kind == Kind::Income && contains_any(&hay, keywords);
    Classified {
        kind,
        is_payroll,
        hay,
    }
}

fn match_category_id(state: &Ledger, hay_upper: &str) -> String {
    for category in &state.categories {
        let name = category.name.to_lowercase();
        let hit = match name.as_str() {
            "food" => contains_any(
                hay_upper,
                &["GROCERY", "STARBUCKS", "MCDONALD", "RESTAURANT", "CAFE"],
            ),
            "transport" => contains_any(
                hay_upper,
                &["GAS", "FUEL", "UBER", "LYFT", "PARKING", "TRANSIT"],
            ),
            "entertainment" => contains_any(hay_upper, &["NETFLIX", "SPOTIFY", "MOVIE", "CINEMA"]),
            "shopping" => contains_any(hay_upper, &["AMAZON", "WALMART", "TARGET"]),
            "utilities" => contains_any(
                hay_upper,
                &["ELECTRIC", "WATER", "INTERNET", "PHONE", "UTILITY"],
            ),
            _ => false,
        };
        if hit {
            return category.id.clone();
        }
    }
    "general".into()
}

fn contains_any(hay: &str, needles: &[impl AsRef<str>]) -> bool {
    needles.iter().any(|needle| {
        let needle = needle.as_ref().to_uppercase();
        !needle.is_empty() && hay.contains(&needle)
    })
}

pub fn month_label(year: i64, month: i64) -> String {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let name = usize::try_from(month)
        .ok()
        .and_then(|month| month.checked_sub(1))
        .and_then(|index| NAMES.get(index).copied())
        .unwrap_or("—");
    format!("{name} {year}")
}

pub fn add_months(year: i64, month: i64, delta: i64) -> (i64, i64) {
    let mut month = month.saturating_add(delta);
    let mut year = year;
    while month > 12 {
        month -= 12;
        year += 1;
    }
    while month < 1 {
        month += 12;
        year -= 1;
    }
    (year, month)
}

pub fn parse_date(iso: &str) -> Option<(i64, i64, i64)> {
    let mut parts = iso.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if year == 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

pub fn in_month(iso: &str, year: i64, month: i64) -> bool {
    parse_date(iso).is_some_and(|(y, m, _)| y == year && m == month)
}

pub fn paycheck_half_for_date(iso: &str) -> String {
    match parse_date(iso) {
        Some((_, _, day)) if day <= 15 => "first".into(),
        Some(_) => "second".into(),
        None => "first".into(),
    }
}

pub fn parse_dollars(input: &str) -> Option<i64> {
    let trimmed: String = input
        .trim()
        .chars()
        .filter(|c| *c != '$' && *c != ',' && !c.is_whitespace())
        .collect();
    if trimmed.is_empty() {
        return None;
    }
    let negative = trimmed.starts_with('-') || trimmed.starts_with('(');
    let mut body = trimmed.as_str();
    if body.starts_with('-') || body.starts_with('(') {
        body = &body[1..];
    }
    if body.ends_with(')') {
        body = &body[..body.len() - 1];
    }
    if body.is_empty() || !dollar_body(body) {
        return None;
    }
    let mut parts = body.split('.');
    let whole: i64 = parts.next()?.parse().ok()?;
    let frac = match parts.next() {
        Some(text) if text.len() == 1 => text.parse::<i64>().ok()? * 10,
        Some(text) if text.len() == 2 => text.parse::<i64>().ok()?,
        Some(_) => return None,
        None => 0,
    };
    let cents = whole.saturating_mul(100).saturating_add(frac);
    Some(if negative { -cents } else { cents })
}

fn dollar_body(body: &str) -> bool {
    let mut parts = body.split('.');
    let whole = parts.next().unwrap_or("");
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match parts.next() {
        None => parts.next().is_none(),
        Some(frac) => {
            (frac.len() == 1 || frac.len() == 2)
                && frac.chars().all(|c| c.is_ascii_digit())
                && parts.next().is_none()
        }
    }
}

pub fn format_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if neg { "-$" } else { "$" },
        with_commas(abs / 100),
        abs % 100
    )
}

fn with_commas(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn today() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    // localtime_r writes a calendar date into `tm`. A null result falls back to the epoch date.
    let ptr = unsafe { libc::localtime_r(&now, &mut tm) };
    if ptr.is_null() {
        return "1970-01-01".into();
    }
    iso_date(
        tm.tm_year as i64 + 1900,
        tm.tm_mon as i64 + 1,
        tm.tm_mday as i64,
    )
}

pub fn today_parts() -> (i64, i64) {
    parse_date(&today())
        .map(|(year, month, _)| (year, month))
        .unwrap_or((1970, 1))
}

fn iso_date(year: i64, month: i64, day: i64) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn uuid() -> String {
    let bytes = random_bytes(16);
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        (bytes[6] & 0x0f) | 0x40,
        bytes[7],
        (bytes[8] & 0x3f) | 0x80,
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if file.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (index, byte) in buf.iter_mut().enumerate() {
        *byte = tick.rotate_left((index as u32).wrapping_mul(3)) as u8 ^ index as u8;
    }
    buf
}

pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let collapsed = collapse_slashes(trimmed);
    let mut out: Vec<&str> = Vec::new();
    for (index, part) in collapsed.split('/').enumerate() {
        if part.is_empty() && index == 0 {
            out.push("");
            continue;
        }
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if out.len() > 1 {
                out.pop();
            }
            continue;
        }
        out.push(part);
    }
    if out.len() == 1 && out[0].is_empty() {
        return "/".into();
    }
    let joined = out.join("/");
    if joined.is_empty() {
        "/".into()
    } else {
        joined
    }
}

fn collapse_slashes(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev = false;
    for ch in path.chars() {
        if ch == '/' {
            if !prev {
                out.push('/');
            }
            prev = true;
        } else {
            prev = false;
            out.push(ch);
        }
    }
    out
}

pub fn expand_home(path: &str, home: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if path == "~" {
        return home.to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    path.to_string()
}

pub fn is_allowed_import_path(
    path: &str,
    home: Option<&str>,
    extra_roots: &[String],
) -> Result<String, String> {
    if path.contains('\0') {
        return Err("Invalid path".into());
    }
    let home_norm = home
        .map(normalize_path)
        .filter(|item| item.starts_with('/') && item != "/");
    let trimmed = path.trim();
    let expanded = if trimmed == "~" || trimmed.starts_with("~/") {
        let Some(home) = home_norm.as_deref() else {
            return Err("HOME is unset".into());
        };
        expand_home(trimmed, home)
    } else {
        trimmed.to_string()
    };
    let expanded = normalize_path(&expanded);
    if expanded.is_empty() || !expanded.starts_with('/') {
        return Err("Use an absolute path or ~/...".into());
    }
    if expanded.contains('\0') {
        return Err("Invalid path".into());
    }
    let mut roots = Vec::new();
    if let Some(home) = &home_norm {
        for rel in [
            "Downloads",
            "Download",
            "documents",
            "Documents",
            ".config/omarchy/plugins/mybudget.expenses/fixtures",
        ] {
            roots.push(normalize_path(&format!("{home}/{rel}")));
        }
    }
    for extra in extra_roots {
        let root = normalize_path(extra);
        if !root.is_empty() {
            roots.push(root);
        }
    }
    let allowed = roots
        .iter()
        .any(|root| expanded == *root || expanded.starts_with(&format!("{root}/")));
    if !allowed {
        if home_norm.is_none() {
            return Err("HOME is unset".into());
        }
        return Err("Import is limited to ~/Downloads, ~/Documents, the plugin fixtures folder, or the app fixtures folder".into());
    }
    let lower = expanded.to_lowercase();
    if !(lower.ends_with(".qfx") || lower.ends_with(".ofx")) {
        return Err("Only .qfx / .ofx files are allowed".into());
    }
    Ok(expanded)
}

pub fn split_keywords(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn json_number_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().and_then(trunc_i64)),
        _ => None,
    }
}

fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().and_then(trunc_i64)),
        Value::String(text) => parse_int_prefix(text),
        _ => None,
    }
}

fn trunc_i64(value: f64) -> Option<i64> {
    if value.is_finite() {
        Some(value as i64)
    } else {
        None
    }
}

fn parse_int_prefix(text: &str) -> Option<i64> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let (neg, rest) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        (false, rest)
    } else {
        (false, text)
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let value: i64 = digits.parse().ok()?;
    Some(if neg { -value } else { value })
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".into(),
        Value::Array(_) | Value::Object(_) => "[object Object]".into(),
    }
}

fn js_present_string(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Bool(false) => None,
        Value::String(text) if text.is_empty() => None,
        Value::Number(number) if number.as_f64() == Some(0.0) => None,
        other => Some(js_string(other)),
    }
}

/// JavaScript `value || fallback`, then `String(...)`.
fn js_or_string(value: Option<&Value>, fallback: &str) -> String {
    match value {
        None | Some(Value::Null) => fallback.to_string(),
        Some(Value::String(text)) if text.is_empty() => fallback.to_string(),
        Some(Value::Bool(false)) => fallback.to_string(),
        Some(Value::Number(number)) if number.as_f64() == Some(0.0) => fallback.to_string(),
        Some(other) => js_string(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qfx;

    fn jan_expense(id: &str, day: &str, cents: i64, parent: &str) -> Entry {
        Entry {
            id: id.into(),
            kind: Kind::Expense,
            date: format!("2026-01-{day}"),
            amount_cents: cents,
            title: id.into(),
            note: String::new(),
            original_note: String::new(),
            category_id: "food".into(),
            account_id: String::new(),
            parent_id: parent.into(),
            paycheck_half: "first".into(),
            import_id: String::new(),
        }
    }

    #[test]
    fn empty_and_invalid_files() {
        let empty = load_strict("  \n");
        assert!(empty.ok && empty.empty_file && empty.state.entries.is_empty());
        assert_eq!(empty.state.categories[0].id, "food");
        assert!(!load_strict("{").ok);
        assert_eq!(
            load_strict("[1]").error,
            "Ledger root must be a JSON object"
        );
    }

    #[test]
    fn normalizes_entries_and_keeps_settings_extras() {
        let raw = r#"{
            "settings": {"firstPaycheckName": "Ada", "payrollKeywords": "nope", "minImportCents": 12, "theme": "ink"},
            "categories": [],
            "entries": [
                {"amountCents": "-12.9", "kind": "income", "title": "Pay", "date": "2026-01-20", "paycheckHalf": "second"},
                {"title": "drop me"}
            ]
        }"#;
        let loaded = load_strict(raw);
        assert!(loaded.ok);
        assert_eq!(loaded.state.settings.first_paycheck_name, "Ada");
        assert_eq!(
            loaded.state.settings.payroll_keywords,
            Settings::default().payroll_keywords
        );
        assert_eq!(loaded.state.settings.min_import_cents, 12);
        assert_eq!(
            loaded
                .state
                .settings
                .extra
                .get("theme")
                .and_then(Value::as_str),
            Some("ink")
        );
        assert_eq!(loaded.state.categories[0].id, "food");
        assert_eq!(loaded.state.entries.len(), 1);
        assert_eq!(loaded.state.entries[0].amount_cents, 12);
        assert_eq!(loaded.state.entries[0].kind, Kind::Income);
        assert_eq!(loaded.state.entries[0].paycheck_half, "second");
        let again = load_strict(&dump(&loaded.state));
        assert!(again.ok);
        assert_eq!(again.state.entries[0].title, "Pay");
        assert_eq!(
            again
                .state
                .settings
                .extra
                .get("theme")
                .and_then(Value::as_str),
            Some("ink")
        );
    }

    #[test]
    fn totals_ignore_parent_expenses_and_income_budgets() {
        let mut state = Ledger::empty();
        state.entries.push(jan_expense("parent", "04", 5_000, ""));
        state
            .entries
            .push(jan_expense("child", "04", 400, "parent"));
        state.entries.push(Entry {
            id: "pay".into(),
            kind: Kind::Income,
            date: "2026-01-15".into(),
            amount_cents: 2_000,
            title: "Pay".into(),
            note: String::new(),
            original_note: String::new(),
            category_id: "income".into(),
            account_id: String::new(),
            parent_id: String::new(),
            paycheck_half: "first".into(),
            import_id: String::new(),
        });
        state.budgets.push(Budget {
            id: "b1".into(),
            year: 2026,
            month: 1,
            category_id: "food".into(),
            amount_cents: 5_000,
            extra: Map::new(),
        });
        state.budgets.push(Budget {
            id: "b2".into(),
            year: 2026,
            month: 1,
            category_id: "income".into(),
            amount_cents: 9_000,
            extra: Map::new(),
        });
        assert_eq!(month_expense_cents(&state, 2026, 1), 400);
        assert_eq!(month_income_cents(&state, 2026, 1), 2_000);
        assert_eq!(month_budget_cents(&state, 2026, 1), 5_000);
        assert_eq!(category_month_cents(&state, 2026, 1, "food"), 400);
        let rows = month_entries(&state, 2026, 1);
        assert!(rows.iter().all(|entry| entry.id != "parent"));
        assert_eq!(rows[0].id, "pay");
        let deleted = delete_entry(&state, "parent");
        assert!(
            deleted
                .entries
                .iter()
                .all(|entry| entry.id != "child" && entry.id != "parent")
        );
    }

    #[test]
    fn dollars_and_month_math() {
        assert_eq!(parse_dollars(" $1,234.50 "), Some(123_450));
        assert_eq!(parse_dollars("12.5"), Some(1_250));
        assert_eq!(parse_dollars("(3.00)"), Some(-300));
        assert_eq!(parse_dollars("12.555"), None);
        assert_eq!(parse_dollars(""), None);
        assert_eq!(format_cents(123_450), "$1,234.50");
        assert_eq!(format_cents(-105), "-$1.05");
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        assert_eq!(paycheck_half_for_date("2026-01-15"), "first");
        assert_eq!(paycheck_half_for_date("2026-01-16"), "second");
        assert_eq!(month_label(2026, 11), "November 2026");
    }

    #[test]
    fn add_assigns_paycheck_half_only_for_expenses() {
        let expense = add_entry(
            &Ledger::empty(),
            NewEntry::manual(
                Kind::Expense,
                "2026-01-20".into(),
                -475,
                "Cafe".into(),
                "note".into(),
                "food".into(),
            ),
        );
        assert_eq!(expense.entries[0].amount_cents, 475);
        assert_eq!(expense.entries[0].paycheck_half, "second");
        assert_eq!(expense.entries[0].original_note, "note");
        let income = add_entry(
            &Ledger::empty(),
            NewEntry::manual(
                Kind::Income,
                "not-a-date".into(),
                100,
                "Gift".into(),
                String::new(),
                String::new(),
            ),
        );
        assert_eq!(income.entries[0].category_id, "income");
        assert_eq!(income.entries[0].paycheck_half, "");
        assert!(parse_date(&income.entries[0].date).is_some());
    }

    #[test]
    fn synthetic_import_and_duplicates() {
        let txns = qfx::parse(include_str!("../fixtures/synthetic.qfx"));
        let report = import_txns(&Ledger::empty(), &txns);
        assert_eq!(report.added, 3);
        assert_eq!(report.lines[0], "Imported 3 transactions");
        assert_eq!(report.lines[1], "Added expense: $4.75 — Example Cafe");
        let cafe = report
            .state
            .entries
            .iter()
            .find(|entry| entry.import_id == "synth-coffee-1")
            .unwrap();
        assert_eq!(cafe.category_id, "food");
        assert_eq!(cafe.paycheck_half, "first");
        assert_eq!(cafe.kind, Kind::Expense);
        let pay = report
            .state
            .entries
            .iter()
            .find(|entry| entry.import_id == "synth-pay-1")
            .unwrap();
        assert_eq!(pay.kind, Kind::Income);
        assert_eq!(pay.paycheck_half, "first");
        let refund = report
            .state
            .entries
            .iter()
            .find(|entry| entry.import_id == "synth-refund-1")
            .unwrap();
        assert_eq!(refund.paycheck_half, "");
        assert_eq!(month_expense_cents(&report.state, 2026, 1), 475);
        assert_eq!(month_income_cents(&report.state, 2026, 1), 213_500);
        let again = import_txns(&report.state, &txns);
        assert_eq!(again.added, 0);
        assert_eq!(again.skipped, 3);
        assert_eq!(
            again.lines[0],
            "Imported 0 transactions (3 duplicates skipped)"
        );
    }

    #[test]
    fn min_import_and_sign_override_and_mapping() {
        let mut state = Ledger::empty();
        state.settings.min_import_cents = 1_000;
        state.title_mappings.insert(
            "example store".into(),
            serde_json::json!({"title": "Corner store", "categoryId": "shopping"}),
        );
        let txns = vec![
            Txn {
                trntype: "CREDIT".into(),
                date: "2026-01-02".into(),
                amount_cents: -500,
                fitid: "a".into(),
                name: "Nope".into(),
                memo: String::new(),
            },
            Txn {
                trntype: "DEBIT".into(),
                date: "2026-01-02".into(),
                amount_cents: 3_500,
                fitid: "b".into(),
                name: "Example Store".into(),
                memo: "Refund".into(),
            },
        ];
        let report = import_txns(&state, &txns);
        assert_eq!(report.added, 1);
        assert_eq!(report.state.entries[0].kind, Kind::Income);
        assert_eq!(report.state.entries[0].title, "Corner store");
        assert_eq!(report.state.entries[0].category_id, "shopping");
        assert_eq!(report.state.entries[0].paycheck_half, "");
    }

    #[test]
    fn import_paths_stay_inside_the_jail() {
        let home = Some("/Users/ada");
        assert!(is_allowed_import_path("~/Downloads/export.qfx", home, &[]).is_ok());
        assert!(is_allowed_import_path("~/Documents/a.ofx", home, &[]).is_ok());
        assert!(is_allowed_import_path("~/Downloads/../.ssh/id.qfx", home, &[]).is_err());
        assert!(is_allowed_import_path("~/Desktop/a.qfx", home, &[]).is_err());
        assert_eq!(
            is_allowed_import_path("~/Downloads/a.txt", home, &[]).unwrap_err(),
            "Only .qfx / .ofx files are allowed"
        );
        assert!(is_allowed_import_path("export.qfx", home, &[]).is_err());
        let extra = vec!["/opt/mybudget/fixtures".into()];
        assert!(
            is_allowed_import_path("/opt/mybudget/fixtures/synthetic.qfx", None, &extra).is_ok()
        );
        assert_eq!(
            is_allowed_import_path("/tmp/a.qfx", None, &extra).unwrap_err(),
            "HOME is unset"
        );
    }

    #[test]
    fn budget_upsert_replaces_the_same_cell() {
        let once = upsert_budget(&Ledger::empty(), 2026, 1, "food", -2500);
        let twice = upsert_budget(&once, 2026, 1, "food", 2600);
        assert_eq!(twice.budgets.len(), 1);
        assert_eq!(twice.budgets[0].amount_cents, 2600);
        assert_eq!(twice.budgets[0].id, once.budgets[0].id);
        assert_eq!(budget_field_text(&twice, 2026, 1, "food"), "26.00");
        assert_eq!(budget_field_text(&twice, 2026, 1, "transport"), "");
    }
}
