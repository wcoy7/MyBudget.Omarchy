//! Line-oriented QFX/OFX statement parser.
//!
//! Tags are matched case-insensitively inside `<STMTTRN>` blocks. A row without
//! a posted date or an amount is dropped.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Txn {
    pub trntype: String,
    pub date: String,
    pub amount_cents: i64,
    pub fitid: String,
    pub name: String,
    pub memo: String,
}

pub fn parse(text: &str) -> Vec<Txn> {
    let mut out = Vec::new();
    let mut in_txn = false;
    let mut cur = RawTxn::default();
    for line in text.split('\n') {
        let line = line.trim().trim_end_matches('\r');
        if line.eq_ignore_ascii_case("<STMTTRN>") {
            in_txn = true;
            cur = RawTxn::default();
            continue;
        }
        if line.eq_ignore_ascii_case("</STMTTRN>") {
            if in_txn {
                if let Some(txn) = cur.finish() {
                    out.push(txn);
                }
            }
            in_txn = false;
            continue;
        }
        if !in_txn {
            continue;
        }
        take(line, "TRNTYPE", &mut cur.trntype);
        take(line, "DTPOSTED", &mut cur.date);
        take(line, "TRNAMT", &mut cur.amount);
        take(line, "FITID", &mut cur.fitid);
        take(line, "NAME", &mut cur.name);
        take(line, "MEMO", &mut cur.memo);
    }
    out
}

#[derive(Default)]
struct RawTxn {
    trntype: String,
    date: String,
    amount: String,
    fitid: String,
    name: String,
    memo: String,
}

impl RawTxn {
    fn finish(&self) -> Option<Txn> {
        let amount_cents = parse_ofx_amount(&self.amount)?;
        let date = parse_ofx_date(&self.date)?;
        Some(Txn {
            trntype: self.trntype.trim().to_ascii_uppercase(),
            date,
            amount_cents,
            fitid: self.fitid.trim().to_string(),
            name: self.name.trim().to_string(),
            memo: self.memo.trim().to_string(),
        })
    }
}

fn take(line: &str, tag: &str, dest: &mut String) {
    let open = format!("<{tag}>");
    if line.len() < open.len() || !line[..open.len()].eq_ignore_ascii_case(&open) {
        return;
    }
    let rest = line[open.len()..].split("</").next().unwrap_or("").trim();
    if !rest.is_empty() {
        *dest = rest.to_string();
    }
}

pub fn parse_ofx_date(raw: &str) -> Option<String> {
    let head = raw.trim().split('.').next().unwrap_or("");
    let digits: String = head.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return None;
    }
    let year: i64 = digits[0..4].parse().ok()?;
    let month: i64 = digits[4..6].parse().ok()?;
    let day: i64 = digits[6..8].parse().ok()?;
    if year == 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

pub fn parse_ofx_amount(raw: &str) -> Option<i64> {
    let cleaned = raw.trim().replace(',', "");
    if cleaned.is_empty() {
        return None;
    }
    let negative = cleaned.starts_with('-');
    let body = cleaned.trim_start_matches(['+', '-']);
    let mut parts = body.split('.');
    let whole = parse_whole(parts.next().unwrap_or(""))?;
    let frac = match parts.next() {
        Some(src) => {
            let digits: String = src.chars().filter(|c| c.is_ascii_digit()).take(2).collect();
            match digits.len() {
                1 => digits.parse::<i64>().unwrap_or(0) * 10,
                2 => digits.parse::<i64>().unwrap_or(0),
                _ => 0,
            }
        }
        None => 0,
    };
    let cents = whole.saturating_mul(100).saturating_add(frac);
    Some(if negative { -cents } else { cents })
}

fn parse_whole(text: &str) -> Option<i64> {
    if text.is_empty() {
        return Some(0);
    }
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_synthetic_fixture() {
        let txns = parse(include_str!("../fixtures/synthetic.qfx"));
        assert_eq!(txns.len(), 3);
        assert_eq!(txns[0].amount_cents, -475);
        assert_eq!(txns[0].date, "2026-01-03");
        assert_eq!(txns[0].name, "Example Cafe");
        assert_eq!(txns[0].trntype, "DEBIT");
        assert_eq!(txns[1].amount_cents, 210_000);
        assert_eq!(txns[1].fitid, "synth-pay-1");
        assert_eq!(txns[2].memo, "Refund");
    }

    #[test]
    fn dates_ignore_time_and_zone() {
        assert_eq!(
            parse_ofx_date("20260103120000.000").as_deref(),
            Some("2026-01-03")
        );
        assert_eq!(parse_ofx_date("20260103").as_deref(), Some("2026-01-03"));
        assert_eq!(parse_ofx_date("20261301"), None);
    }

    #[test]
    fn amounts_truncate_extra_fraction_digits() {
        assert_eq!(parse_ofx_amount("-4.75"), Some(-475));
        assert_eq!(parse_ofx_amount("1,234.50"), Some(123_450));
        assert_eq!(parse_ofx_amount("1.2"), Some(120));
        assert_eq!(parse_ofx_amount("1.239"), Some(123));
        assert_eq!(parse_ofx_amount(".50"), Some(50));
        assert_eq!(parse_ofx_amount("abc"), None);
    }
}
