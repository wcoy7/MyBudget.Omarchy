//! MyExpenses window. One Rune view covers the summary, register, add, import,
//! budget, and settings flows that the Omarchy panel and overlay both hosted.

use std::collections::HashMap;
use std::path::Path;

use mybudget::ledger::{
    self, Kind, Ledger, NewEntry, add_entry, add_months, budget_field_text, category_month_cents,
    category_name, format_cents, import_txns, is_allowed_import_path, month_budget_cents,
    month_entries, month_expense_cents, month_income_cents, month_label, parse_dollars,
    split_keywords, today, today_parts, upsert_budget,
};
use mybudget::qfx;
use mybudget::store::{self, QFX_MAX, Session, Storage};
use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, ScrollSpec, TextEdit, Tx, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, TextInput};

const VERSION_LABEL: &str = "MyExpenses v0.1.4";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Summary,
    Register,
    Add,
    Import,
    Budget,
    Settings,
}

struct State {
    session: Session,
    page: Page,
    year: i64,
    month: i64,
    form_title: TextEdit,
    form_amount: TextEdit,
    form_date: TextEdit,
    form_note: TextEdit,
    form_kind: Kind,
    form_category: String,
    import_path: TextEdit,
    import_log: String,
    first_name: TextEdit,
    second_name: TextEdit,
    keywords: TextEdit,
    budget_fields: HashMap<String, TextEdit>,
}

impl State {
    fn fresh(storage: Storage) -> Self {
        Self::from_session(Session::open(storage))
    }

    fn from_session(session: Session) -> Self {
        let (year, month) = today_parts();
        Self {
            session,
            page: Page::Summary,
            year,
            month,
            form_title: TextEdit::new(""),
            form_amount: TextEdit::new(""),
            form_date: TextEdit::new(today()),
            form_note: TextEdit::new(""),
            form_kind: Kind::Expense,
            form_category: "general".into(),
            import_path: TextEdit::new(""),
            import_log: String::new(),
            first_name: TextEdit::new(""),
            second_name: TextEdit::new(""),
            keywords: TextEdit::new(""),
            budget_fields: HashMap::new(),
        }
    }
}

fn load() -> State {
    if sample_requested() {
        return State::fresh(Storage::Memory);
    }
    let mut state = State::from_session(Session::from_env());
    sync_editors(&mut state);
    state
}

fn sample_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    args.windows(2)
        .any(|pair| pair[0] == "--scene" && pair[1] == "sample")
}

fn scene_sample(state: &mut State) {
    state.session.storage = Storage::Memory;
    state.session.block_empty_persist = false;
    let report = import_txns(
        &Ledger::empty(),
        &qfx::parse(include_str!("../fixtures/synthetic.qfx")),
    );
    state.session.state = report.state;
    state.session.status = "Sample data".into();
    state.year = 2026;
    state.month = 1;
    state.page = Page::Register;
    sync_editors(state);
}

fn poll(state: &mut State, _: f64, _: &mut Tx) -> bool {
    if state.session.reload_if_changed() {
        sync_editors(state);
        true
    } else {
        false
    }
}

pub fn run() {
    rune::run(AppSpec {
        scenes: &[("sample", scene_sample)],
        poll: Some(poll),
        min_size: Some((720.0, 560.0)),
        ..AppSpec::new("MyExpenses", (960.0, 720.0), load, view)
    });
}

fn sync_editors(state: &mut State) {
    state
        .first_name
        .replace_text(&state.session.state.settings.first_paycheck_name);
    state
        .second_name
        .replace_text(&state.session.state.settings.second_paycheck_name);
    state
        .keywords
        .replace_text(&state.session.state.settings.payroll_keywords.join(", "));
    fill_budgets(state);
}

fn fill_budgets(state: &mut State) {
    let ids: Vec<String> = state
        .session
        .state
        .categories
        .iter()
        .filter(|category| category.id != "income")
        .map(|category| category.id.clone())
        .collect();
    for id in ids {
        let text = budget_field_text(&state.session.state, state.year, state.month, &id);
        let field = state
            .budget_fields
            .entry(id)
            .or_insert_with(|| TextEdit::new(""));
        if field.text != text {
            field.replace_text(text);
        }
    }
}

