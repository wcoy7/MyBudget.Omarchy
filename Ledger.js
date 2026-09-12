.pragma library

function empty() {
    return {
        schemaVersion: 1,
        settings: {
            firstPaycheckName: "First paycheck",
            secondPaycheckName: "Second paycheck",
            payrollKeywords: ["PAYROLL", "DIRECT DEP", "DIRECT DEPOSIT", "SALARY"],
            minImportCents: 0
        },
        categories: [
            { id: "food", name: "Food", icon: "🍽" },
            { id: "transport", name: "Transport", icon: "🚗" },
            { id: "entertainment", name: "Entertainment", icon: "🎬" },
            { id: "shopping", name: "Shopping", icon: "🛒" },
            { id: "utilities", name: "Utilities", icon: "⚡" },
            { id: "income", name: "Income", icon: "💵" },
            { id: "general", name: "General", icon: "🏷" }
        ],
        accounts: [],
        entries: [],
        budgets: [],
        titleMappings: {}
    }
}

function load(text) {
    return loadStrict(text).state
}

function isEffectivelyEmpty(state) {
    return !state || !state.entries || state.entries.length === 0
}

function entryCount(state) {
    return (state && state.entries) ? state.entries.length : 0
}

function loadStrict(text) {
    if (!text || !String(text).trim())
        return { ok: true, state: empty(), emptyFile: true }
    try {
        var data = JSON.parse(text)
        if (!data || typeof data !== "object" || Array.isArray(data))
            return { ok: false, state: empty(), error: "Ledger root must be a JSON object" }
        var base = empty()
        if (data.settings && typeof data.settings === "object")
            base.settings = Object.assign(base.settings, data.settings)
        if (!Array.isArray(base.settings.payrollKeywords))
            base.settings.payrollKeywords = empty().settings.payrollKeywords
        if (typeof base.settings.minImportCents !== "number")
            base.settings.minImportCents = 0
        if (Array.isArray(data.categories) && data.categories.length)
            base.categories = data.categories
        if (Array.isArray(data.accounts))
            base.accounts = data.accounts
        if (Array.isArray(data.entries))
            base.entries = data.entries.map(normalizeEntry).filter(function (e) { return e !== null })
        if (Array.isArray(data.budgets))
            base.budgets = data.budgets
        if (data.titleMappings && typeof data.titleMappings === "object")
            base.titleMappings = data.titleMappings
        return { ok: true, state: base, emptyFile: false }
    } catch (e) {
        return { ok: false, state: empty(), error: "Invalid JSON ledger" }
    }
}

function normalizeEntry(raw) {
    if (!raw || typeof raw !== "object")
        return null
    var amount = parseInt(raw.amountCents, 10)
    if (isNaN(amount))
        return null
    var kind = raw.kind === "income" ? "income" : "expense"
    return {
        id: String(raw.id || uuid()),
        kind: kind,
        date: String(raw.date || today()),
        amountCents: Math.abs(amount),
        title: String(raw.title || ""),
        note: String(raw.note || ""),
        originalNote: String(raw.originalNote || raw.note || ""),
        categoryId: String(raw.categoryId || (kind === "income" ? "income" : "general")),
        accountId: String(raw.accountId || ""),
        parentId: String(raw.parentId || ""),
        paycheckHalf: raw.paycheckHalf === "second" ? "second" : (raw.paycheckHalf === "first" ? "first" : ""),
        importId: String(raw.importId || "")
    }
}

function dump(state) {
    return JSON.stringify(state, null, 2) + "\n"
}

function uuid() {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function (c) {
        var r = Math.random() * 16 | 0
        var v = c === "x" ? r : (r & 0x3 | 0x8)
        return v.toString(16)
    })
}

function today() {
    var d = new Date()
    return isoDate(d.getFullYear(), d.getMonth() + 1, d.getDate())
}

function isoDate(y, m, d) {
    return pad(y, 4) + "-" + pad(m, 2) + "-" + pad(d, 2)
}

function pad(n, width) {
    var s = String(n)
    while (s.length < width)
        s = "0" + s
    return s
}

