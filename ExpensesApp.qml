import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Ledger.js" as Ledger
import "Qfx.js" as Qfx

// Shared ledger UI used by the bar panel and the fullscreen overlay.
Item {
    id: root

    property color foreground: Color.menu.text
    property var state: Ledger.empty()
    property int year: new Date().getFullYear()
    property int month: new Date().getMonth() + 1
    property string page: "summary"
    property string status: ""
    property string formTitle: ""
    property string formAmount: ""
    property string formDate: Ledger.today()
    property string formKind: "expense"
    property string formCategory: "general"
    property string formNote: ""
    property string importPath: ""
    property string importLog: ""
    property string keywordText: ""
    property string versionLabel: "MyExpenses v0.1.4"

    readonly property string barLabel: Ledger.formatCents(Ledger.monthExpenseCents(state, year, month))
    readonly property string homeDir: {
        var h = Ledger.normalizePath(Quickshell.env("HOME") || "")
        if (!h || h === "/" || h.charAt(0) !== "/")
            return ""
        return h
    }
    readonly property string dataDir: homeDir ? (homeDir + "/.local/share/expenses") : ""
    readonly property string dataPath: dataDir ? (dataDir + "/ledger.json") : ""
    readonly property string backupPath: dataPath ? (dataPath + ".bak") : ""
    readonly property alias content: body
    readonly property int ledgerMaxBytes: 8 * 1024 * 1024
    readonly property int qfxMaxBytes: 16 * 1024 * 1024
    readonly property string ioPython: "/usr/bin/python3"
    readonly property var helperEnv: ({
        LC_ALL: "C",
        PYTHONIOENCODING: "utf-8"
    })

    // When true, we are showing an empty ledger because the on-disk file was
    // missing/corrupt. Never overwrite disk with that empty seed.
    property bool blockEmptyPersist: false
    property bool loadingBackup: false
    property bool ioActive: false
    property bool ioSawStart: false
    property bool suppressWatch: false
    property int watchHold: 0
    property var ioQueue: []
    property var ioJob: null
    property string ioPendingStdin: ""

    Component.onCompleted: reloadFromDisk()

    function acceptLoadedState(next, message) {
        state = next
        keywordText = (next.settings.payrollKeywords || []).join(", ")
        blockEmptyPersist = false
        if (message)
            status = message
    }

    function seedEmptyInMemory(message) {
        state = Ledger.empty()
        keywordText = (state.settings.payrollKeywords || []).join(", ")
        blockEmptyPersist = true
        status = message
    }

    function fileUrlToPath(url) {
        var s = String(url || "")
        if (s.indexOf("file://") === 0)
            s = decodeURIComponent(s.slice(7))
        return s
    }

    function utf8ByteLength(text) {
        var s = String(text)
        var n = 0
        for (var i = 0; i < s.length; i++) {
            var c = s.charCodeAt(i)
            if (c <= 0x7f)
                n += 1
            else if (c <= 0x7ff)
                n += 2
            else if (c >= 0xd800 && c <= 0xdbff) {
                n += 4
                i += 1
            } else {
                n += 3
            }
        }
        return n
    }

    function encodePayload(text) {
        return utf8ByteLength(text) + "\n" + text
    }

    function helperCommand(args) {
        var helper = fileUrlToPath(Qt.resolvedUrl("ledger_io.py"))
        if (!helper || helper.charAt(0) !== "/" || helper.substring(helper.length - 13) !== "/ledger_io.py")
            return []
        return [ioPython, "-I", "-B", helper].concat(args)
    }

    function readCommand(path, maxBytes) {
        return helperCommand(["read", "--path", path, "--max-bytes", String(maxBytes)])
    }

    function writeCommand(path, maxBytes) {
        return helperCommand(["write", "--path", path, "--max-bytes", String(maxBytes)])
    }

    function saveCommand(path, backup, maxBytes) {
        return helperCommand(["save", "--path", path, "--backup", backup, "--max-bytes", String(maxBytes)])
    }

    function ioMessage(code, stderrText) {
        var msg = String(stderrText || "").trim()
        if (msg)
            return msg
        if (code === 2)
            return "file not found"
        if (code === 3)
            return "file exceeds size limit"
        if (code === 4)
            return "file is not a regular file"
        if (code === 5)
            return "file failed ownership checks"
        if (code === 6)
            return "invalid path"
        return "I/O failed"
    }

    function enqueueIo(job) {
        var q = ioQueue.slice()
        q.push(job)
        ioQueue = q
        pumpIo()
    }

    function holdWatch() {
        watchHold += 1
        suppressWatch = true
    }

    function releaseWatch() {
        watchHold -= 1
        if (watchHold > 0)
            return
        watchHold = 0
        Qt.callLater(function () {
            if (root.watchHold === 0)
                root.suppressWatch = false
        })
    }

    function pumpIo() {
        if (ioActive || ioQueue.length === 0)
            return
        var q = ioQueue.slice()
        var job = q.shift()
        ioQueue = q
        if (!job || !job.command || job.command.length < 4) {
            handleIo(job, 1, "", "I/O helper is not available")
            pumpIo()
            return
        }
        ioActive = true
        ioSawStart = false
        ioJob = job
        ioPendingStdin = job.stdin || ""
        ioProc.stdinEnabled = ioPendingStdin !== ""
        ioProc.exec({
            command: job.command,
            clearEnvironment: true,
            environment: helperEnv,
            workingDirectory: "/"
        })
    }

    function finishIo(code, stdoutText, stderrText) {
        var job = ioJob
        ioActive = false
        ioJob = null
        ioPendingStdin = ""
        handleIo(job, code, stdoutText, stderrText)
        pumpIo()
    }

    function handleIo(job, code, stdoutText, stderrText) {
        if (!job)
            return
        if (job.kind === "load-ledger") {
            if (code !== 0) {
                tryLoadBackup((code === 2 ? "No ledger file" : ioMessage(code, stderrText)) + ".")
                return
            }
            var loaded = Ledger.loadStrict(stdoutText)
            if (!loaded.ok) {
                tryLoadBackup(loaded.error + ".")
                return
            }
            acceptLoadedState(loaded.state, "")
            return
        }
        if (job.kind === "load-backup") {
            loadingBackup = false
            if (code !== 0) {
                seedEmptyInMemory("Ledger missing/damaged and no backup found. Showing empty until you save an entry.")
                return
            }
            var bak = Ledger.loadStrict(stdoutText)
            if (bak.ok && !Ledger.isEffectivelyEmpty(bak.state)) {
                acceptLoadedState(bak.state, "Restored from ledger.json.bak")
                holdWatch()
                enqueueIo({
                    kind: "restore-write",
                    command: writeCommand(dataPath, ledgerMaxBytes),
                    stdin: encodePayload(Ledger.dump(state))
                })
                return
            }
            seedEmptyInMemory("Ledger missing/damaged and backup unavailable. Showing empty until you save an entry.")
            return
        }
        if (job.kind === "save" || job.kind === "restore-write") {
            releaseWatch()
            if (code !== 0) {
                status = (job.kind === "save" ? "Could not save ledger: " : "Restored in memory but could not rewrite ledger: ")
                        + ioMessage(code, stderrText)
            }
            return
        }
        if (job.kind === "read-qfx") {
            if (code !== 0) {
                importLog = "Could not read file: " + ioMessage(code, stderrText)
                return
            }
            var imported = Ledger.importTxns(state, Qfx.parse(stdoutText))
            applyState(imported.state, "Imported " + imported.added)
            importLog = imported.lines.join("\n")
        }
    }

    function persist() {
        if (!dataPath) {
            status = "HOME is unset; cannot save ledger."
            return false
        }
        if (blockEmptyPersist && Ledger.isEffectivelyEmpty(state)) {
            status = "Not saving empty ledger over a missing/damaged file. Add an entry first."
            return false
        }
        holdWatch()
        enqueueIo({
            kind: "save",
            command: saveCommand(dataPath, backupPath, ledgerMaxBytes),
            stdin: encodePayload(Ledger.dump(state))
        })
        blockEmptyPersist = false
        return true
    }

    function reloadFromDisk() {
        loadingBackup = false
        if (!dataPath) {
            seedEmptyInMemory("HOME is unset. Showing empty ledger.")
            return
        }
        enqueueIo({
            kind: "load-ledger",
            command: readCommand(dataPath, ledgerMaxBytes)
        })
    }

    function tryLoadBackup(reason) {
        loadingBackup = true
        status = reason + " Trying backup…"
        if (!backupPath) {
            loadingBackup = false
            seedEmptyInMemory("Ledger missing/damaged and no backup found. Showing empty until you save an entry.")
            return
        }
        enqueueIo({
            kind: "load-backup",
            command: readCommand(backupPath, ledgerMaxBytes)
        })
    }

    function applyState(next, message) {
        state = next
        if (!persist())
            return
        if (message)
            status = message
    }

    Process {
        id: ioProc
        clearEnvironment: true
        environment: root.helperEnv
        workingDirectory: "/"
        stdinEnabled: false
        stdout: StdioCollector { id: ioStdout }
        stderr: StdioCollector { id: ioStderr }
        onStarted: {
            root.ioSawStart = true
            if (root.ioPendingStdin !== "") {
                write(root.ioPendingStdin)
                stdinEnabled = false
            }
        }
        onExited: function (exitCode, exitStatus) {
            if (!root.ioActive)
                return
            root.finishIo(exitCode, ioStdout.text, ioStderr.text)
        }
        onRunningChanged: {
            if (!running && root.ioActive && !root.ioSawStart)
                root.finishIo(1, "", "helper failed to start")
        }
    }

    FileView {
        id: ledgerWatch
        path: root.dataPath
        preload: false
        watchChanges: true
        printErrors: false
        onFileChanged: {
            if (root.suppressWatch || root.ioActive || root.loadingBackup)
                return
            root.reloadFromDisk()
        }
    }

    Column {
        id: body
        width: parent.width
        spacing: Style.space(8)

        Row {
            spacing: Style.space(8)
            Button {
                text: "‹"
                onClicked: {
                    var n = Ledger.addMonths(root.year, root.month, -1)
                    root.year = n.year
                    root.month = n.month
                }
            }
            Text {
                text: Ledger.monthLabel(root.year, root.month)
                color: root.foreground
                font.pixelSize: Style.font.subtitle
                font.bold: true
                anchors.verticalCenter: parent.verticalCenter
            }
            Button {
                text: "›"
                onClicked: {
                    var n = Ledger.addMonths(root.year, root.month, 1)
                    root.year = n.year
                    root.month = n.month
                }
            }
            Text {
                text: root.status
                color: root.foreground
                opacity: 0.7
                font.pixelSize: Style.font.bodySmall
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        Row {
            spacing: Style.space(6)
            Repeater {
                model: [
                    { id: "summary", label: "Summary" },
                    { id: "register", label: "Register" },
                    { id: "add", label: "Add" },
                    { id: "import", label: "Import" },
                    { id: "budget", label: "Budget" },
                    { id: "settings", label: "Settings" }
                ]
                Button {
                    text: modelData.label
                    selected: root.page === modelData.id
                    onClicked: root.page = modelData.id
                }
            }
        }

        Text {
            text: root.versionLabel
            color: root.foreground
            opacity: 0.55
            font.pixelSize: Style.font.bodySmall
        }

        Row {
            spacing: Style.space(16)
            Text {
                text: "Spent  " + Ledger.formatCents(Ledger.monthExpenseCents(root.state, root.year, root.month))
                color: root.foreground
            }
            Text {
                text: "Income  " + Ledger.formatCents(Ledger.monthIncomeCents(root.state, root.year, root.month))
                color: root.foreground
            }
            Text {
                text: "Budget  " + Ledger.formatCents(Ledger.monthBudgetCents(root.state, root.year, root.month))
                color: root.foreground
            }
        }

        Loader {
            width: parent.width
            height: Math.max(root.height - Style.space(160), Style.space(420))
            sourceComponent: {
                if (root.page === "register")
                    return registerPage
                if (root.page === "add")
                    return addPage
                if (root.page === "import")
                    return importPage
                if (root.page === "budget")
                    return budgetPage
                if (root.page === "settings")
                    return settingsPage
                return summaryPage
            }
        }
    }

    Component {
        id: summaryPage
        Flickable {
            clip: true
            contentHeight: summaryCol.implicitHeight
            Column {
                id: summaryCol
                width: parent.width
                spacing: Style.space(4)
                Repeater {
                    model: root.state.categories
                    Row {
                        spacing: Style.space(12)
                        width: summaryCol.width
                        visible: modelData.id !== "income"
                        Text {
                            width: Style.space(140)
                            text: modelData.icon + "  " + modelData.name
                            color: root.foreground
                        }
                        Text {
                            text: Ledger.formatCents(Ledger.categoryMonthCents(root.state, root.year, root.month, modelData.id))
                            color: root.foreground
                        }
                    }
                }
            }
        }
    }

    Component {
        id: registerPage
        ListView {
            clip: true
            model: Ledger.monthEntries(root.state, root.year, root.month)
            spacing: Style.space(4)
            delegate: Row {
                width: ListView.view.width
                spacing: Style.space(8)
                Text {
                    width: Style.space(90)
                    text: modelData.date
                    color: root.foreground
                }
                Text {
                    width: Style.space(220)
                    text: modelData.title || "(untitled)"
                    elide: Text.ElideRight
                    color: root.foreground
                }
                Text {
                    width: Style.space(100)
                    text: Ledger.categoryName(root.state, modelData.categoryId)
                    color: root.foreground
                    opacity: 0.75
                }
                Text {
                    width: Style.space(90)
                    horizontalAlignment: Text.AlignRight
                    text: Ledger.formatCents(modelData.amountCents)
                    color: root.foreground
                }
                Button {
                    text: "Delete"
                    onClicked: root.applyState(Ledger.deleteEntry(root.state, modelData.id), "Deleted")
                }
            }
        }
    }

    Component {
        id: addPage
        Column {
            spacing: Style.space(8)
            TextField { placeholderText: "Title"; text: root.formTitle; onTextChanged: root.formTitle = text }
            TextField { placeholderText: "Amount (12.50)"; text: root.formAmount; onTextChanged: root.formAmount = text }
            TextField { placeholderText: "Date YYYY-MM-DD"; text: root.formDate; onTextChanged: root.formDate = text }
            Row {
                spacing: Style.space(8)
                Button { text: "Expense"; selected: root.formKind === "expense"; onClicked: root.formKind = "expense" }
                Button { text: "Income"; selected: root.formKind === "income"; onClicked: root.formKind = "income" }
            }
            Flow {
                width: parent.width
                spacing: Style.space(6)
                Repeater {
                    model: root.state.categories
                    Button {
                        text: modelData.name
                        selected: root.formCategory === modelData.id
                        onClicked: root.formCategory = modelData.id
                    }
                }
            }
            TextField { placeholderText: "Note"; text: root.formNote; onTextChanged: root.formNote = text }
            Button {
                text: "Save"
                onClicked: {
                    var cents = Ledger.parseDollars(root.formAmount)
                    if (cents === null || cents === 0) {
                        root.status = "Enter a valid amount"
                        return
                    }
                    root.applyState(Ledger.addEntry(root.state, {
                        kind: root.formKind,
                        date: root.formDate,
                        amountCents: cents,
                        title: root.formTitle,
                        note: root.formNote,
                        categoryId: root.formCategory
                    }), "Saved")
                    root.formTitle = ""
                    root.formAmount = ""
                    root.formNote = ""
                    root.page = "register"
                }
            }
        }
    }

    Component {
        id: importPage
        Column {
            spacing: Style.space(8)
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                color: root.foreground
                text: "Credits import as income. Payroll keywords are in Settings. Path is on this Omarchy machine."
            }
            TextField {
                width: parent.width
                placeholderText: "/home/you/Downloads/export.qfx"
                text: root.importPath
                onTextChanged: root.importPath = text
            }
            Button {
                text: "Import QFX"
                onClicked: {
                    var allowed = Ledger.isAllowedImportPath(root.importPath, root.homeDir)
                    if (!allowed.ok) {
                        root.importLog = allowed.reason
                        return
                    }
                    root.enqueueIo({
                        kind: "read-qfx",
                        command: root.readCommand(allowed.path, root.qfxMaxBytes)
                    })
                }
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                color: root.foreground
                text: root.importLog
            }
        }
    }

    Component {
        id: budgetPage
        Flickable {
            clip: true
            contentHeight: budgetCol.implicitHeight
            Column {
                id: budgetCol
                width: parent.width
                spacing: Style.space(6)
                Repeater {
                    model: root.state.categories
                    Row {
                        spacing: Style.space(8)
                        visible: modelData.id !== "income"
                        Text {
                            width: Style.space(140)
                            text: modelData.name
                            color: root.foreground
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        TextField {
                            id: budgetField
                            width: Style.space(120)
                            text: {
                                var budgets = root.state.budgets || []
                                for (var i = 0; i < budgets.length; i++) {
                                    if (budgets[i].year === root.year && budgets[i].month === root.month && budgets[i].categoryId === modelData.id)
                                        return (budgets[i].amountCents / 100).toFixed(2)
                                }
                                return ""
                            }
                            placeholderText: "0.00"
                        }
                        Button {
                            text: "Set"
                            onClicked: {
                                var cents = Ledger.parseDollars(budgetField.text)
                                if (cents === null)
                                    cents = 0
                                root.applyState(Ledger.upsertBudget(root.state, root.year, root.month, modelData.id, Math.abs(cents)), "Budget saved")
                            }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: settingsPage
        Column {
            spacing: Style.space(8)
            TextField {
                width: parent.width
                placeholderText: "First paycheck name"
                text: root.state.settings.firstPaycheckName
                onEditingFinished: {
                    var next = Ledger.clone(root.state)
                    next.settings.firstPaycheckName = text
                    root.applyState(next, "Saved")
                }
            }
            TextField {
                width: parent.width
                placeholderText: "Second paycheck name"
                text: root.state.settings.secondPaycheckName
                onEditingFinished: {
                    var next = Ledger.clone(root.state)
                    next.settings.secondPaycheckName = text
                    root.applyState(next, "Saved")
                }
            }
            Text {
                text: "Payroll keywords (comma-separated)"
                color: root.foreground
            }
            TextField {
                width: parent.width
                text: root.keywordText
                onTextChanged: root.keywordText = text
                onEditingFinished: {
                    var next = Ledger.clone(root.state)
                    next.settings.payrollKeywords = root.keywordText.split(",").map(function (s) { return s.trim() }).filter(function (s) { return s.length })
                    root.applyState(next, "Saved")
                }
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                color: root.foreground
                opacity: 0.7
                text: "Ledger file: " + root.dataPath
            }
        }
    }
}
