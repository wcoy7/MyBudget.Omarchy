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
    readonly property string dataDir: (Quickshell.env("HOME") || "") + "/.local/share/expenses"
    readonly property string dataPath: dataDir + "/ledger.json"
    readonly property string backupPath: dataPath + ".bak"
    readonly property alias content: body

    // When true, we are showing an empty ledger because the on-disk file was
    // missing/corrupt. Never overwrite disk with that empty seed.
    property bool blockEmptyPersist: false
    property bool loadingBackup: false

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

    function backupLedgerFile() {
        Quickshell.execDetached([
            "bash", "-lc",
            'src="$1"; bak="$2"; mkdir -p "$(dirname "$src")"; if [ -f "$src" ] && [ -s "$src" ]; then cp -f "$src" "$bak"; fi',
            "_", dataPath, backupPath
        ])
    }

    function persist() {
        if (blockEmptyPersist && Ledger.isEffectivelyEmpty(state)) {
            status = "Not saving empty ledger over a missing/damaged file. Add an entry first."
            return false
        }
        Quickshell.execDetached(["mkdir", "-p", dataDir])
        backupLedgerFile()
        ledgerFile.setText(Ledger.dump(state))
        blockEmptyPersist = false
        return true
    }

    function reloadFromDisk() {
        loadingBackup = false
        ledgerFile.reload()
    }

    function tryLoadBackup(reason) {
        loadingBackup = true
        status = reason + " Trying backup…"
        backupFile.reload()
    }

    function applyState(next, message) {
        state = next
        if (!persist())
            return
        if (message)
            status = message
    }

    FileView {
        id: ledgerFile
        path: root.dataPath
        watchChanges: true
        atomicWrites: true
        printErrors: false
        onLoaded: {
            var result = Ledger.loadStrict(text())
            if (!result.ok) {
                root.tryLoadBackup(result.error + ".")
                return
            }
            root.acceptLoadedState(result.state, "")
        }
        onLoadFailed: {
            root.tryLoadBackup("No ledger file.")
        }
        onFileChanged: {
            if (!root.loadingBackup)
                reload()
        }
    }

    FileView {
        id: backupFile
        path: root.backupPath
        watchChanges: false
        atomicWrites: true
        printErrors: false
        onLoaded: {
            root.loadingBackup = false
            var result = Ledger.loadStrict(text())
            if (result.ok && !Ledger.isEffectivelyEmpty(result.state)) {
                root.acceptLoadedState(result.state, "Restored from ledger.json.bak")
                Quickshell.execDetached(["mkdir", "-p", root.dataDir])
                ledgerFile.setText(Ledger.dump(root.state))
                return
            }
            root.seedEmptyInMemory("Ledger missing/damaged and backup unavailable. Showing empty until you save an entry.")
        }
        onLoadFailed: {
            root.loadingBackup = false
            root.seedEmptyInMemory("Ledger missing/damaged and no backup found. Showing empty until you save an entry.")
        }
    }

    FileView {
        id: qfxFile
        path: ""
        printErrors: false
        onLoaded: {
            var txns = Qfx.parse(text())
            var result = Ledger.importTxns(root.state, txns)
            root.applyState(result.state, "Imported " + result.added)
            root.importLog = result.lines.join("\n")
        }
        onLoadFailed: function (error) {
            root.importLog = "Could not read file: " + error
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
                    qfxFile.path = root.importPath
                    qfxFile.reload()
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