function parseDate(iso) {
    var p = String(iso || "").split("-")
    if (p.length < 3)
        return null
    var year = parseInt(p[0], 10)
    var month = parseInt(p[1], 10)
    var day = parseInt(p[2], 10)
    if (!year || month < 1 || month > 12 || day < 1 || day > 31)
        return null
    return { year: year, month: month, day: day }
}

function inMonth(iso, year, month) {
    var p = parseDate(iso)
    return p && p.year === year && p.month === month
}

function paycheckHalfForDate(iso) {
    var p = parseDate(iso)
    if (!p)
        return "first"
    return p.day <= 15 ? "first" : "second"
}

function parseDollars(input) {
    var trimmed = String(input || "").trim().replace(/[$,\s]/g, "")
    if (!trimmed)
        return null
    var negative = trimmed.charAt(0) === "-" || trimmed.charAt(0) === "("
    var body = trimmed.replace(/^[-(]/, "").replace(/\)$/, "")
    if (!body || !/^\d+(\.\d{1,2})?$/.test(body))
        return null
    var parts = body.split(".")
    var whole = parseInt(parts[0], 10)
    var frac = 0
    if (parts.length === 2) {
        frac = parts[1].length === 1 ? parseInt(parts[1], 10) * 10 : parseInt(parts[1], 10)
    }
    var cents = whole * 100 + frac
    return negative ? -cents : cents
}

function formatCents(cents) {
    var n = parseInt(cents, 10) || 0
    var neg = n < 0
    var abs = Math.abs(n)
    var dollars = Math.floor(abs / 100)
    var rem = abs % 100
    var body = String(dollars).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    return (neg ? "-$" : "$") + body + "." + pad(rem, 2)
}

function isLeaf(entry, entries) {
    if (!entry)
        return false
    for (var i = 0; i < entries.length; i++) {
        if (entries[i].parentId === entry.id)
            return false
    }
    return true
}

function countsTowardExpenseTotals(entry, entries) {
    return entry && entry.kind === "expense" && isLeaf(entry, entries)
}

function monthExpenseCents(state, year, month) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (countsTowardExpenseTotals(e, entries) && inMonth(e.date, year, month))
            sum += Math.abs(e.amountCents || 0)
    }
    return sum
}

function monthIncomeCents(state, year, month) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (e.kind === "income" && inMonth(e.date, year, month))
            sum += Math.abs(e.amountCents || 0)
    }
    return sum
}

function monthBudgetCents(state, year, month) {
    var sum = 0
    var budgets = state.budgets || []
    for (var i = 0; i < budgets.length; i++) {
        var b = budgets[i]
        if (b.year === year && b.month === month && b.categoryId !== "income")
            sum += Math.abs(b.amountCents || 0)
    }
    return sum
}

function categoryMonthCents(state, year, month, categoryId) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (countsTowardExpenseTotals(e, entries) && e.categoryId === categoryId && inMonth(e.date, year, month))
            sum += Math.abs(e.amountCents || 0)
    }
    return sum
}

function monthEntries(state, year, month) {
    var out = []
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (!inMonth(e.date, year, month))
            continue
        // Hide parent expense shells when children exist so amounts are not shown twice.
        if (e.kind === "expense" && !isLeaf(e, entries))
            continue
        out.push(e)
    }
    out.sort(function (a, b) {
        if (a.date === b.date)
            return (b.title || "").localeCompare(a.title || "")
        return a.date < b.date ? 1 : -1
    })
    return out
}

function categoryName(state, id) {
    var cats = state.categories || []
    for (var i = 0; i < cats.length; i++) {
        if (cats[i].id === id)
            return cats[i].name
    }
    return "—"
}

function addEntry(state, fields) {
    var next = clone(state)
    next.entries = (state.entries || []).slice()
    var kind = fields.kind === "income" ? "income" : "expense"
    var date = fields.date || today()
    if (!parseDate(date))
        date = today()
    next.entries.push({
        id: fields.id || uuid(),
        kind: kind,
        date: date,
        amountCents: Math.abs(fields.amountCents || 0),
        title: fields.title || "",
        note: fields.note || "",
        originalNote: fields.originalNote || fields.note || "",
        categoryId: fields.categoryId || (kind === "income" ? "income" : "general"),
        accountId: fields.accountId || "",
        parentId: fields.parentId || "",
        paycheckHalf: fields.paycheckHalf || (kind === "expense" || fields.isPayroll ? paycheckHalfForDate(date) : ""),
        importId: fields.importId || ""
    })
    return next
}

