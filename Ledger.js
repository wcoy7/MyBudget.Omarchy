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
    if (!text || !String(text).trim())
        return empty()
    try {
        var data = JSON.parse(text)
        var base = empty()
        if (!data || typeof data !== "object")
            return base
        if (data.settings)
            base.settings = Object.assign(base.settings, data.settings)
        if (data.categories && data.categories.length)
            base.categories = data.categories
        if (data.accounts)
            base.accounts = data.accounts
        if (data.entries)
            base.entries = data.entries
        if (data.budgets)
            base.budgets = data.budgets
        if (data.titleMappings)
            base.titleMappings = data.titleMappings
        return base
    } catch (e) {
        return empty()
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
    return { year: parseInt(p[0], 10), month: parseInt(p[1], 10), day: parseInt(p[2], 10) }
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
    if (!body)
        return null
    var parts = body.split(".")
    if (parts.length > 2)
        return null
    var whole = parts[0] === "" ? 0 : parseInt(parts[0], 10)
    if (isNaN(whole))
        return null
    var frac = 0
    if (parts.length === 2) {
        if (parts[1].length > 2)
            return null
        if (parts[1].length === 1)
            frac = parseInt(parts[1], 10) * 10
        else if (parts[1].length === 2)
            frac = parseInt(parts[1], 10)
        if (isNaN(frac))
            return null
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
    for (var i = 0; i < entries.length; i++) {
        if (entries[i].parentId === entry.id)
            return false
    }
    return true
}

function monthExpenseCents(state, year, month) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (e.kind === "expense" && inMonth(e.date, year, month) && isLeaf(e, entries))
            sum += e.amountCents
    }
    return sum
}

function monthIncomeCents(state, year, month) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (e.kind === "income" && inMonth(e.date, year, month))
            sum += e.amountCents
    }
    return sum
}

function monthBudgetCents(state, year, month) {
    var sum = 0
    var budgets = state.budgets || []
    for (var i = 0; i < budgets.length; i++) {
        var b = budgets[i]
        if (b.year === year && b.month === month)
            sum += b.amountCents
    }
    return sum
}

function categoryMonthCents(state, year, month, categoryId) {
    var sum = 0
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        var e = entries[i]
        if (e.kind === "expense" && e.categoryId === categoryId && inMonth(e.date, year, month) && isLeaf(e, entries))
            sum += e.amountCents
    }
    return sum
}

function monthEntries(state, year, month) {
    var out = []
    var entries = state.entries || []
    for (var i = 0; i < entries.length; i++) {
        if (inMonth(entries[i].date, year, month))
            out.push(entries[i])
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
    next.entries.push({
        id: fields.id || uuid(),
        kind: fields.kind || "expense",
        date: fields.date || today(),
        amountCents: Math.abs(fields.amountCents || 0),
        title: fields.title || "",
        note: fields.note || "",
        originalNote: fields.originalNote || fields.note || "",
        categoryId: fields.categoryId || "general",
        accountId: fields.accountId || "",
        parentId: fields.parentId || "",
        paycheckHalf: fields.paycheckHalf || paycheckHalfForDate(fields.date || today()),
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
            next.budgets.push({ id: b.id, year: year, month: month, categoryId: categoryId, amountCents: cents })
            found = true
        } else {
            next.budgets.push(b)
        }
    }
    if (!found)
        next.budgets.push({ id: uuid(), year: year, month: month, categoryId: categoryId, amountCents: cents })
    return next
}

function clone(state) {
    return JSON.parse(JSON.stringify(state))
}

function containsAny(hay, needles) {
    for (var i = 0; i < needles.length; i++) {
        if (needles[i] && hay.indexOf(needles[i]) !== -1)
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
        if (name === "shopping" && containsAny(hayUpper, ["AMAZON", "WALMART", "TARGET", "STORE"]))
            return cats[i].id
        if (name === "utilities" && containsAny(hayUpper, ["ELECTRIC", "WATER", "INTERNET", "PHONE", "UTILITY"]))
            return cats[i].id
    }
    return "general"
}

function importTxns(state, txns) {
    var next = clone(state)
    next.entries = (state.entries || []).slice()
    var seen = {}
    var i
    for (i = 0; i < next.entries.length; i++) {
        if (next.entries[i].importId)
            seen[next.entries[i].importId] = true
    }
    var keywords = (next.settings.payrollKeywords || []).map(function (k) { return String(k).toUpperCase() })
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
        var abs = Math.abs(t.amountCents)
        if (abs < (next.settings.minImportCents || 0))
            continue
        var mapped = next.titleMappings[String(t.name || "").toLowerCase()]
        var title = (mapped && mapped.title) ? mapped.title : t.name
        var hay = (t.name + " " + t.memo).toUpperCase()
        var isCredit = t.trntype === "CREDIT" || t.amountCents > 0
        var isPayroll = isCredit && containsAny(hay, keywords)
        var kind = isCredit ? "income" : "expense"
        var categoryId = "general"
        if (mapped && mapped.categoryId)
            categoryId = mapped.categoryId
        else if (isCredit)
            categoryId = "income"
        else
            categoryId = matchCategoryId(next, hay)

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
            paycheckHalf: isPayroll ? paycheckHalfForDate(t.date) : (kind === "expense" ? paycheckHalfForDate(t.date) : ""),
            importId: fitid
        })
        if (fitid)
            seen[fitid] = true
        added += 1
        lines.push("Added " + kind + ": " + formatCents(abs) + " — " + title)
    }
    lines.unshift("Imported " + added + " transactions")
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
