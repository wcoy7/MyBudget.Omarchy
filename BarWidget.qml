import QtQuick
import Quickshell
import qs.Ui

BarWidget {
    id: root
    moduleName: "mybudget.expenses"

    property string loadError: ""

    readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
    readonly property bool popoutSwitchClosing: panelLoader.item ? panelLoader.item.popoutSwitchClosing === true : false
    readonly property string spendLabel: {
        if (loadError)
            return "Expenses!"
        if (panelLoader.item)
            return panelLoader.item.barLabel
        return "Expenses"
    }

    function injectPanel() {
        if (!panelLoader.item)
            return
        panelLoader.item.bar = root.bar
        panelLoader.item.anchorItem = button
        panelLoader.item.hostWidget = root
    }

    function open() {
        injectPanel()
        if (!panelLoader.item) {
            console.warn("MyExpenses: Panel.qml not loaded (status=" + panelLoader.status + " error=" + loadError + ")")
            // Retry once in case the first load raced the bar.
            if (panelLoader.status === Loader.Error || panelLoader.status === Loader.Null) {
                panelLoader.active = false
                panelLoader.active = true
            }
            return
        }
        panelLoader.item.open()
    }

    function close() {
        if (panelLoader.item)
            panelLoader.item.close()
    }

    function toggle() {
        if (!panelLoader.item) {
            open()
            return
        }
        panelLoader.item.toggle()
    }

    function closeForPopoutSwitch() {
        if (panelLoader.item)
            panelLoader.item.closeForPopoutSwitch()
    }

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    onBarChanged: injectPanel()

    Loader {
        id: panelLoader
        active: true
        asynchronous: false
        source: Qt.resolvedUrl("Panel.qml")
        visible: false
        onLoaded: {
            root.loadError = ""
            root.injectPanel()
            Qt.callLater(root.injectPanel)
        }
        onStatusChanged: {
            if (status === Loader.Error) {
                root.loadError = "Panel.qml failed to load"
                console.warn("MyExpenses: Panel.qml Loader.Error")
            } else if (status === Loader.Ready) {
                root.loadError = ""
            }
        }
    }

    WidgetButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: root.spendLabel
        tooltipText: root.loadError !== ""
            ? ("MyExpenses error: " + root.loadError)
            : "Open MyExpenses 0.1.2"
        onPressed: function (buttonCode) {
            if (buttonCode === Qt.LeftButton)
                root.toggle()
        }
    }
}