fn settings_dirty(state: &State) -> bool {
    state.first_name.text != state.session.state.settings.first_paycheck_name
        || state.second_name.text != state.session.state.settings.second_paycheck_name
        || split_keywords(&state.keywords.text) != state.session.state.settings.payroll_keywords
}

fn commit_settings(state: &mut State) {
    let mut next = state.session.state.clone();
    next.settings.first_paycheck_name = state.first_name.text.clone();
    next.settings.second_paycheck_name = state.second_name.text.clone();
    next.settings.payroll_keywords = split_keywords(&state.keywords.text);
    state.session.apply(next, "Saved");
}

fn commit_settings_if_dirty(state: &mut State) {
    if state.page == Page::Settings && settings_dirty(state) {
        commit_settings(state);
    }
}

fn go(state: &mut State, page: Page) {
    commit_settings_if_dirty(state);
    state.page = page;
}

fn shift_month(state: &mut State, delta: i64) {
    commit_settings_if_dirty(state);
    let (year, month) = add_months(state.year, state.month, delta);
    state.year = year;
    state.month = month;
    fill_budgets(state);
}

fn save_entry(state: &mut State) {
    let Some(cents) = parse_dollars(&state.form_amount.text) else {
        state.session.status = "Enter a valid amount".into();
        return;
    };
    if cents == 0 {
        state.session.status = "Enter a valid amount".into();
        return;
    }
    let next = add_entry(
        &state.session.state,
        NewEntry::manual(
            state.form_kind,
            state.form_date.text.clone(),
            cents,
            state.form_title.text.clone(),
            state.form_note.text.clone(),
            state.form_category.clone(),
        ),
    );
    state.session.apply(next, "Saved");
    state.form_title.clear();
    state.form_amount.clear();
    state.form_note.clear();
    state.page = Page::Register;
}

fn set_budget(state: &mut State, category_id: &str) {
    let raw = state
        .budget_fields
        .get(category_id)
        .map(|field| field.text.clone())
        .unwrap_or_default();
    let cents = parse_dollars(&raw).map(|value| value.abs()).unwrap_or(0);
    let next = upsert_budget(
        &state.session.state,
        state.year,
        state.month,
        category_id,
        cents,
    );
    state.session.apply(next, "Budget saved");
    if let Some(field) = state.budget_fields.get_mut(category_id) {
        let text = budget_field_text(&state.session.state, state.year, state.month, category_id);
        field.replace_text(text);
    }
}

fn import_qfx(state: &mut State) {
    let home = std::env::var("HOME").ok();
    match is_allowed_import_path(&state.import_path.text, home.as_deref(), &extra_roots()) {
        Err(reason) => state.import_log = reason,
        Ok(path) => match store::read_file(Path::new(&path), QFX_MAX) {
            Err(error) => state.import_log = format!("Could not read file: {}", error.message),
            Ok(bytes) => match std::str::from_utf8(&bytes) {
                Err(_) => state.import_log = "Could not read file: invalid UTF-8".into(),
                Ok(text) => {
                    let report = import_txns(&state.session.state, &qfx::parse(text));
                    state
                        .session
                        .apply(report.state, &format!("Imported {}", report.added));
                    state.import_log = report.lines.join("\n");
                }
            },
        },
    }
}

fn extra_roots() -> Vec<String> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    if fixtures.is_dir() {
        vec![fixtures.to_string_lossy().into_owned()]
    } else {
        Vec::new()
    }
}

fn delete_row(state: &mut State, id: &str) {
    let next = ledger::delete_entry(&state.session.state, id);
    state.session.apply(next, "Deleted");
}

fn view(state: &State, cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(cx.system_dark());
    let t = tok::theme(cx.system_dark());
    let body = match state.page {
        Page::Summary => summary_page(state, &t),
        Page::Register => register_page(state, &k, &t),
        Page::Add => add_page(state, &k),
        Page::Import => import_page(state, &k, &t),
        Page::Budget => budget_page(state, &k, &t),
        Page::Settings => settings_page(state, &k, &t),
    };
    let screen = col()
        .w_full()
        .h_full()
        .padding(20.0)
        .gap(10.0)
        .fill(t.bg)
        .child(header(state, &k, &t))
        .child(nav(state, &k))
        .child(
            text(VERSION_LABEL)
                .key("version")
                .font(type_::LABEL)
                .color(t.text_secondary),
        )
        .child(totals(state, &t))
        .child(
            body.scroll(ScrollSpec {
                stick_bottom: false,
            })
            .grow(1.0)
            .min_h(0.0)
            .w_full(),
        );
    rune_kit::root(screen, Vec::<El>::new()).on_window_focus(|state: &mut State, focused, _| {
        if focused {
            if state.session.reload_if_changed() {
                sync_editors(state);
            }
        } else {
            commit_settings_if_dirty(state);
        }
    })
}

