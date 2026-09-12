import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Ledger.js" as Ledger
import "Qfx.js" as Qfx

Panel {
    id: root
    moduleName: "mybudget.expenses"
    manageIpc: false

    property var anchorItem: null
    property var hostWidget: null
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

    readonly property string barLabel: Ledger.formatCents(Ledger.monthExpenseCents(state, year, month))
    readonly property string dataDir: (Quickshell.env("HOME") || "") + "/.local/share/expenses"
    readonly property string dataPath: dataDir + "/ledger.json"

    function open() {
        root.controller.show()
        reloadFromDisk()
    }

    function close() {
        root.controller.hide()
    }

    function toggle() {
        if (root.opened)
            close()
        else
            open()
    }

    function switchPanel(direction) {
        if (root.bar && typeof root.bar.switchPanelFrom === "function")
            return root.bar.switchPanelFrom(root.hostWidget || root, direction)
        return false
    }

    function persist() {
        Quickshell.execDetached(["mkdir", "-p", dataDir])
        ledgerFile.setText(Ledger.dump(state))
    }

    function reloadFromDisk() {
        ledgerFile.reload()
    }

    function applyState(next, message) {
        state = next
        persist()
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
            root.state = Ledger.load(text())
            root.keywordText = (root.state.settings.payrollKeywords || []).join(", ")
        }
        onLoadFailed: {
            root.state = Ledger.empty()
            root.persist()
        }
        onFileChanged: reload()
    }

    KeyboardPanel {
        id: panel
        anchorItem: root.anchorItem
        owner: root.hostWidget || root
        bar: root.bar
        open: root.opened
        focusTarget: keyCatcher
        contentWidth: panel.fittedContentWidth(Style.space(760))
        contentHeight: panel.fittedContentHeight(Style.space(560))

        PanelKeyCatcher {
            id: keyCatcher
            anchors.fill: parent
            onCloseRequested: root.close()
            onTabRequested: function (direction) { root.switchPanel(direction) }

            Column {
                id: content
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
                        color: root.barForeground
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
                        color: root.barForeground
                        opacity: 0.7
                        font.pixelSize: Style.font.small
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
                            highlighted: root.page === modelData.id
                            onClicked: root.page = modelData.id
                        }
                    }
                }

                Row {
                    spacing: Style.space(16)
                    Text {
                        text: "Spent  " + Ledger.formatCents(Ledger.monthExpenseCents(root.state, root.year, root.month))
                        color: root.barForeground
                    }
                    Text {
                        text: "Income  " + Ledger.formatCents(Ledger.monthIncomeCents(root.state, root.year, root.month))
                        color: root.barForeground
                    }
                    Text {
                        text: "Budget  " + Ledger.formatCents(Ledger.monthBudgetCents(root.state, root.year, root.month))
                        color: root.barForeground
                    }
                }

                Loader {
                    width: parent.width
                    height: Style.space(420)
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
                            color: root.barForeground
                        }
                        Text {
                            text: Ledger.formatCents(Ledger.categoryMonthCents(root.state, root.year, root.month, modelData.id))
                            color: root.barForeground
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
                    color: root.barForeground
                }
                Text {
                    width: Style.space(220)
                    text: modelData.title || "(untitled)"
                    elide: Text.ElideRight
                    color: root.barForeground
                }
                Text {
                    width: Style.space(100)
                    text: Ledger.categoryName(root.state, modelData.categoryId)
                    color: root.barForeground
                    opacity: 0.75
                }
                Text {
                    width: Style.space(90)
                    horizontalAlignment: Text.AlignRight
                    text: Ledger.formatCents(modelData.amountCents)
                    color: root.barForeground
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
                Button { text: "Expense"; highlighted: root.formKind === "expense"; onClicked: root.formKind = "expense" }
                Button { text: "Income"; highlighted: root.formKind === "income"; onClicked: root.formKind = "income" }
            }
            ComboBox {
                model: root.state.categories.map(function (c) { return c.name })
                onActivated: function (index) { root.formCategory = root.state.categories[index].id }
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
                color: root.barForeground
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
                color: root.barForeground
                text: root.importLog
            }
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
                            color: root.barForeground
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
                color: root.barForeground
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
                color: root.barForeground
                opacity: 0.7
                text: "Ledger file: " + root.dataPath
            }
        }
    }
}