function deleteEntry(state, id) {
    var next = clone(state)
    next.entries = []
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        if (entries[i].id !== id && entries[i].parentId !== id)
            next.entries.push(entries[i])
    }
    return next
}

function upsertBudget(state, year, month, categoryId, cents) {
    var next = clone(state)
    next.budgets = []
    var found = false
    var budgets = state.budgets || []
    for (var i = 0; i < budgets.length; i++) {
        var b = budgets[i]
        if (b.year === year && b.month === month && b.categoryId === categoryId) {
            next.budgets.push({ id: b.id, year: year, month: month, categoryId: categoryId, amountCents: Math.abs(cents) })
            found = true
        } else {
            next.budgets.push(b)
        }
    }
    if (!found)
        next.budgets.push({ id: uuid(), year: year, month: month, categoryId: categoryId, amountCents: Math.abs(cents) })
    return next
}

function clone(state) {
    return JSON.parse(JSON.stringify(state))
}

function containsAny(hay, needles) {
    for (var i = 0; i < needles.length; i++) {
        var needle = String(needles[i] || "").toUpperCase()
        if (needle && hay.indexOf(needle) !== -1)
            return true
    }
    return false
}

function matchCategoryId(state, hayUpper) {
    var cats = state.categories || []
    for (var i = 0; i < cats.length; i++) {
        var name = String(cats[i].name || "").toLowerCase()
        if (name === "food" && containsAny(hayUpper, ["GROCERY", "STARBUCKS", "MCDONALD", "RESTAURANT", "CAFE"]))
            return cats[i].id
        if (name === "transport" && containsAny(hayUpper, ["GAS", "FUEL", "UBER", "LYFT", "PARKING", "TRANSIT"]))
            return cats[i].id
        if (name === "entertainment" && containsAny(hayUpper, ["NETFLIX", "SPOTIFY", "MOVIE", "CINEMA"]))
            return cats[i].id
        if (name === "shopping" && containsAny(hayUpper, ["AMAZON", "WALMART", "TARGET"]))
            return cats[i].id
        if (name === "utilities" && containsAny(hayUpper, ["ELECTRIC", "WATER", "INTERNET", "PHONE", "UTILITY"]))
            return cats[i].id
    }
    return "general"
}

function classifyTxn(txn, payrollKeywords) {
    var trntype = String(txn.trntype || "").toUpperCase()
    var amount = txn.amountCents || 0
    var hay = (String(txn.name || "") + " " + String(txn.memo || "")).toUpperCase()
    var isCredit = amount > 0 || trntype === "CREDIT" || trntype === "DEP" || trntype === "DIRECTDEP"
    var isDebit = amount < 0 || trntype === "DEBIT" || trntype === "WITHDRAWAL" || trntype === "CHECK" || trntype === "FEE" || trntype === "PAYMENT"
    // Amount sign wins when present; never treat a credit as an expense.
    if (amount > 0)
        isCredit = true, isDebit = false
    else if (amount < 0)
        isDebit = true, isCredit = false
    else if (isCredit)
        isDebit = false
    else
        isDebit = true

    var kind = isCredit ? "income" : "expense"
    var isPayroll = kind === "income" && containsAny(hay, payrollKeywords)
    return { kind: kind, isPayroll: isPayroll, hay: hay }
}

