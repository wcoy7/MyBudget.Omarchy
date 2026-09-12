.pragma library

function parseOfxDate(raw) {
    var cleaned = String(raw || "").trim()
    var head = cleaned.split(".")[0]
    var digits = head.replace(/\D/g, "")
    if (digits.length < 8)
        return null
    var y = parseInt(digits.slice(0, 4), 10)
    var m = parseInt(digits.slice(4, 6), 10)
    var d = parseInt(digits.slice(6, 8), 10)
    if (!y || m < 1 || m > 12 || d < 1 || d > 31)
        return null
    return pad(y, 4) + "-" + pad(m, 2) + "-" + pad(d, 2)
}

function parseOfxAmount(raw) {
    var cleaned = String(raw || "").trim().replace(/,/g, "")
    if (!cleaned)
        return null
    var negative = cleaned.charAt(0) === "-"
    var body = cleaned.replace(/^[+-]/, "")
    var parts = body.split(".")
    var whole = parts[0] === "" ? 0 : parseInt(parts[0], 10)
    if (isNaN(whole))
        return null
    var frac = 0
    if (parts.length > 1) {
        var fracDigits = (parts[1] || "").replace(/\D/g, "").slice(0, 2)
        if (fracDigits.length === 1)
            frac = parseInt(fracDigits, 10) * 10
        else if (fracDigits.length === 2)
            frac = parseInt(fracDigits, 10)
    }
    var cents = whole * 100 + frac
    return negative ? -cents : cents
}

function parse(text) {
    var lines = String(text || "").split(/\r?\n/)
    var out = []
    var inTxn = false
    var cur = emptyTxn()

    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim()
        if (line.toUpperCase() === "<STMTTRN>") {
            inTxn = true
            cur = emptyTxn()
            continue
        }
        if (line.toUpperCase() === "</STMTTRN>") {
            if (inTxn) {
                var txn = finish(cur)
                if (txn)
                    out.push(txn)
            }
            inTxn = false
            continue
        }
        if (!inTxn)
            continue
        take(line, "TRNTYPE", cur, "trntype")
        take(line, "DTPOSTED", cur, "date")
        take(line, "TRNAMT", cur, "amount")
        take(line, "FITID", cur, "fitid")
        take(line, "NAME", cur, "name")
        take(line, "MEMO", cur, "memo")
    }
    return out
}

function emptyTxn() {
    return { trntype: "", date: "", amount: "", fitid: "", name: "", memo: "" }
}

function take(line, tag, dest, key) {
    var open = "<" + tag + ">"
    if (line.length < open.length)
        return
    if (line.substring(0, open.length).toUpperCase() !== open.toUpperCase())
        return
    var rest = line.substring(open.length).split("</")[0].trim()
    if (rest)
        dest[key] = rest
}

function finish(cur) {
    var cents = parseOfxAmount(cur.amount)
    var date = parseOfxDate(cur.date)
    if (cents === null || !date)
        return null
    return {
        trntype: String(cur.trntype || "").trim().toUpperCase(),
        date: date,
        amountCents: cents,
        fitid: String(cur.fitid || "").trim(),
        name: String(cur.name || "").trim(),
        memo: String(cur.memo || "").trim()
    }
}

function pad(n, width) {
    var s = String(n)
    while (s.length < width)
        s = "0" + s
    return s
}