fn header(state: &State, k: &Kit, t: &rune::tok::Theme) -> El {
    row()
        .gap(8.0)
        .center_y()
        .w_full()
        .child(button(
            "‹",
            ButtonVariant::Ghost,
            k,
            "month-prev",
            |state| shift_month(state, -1),
        ))
        .child(
            text(month_label(state.year, state.month))
                .key("month-label")
                .font(type_::TITLE)
                .color(t.text),
        )
        .child(button(
            "›",
            ButtonVariant::Ghost,
            k,
            "month-next",
            |state| shift_month(state, 1),
        ))
        .child(
            text(state.session.status.clone())
                .key("status")
                .font(type_::LABEL)
                .color(t.text_secondary)
                .grow(1.0)
                .min_w(0.0),
        )
}

fn nav(state: &State, k: &Kit) -> El {
    let pages = [
        (Page::Summary, "Summary", "page-summary"),
        (Page::Register, "Register", "page-register"),
        (Page::Add, "Add", "page-add"),
        (Page::Import, "Import", "page-import"),
        (Page::Budget, "Budget", "page-budget"),
        (Page::Settings, "Settings", "page-settings"),
    ];
    let mut strip = row().gap(6.0);
    for (page, label, key) in pages {
        let variant = if state.page == page {
            ButtonVariant::Contrast
        } else {
            ButtonVariant::Ghost
        };
        strip = strip.child(button(label, variant, k, key, move |state| go(state, page)));
    }
    strip
}

fn totals(state: &State, t: &rune::tok::Theme) -> El {
    row()
        .gap(16.0)
        .child(
            text(format!(
                "Spent  {}",
                format_cents(month_expense_cents(
                    &state.session.state,
                    state.year,
                    state.month
                ))
            ))
            .key("spent")
            .font(type_::BODY)
            .color(t.text),
        )
        .child(
            text(format!(
                "Income  {}",
                format_cents(month_income_cents(
                    &state.session.state,
                    state.year,
                    state.month
                ))
            ))
            .key("income")
            .font(type_::BODY)
            .color(t.text),
        )
        .child(
            text(format!(
                "Budget  {}",
                format_cents(month_budget_cents(
                    &state.session.state,
                    state.year,
                    state.month
                ))
            ))
            .key("budget")
            .font(type_::BODY)
            .color(t.text),
        )
}

fn summary_page(state: &State, t: &rune::tok::Theme) -> El {
    let mut list = col().key("summary").gap(4.0).w_full();
    for category in &state.session.state.categories {
        if category.id == "income" {
            continue;
        }
        let amount =
            category_month_cents(&state.session.state, state.year, state.month, &category.id);
        list = list.child(
            row()
                .gap(12.0)
                .child(
                    text(format!("{}  {}", category.icon, category.name))
                        .w(180.0)
                        .font(type_::BODY)
                        .color(t.text),
                )
                .child(text(format_cents(amount)).font(type_::BODY).color(t.text)),
        );
    }
    list
}

fn register_page(state: &State, k: &Kit, t: &rune::tok::Theme) -> El {
    let mut list = col().key("register").gap(4.0).w_full();
    for entry in month_entries(&state.session.state, state.year, state.month) {
        let id = entry.id.clone();
        let delete_id = id.clone();
        let title = if entry.title.is_empty() {
            "(untitled)".into()
        } else {
            entry.title.clone()
        };
        list = list.child(
            row()
                .key(format!("entry-{id}"))
                .h(36.0)
                .gap(8.0)
                .center_y()
                .w_full()
                .child(
                    text(entry.date.clone())
                        .w(96.0)
                        .font(type_::BODY)
                        .color(t.text),
                )
                .child(
                    text(title)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate()
                        .font(type_::BODY)
                        .color(t.text),
                )
                .child(
                    text(category_name(&state.session.state, &entry.category_id))
                        .w(120.0)
                        .font(type_::LABEL)
                        .color(t.text_secondary),
                )
                .child(
                    text(format_cents(entry.amount_cents))
                        .w(96.0)
                        .font(type_::BODY)
                        .color(t.text),
                )
                .child(button(
                    "Delete",
                    ButtonVariant::Ghost,
                    k,
                    &format!("delete-{id}"),
                    move |state| delete_row(state, &delete_id),
                )),
        );
    }
    list
}