function importTxns(state, txns) {
    var next = clone(state)
    if (!next.titleMappings)
        next.titleMappings = {}
    next.entries = (state.entries || []).slice()
    var seen = {}
    var i
    for (i = 0; i < next.entries.length; i++) {
        if (next.entries[i].importId)
            seen[next.entries[i].importId] = true
    }
    var keywords = (next.settings.payrollKeywords || []).map(function (k) { return String(k).toUpperCase() })
    var minCents = next.settings.minImportCents || 0
    var added = 0
    var skipped = 0
    var lines = []

    for (i = 0; i < txns.length; i++) {
        var t = txns[i]
        var fitid = t.fitid || ""
        if (fitid && seen[fitid]) {
            skipped += 1
            continue
        }
        var abs = Math.abs(t.amountCents || 0)
        if (abs < minCents)
            continue
        if (abs === 0)
            continue

        var mapped = next.titleMappings[String(t.name || "").toLowerCase()]
        var title = (mapped && mapped.title) ? mapped.title : (t.name || "Imported")
        var classified = classifyTxn(t, keywords)
        var kind = classified.kind
        var categoryId = "general"
        if (mapped && mapped.categoryId)
            categoryId = mapped.categoryId
        else if (kind === "income")
            categoryId = "income"
        else
            categoryId = matchCategoryId(next, classified.hay)

        next.entries.push({
            id: uuid(),
            kind: kind,
            date: t.date,
            amountCents: abs,
            title: title,
            note: t.memo || "",
            originalNote: t.memo || "",
            categoryId: categoryId,
            accountId: "",
            parentId: "",
            paycheckHalf: classified.isPayroll || kind === "expense" ? paycheckHalfForDate(t.date) : "",
            importId: fitid
        })
        if (fitid)
            seen[fitid] = true
        added += 1
        lines.push("Added " + kind + ": " + formatCents(abs) + " — " + title)
    }
    lines.unshift("Imported " + added + " transactions" + (skipped ? (" (" + skipped + " duplicates skipped)") : ""))
    return { state: next, added: added, skipped: skipped, lines: lines }
}

function monthLabel(year, month) {
    var names = ["January", "February", "March", "April", "May", "June",
                 "July", "August", "September", "October", "November", "December"]
    return names[month - 1] + " " + year
}

function addMonths(year, month, delta) {
    var m = month + delta
    var y = year
    while (m > 12) { m -= 12; y += 1 }
    while (m < 1) { m += 12; y -= 1 }
    return { year: y, month: m }
}

function expandHome(path, home) {
    var p = String(path || "").trim()
    if (!p)
        return ""
    if (p === "~")
        return home
    if (p.indexOf("~/") === 0)
        return home + p.slice(1)
    return p
}

function normalizePath(path) {
    var p = String(path || "").trim()
    if (!p)
        return ""
    p = p.replace(/\/+/g, "/")
    var parts = p.split("/")
    var out = []
    for (var i = 0; i < parts.length; i++) {
        var part = parts[i]
        if (part === "" && i === 0) {
            out.push("")
            continue
        }
        if (part === "" || part === ".")
            continue
        if (part === "..") {
            if (out.length > 1)
                out.pop()
            continue
        }
        out.push(part)
    }
    if (out.length === 1 && out[0] === "")
        return "/"
    return out.join("/") || "/"
}

function isAllowedImportPath(path, home) {
    var homePath = normalizePath(home || "")
    if (!homePath || homePath === "/")
        return { ok: false, reason: "HOME is unset" }
    var expanded = normalizePath(expandHome(path, homePath))
    if (!expanded || expanded.charAt(0) !== "/")
        return { ok: false, reason: "Use an absolute path or ~/..." }
    if (expanded.indexOf("\0") !== -1)
        return { ok: false, reason: "Invalid path" }

    var allowedRoots = [
        homePath + "/Downloads",
        homePath + "/Download",
        homePath + "/documents",
        homePath + "/Documents",
        homePath + "/.config/omarchy/plugins/mybudget.expenses/fixtures"
    ]
    for (var i = 0; i < allowedRoots.length; i++) {
        var root = normalizePath(allowedRoots[i])
        if (expanded === root || expanded.indexOf(root + "/") === 0) {
            var lower = expanded.toLowerCase()
            if (!(lower.indexOf(".qfx", lower.length - 4) !== -1 || lower.indexOf(".ofx", lower.length - 4) !== -1))
                return { ok: false, reason: "Only .qfx / .ofx files are allowed" }
            return { ok: true, path: expanded }
        }
    }
    return { ok: false, reason: "Import is limited to ~/Downloads, ~/Documents, or the plugin fixtures folder" }
}