fn add_page(state: &State, k: &Kit) -> El {
    let mut categories = row().gap(6.0).w_full();
    for category in &state.session.state.categories {
        let id = category.id.clone();
        let chosen = id.clone();
        let variant = if state.form_category == id {
            ButtonVariant::Contrast
        } else {
            ButtonVariant::Ghost
        };
        categories = categories.child(button(
            &category.name,
            variant,
            k,
            &format!("cat-{id}"),
            move |state| state.form_category = chosen.clone(),
        ));
    }
    col()
        .key("add")
        .gap(8.0)
        .w_full()
        .child(field(
            &state.form_title,
            "Title",
            "Title",
            None,
            false,
            k,
            |state| &mut state.form_title,
        ))
        .child(field(
            &state.form_amount,
            "Amount (12.50)",
            "Amount",
            None,
            false,
            k,
            |state| &mut state.form_amount,
        ))
        .child(field(
            &state.form_date,
            "Date YYYY-MM-DD",
            "Date",
            None,
            false,
            k,
            |state| &mut state.form_date,
        ))
        .child(
            row()
                .gap(8.0)
                .child(button(
                    "Expense",
                    if state.form_kind == Kind::Expense {
                        ButtonVariant::Contrast
                    } else {
                        ButtonVariant::Ghost
                    },
                    k,
                    "kind-expense",
                    |state| {
                        state.form_kind = Kind::Expense;
                    },
                ))
                .child(button(
                    "Income",
                    if state.form_kind == Kind::Income {
                        ButtonVariant::Contrast
                    } else {
                        ButtonVariant::Ghost
                    },
                    k,
                    "kind-income",
                    |state| {
                        state.form_kind = Kind::Income;
                    },
                )),
        )
        .child(categories)
        .child(field(
            &state.form_note,
            "Note",
            "Note",
            None,
            false,
            k,
            |state| &mut state.form_note,
        ))
        .child(button(
            "Save",
            ButtonVariant::Contrast,
            k,
            "save-entry",
            save_entry,
        ))
}

fn import_page(state: &State, k: &Kit, t: &rune::tok::Theme) -> El {
    let sample = extra_roots()
        .into_iter()
        .next()
        .map(|root| format!("{root}/synthetic.qfx"))
        .unwrap_or_default();
    let help = format!(
        "Credits import as income. Payroll keywords are in Settings. Path is on this machine. Sample: {sample}"
    );
    col()
        .key("import")
        .gap(8.0)
        .w_full()
        .child(
            text(help)
                .font(type_::BODY)
                .color(t.text)
                .w_full()
                .max_lines(4),
        )
        .child(field(
            &state.import_path,
            "/home/you/Downloads/export.qfx",
            "QFX path",
            None,
            false,
            k,
            |state| &mut state.import_path,
        ))
        .child(button(
            "Import QFX",
            ButtonVariant::Contrast,
            k,
            "import-qfx",
            import_qfx,
        ))
        .child(
            text(state.import_log.clone())
                .key("import-log")
                .font(type_::BODY)
                .color(t.text)
                .w_full()
                .max_lines(12),
        )
}

fn budget_page(state: &State, k: &Kit, t: &rune::tok::Theme) -> El {
    let mut list = col().key("budgets").gap(6.0).w_full();
    for category in &state.session.state.categories {
        if category.id == "income" {
            continue;
        }
        let Some(edit) = state.budget_fields.get(&category.id) else {
            continue;
        };
        let id = category.id.clone();
        let edit_id = id.clone();
        let set_id = id.clone();
        list = list.child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    text(category.name.clone())
                        .w(140.0)
                        .font(type_::BODY)
                        .color(t.text),
                )
                .child(field(
                    edit,
                    "0.00",
                    &format!("{} budget", category.name),
                    Some(120.0),
                    false,
                    k,
                    move |state| state.budget_fields.get_mut(&edit_id).expect("budget field"),
                ))
                .child(button(
                    "Set",
                    ButtonVariant::Neutral,
                    k,
                    &format!("budget-set-{id}"),
                    move |state| set_budget(state, &set_id),
                )),
        );
    }
    list
}

fn settings_page(state: &State, k: &Kit, t: &rune::tok::Theme) -> El {
    let path = match &state.session.storage {
        Storage::Disk(paths) => paths.ledger.display().to_string(),
        Storage::Memory => "In-memory session".into(),
        Storage::Unavailable => "HOME is unset".into(),
    };
    col()
        .key("settings")
        .gap(8.0)
        .w_full()
        .child(field(
            &state.first_name,
            "First paycheck name",
            "First paycheck name",
            None,
            true,
            k,
            |state| &mut state.first_name,
        ))
        .child(field(
            &state.second_name,
            "Second paycheck name",
            "Second paycheck name",
            None,
            true,
            k,
            |state| &mut state.second_name,
        ))
        .child(
            text("Payroll keywords (comma-separated)")
                .font(type_::LABEL)
                .color(t.text),
        )
        .child(field(
            &state.keywords,
            "PAYROLL, DIRECT DEP",
            "Payroll keywords",
            None,
            true,
            k,
            |state| &mut state.keywords,
        ))
        .child(
            text(format!("Ledger file: {path}"))
                .key("ledger-path")
                .font(type_::BODY)
                .color(t.text_secondary)
                .w_full()
                .max_lines(2),
        )
}

fn field(
    edit: &TextEdit,
    placeholder: &str,
    label: &str,
    width: Option<f32>,
    submit: bool,
    k: &Kit,
    access: impl for<'a> Fn(&'a mut State) -> &'a mut TextEdit + 'static,
) -> El {
    let mut input = TextInput::new(edit)
        .placeholder(placeholder)
        .label(label)
        .width(width)
        .on_edit(access);
    if submit {
        input = input.on_submit(|state: &mut State, _| commit_settings(state));
    }
    input.build(k)
}

fn button(
    label: &str,
    variant: ButtonVariant,
    k: &Kit,
    key: &str,
    on_press: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(label)
        .variant(variant)
        .on_press(on_press)
        .build(k)
        .key(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn clicks_through_register_add_and_budget() {
        let mut ledger = Ledger::empty();
        let mut entry = NewEntry::manual(
            Kind::Expense,
            "2026-01-03".into(),
            475,
            "Coffee".into(),
            String::new(),
            "food".into(),
        );
        entry.id = "e1".into();
        ledger = add_entry(&ledger, entry);
        let mut state = State::fresh(Storage::Memory);
        state.session.state = ledger;
        state.year = 2026;
        state.month = 1;
        sync_editors(&mut state);

        let mut rt = Runtime::new(state, view, (960.0, 720.0), Env::new(true, 1.0));
        rt.frame(0.0);
        click(&mut rt, "page-register");
        assert_eq!(rt.state.page, Page::Register);
        click(&mut rt, "delete-e1");
        assert!(rt.state.session.state.entries.is_empty());

        click(&mut rt, "page-add");
        rt.state.form_title.replace_text("Tea");
        rt.state.form_amount.replace_text("2.50");
        rt.state.form_date.replace_text("2026-01-09");
        click(&mut rt, "save-entry");
        assert_eq!(rt.state.page, Page::Register);
        assert_eq!(rt.state.session.state.entries.len(), 1);
        assert_eq!(rt.state.session.state.entries[0].amount_cents, 250);
        assert_eq!(rt.state.session.state.entries[0].title, "Tea");

        click(&mut rt, "page-budget");
        rt.state
            .budget_fields
            .get_mut("food")
            .unwrap()
            .replace_text("40");
        click(&mut rt, "budget-set-food");
        assert_eq!(month_budget_cents(&rt.state.session.state, 2026, 1), 4_000);
        click(&mut rt, "month-next");
        assert_eq!(rt.state.month, 2);
    }

    fn click(rt: &mut Runtime<State>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..8 {
            rt.frame(1.0 / 60.0);
        }
    }
}
