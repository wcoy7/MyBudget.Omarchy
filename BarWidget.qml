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
            return "MyExpenses!"
        if (panelLoader.item)
            return panelLoader.item.barLabel
        return "MyExpenses"
    }

    function open() {
        if (panelLoader.item)
            panelLoader.item.open()
        else if (loadError)
            console.log("MyExpenses panel failed to load:", loadError)
    }

    function close() {
        if (panelLoader.item)
            panelLoader.item.close()
    }

    function toggle() {
        if (panelLoader.item)
            panelLoader.item.toggle()
        else if (loadError)
            console.log("MyExpenses panel failed to load:", loadError)
    }

    function closeForPopoutSwitch() {
        if (panelLoader.item)
            panelLoader.item.closeForPopoutSwitch()
    }

    function injectPanel() {
        if (!panelLoader.item)
            return
        panelLoader.item.bar = root.bar
        panelLoader.item.anchorItem = button
        panelLoader.item.hostWidget = root
    }

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    onBarChanged: injectPanel()

    Loader {
        id: panelLoader
        active: true
        source: Qt.resolvedUrl("Panel.qml")
        visible: false
        onLoaded: {
            root.loadError = ""
            root.injectPanel()
            Qt.callLater(root.injectPanel)
        }
        onStatusChanged: {
            if (status === Loader.Error)
                root.loadError = (sourceComponent && sourceComponent.errorString) ? sourceComponent.errorString() : "Panel.qml failed to load"
            else if (status === Loader.Ready)
                root.loadError = ""
        }
    }

    WidgetButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: root.spendLabel
        tooltipText: root.loadError ? ("MyExpenses error: " + root.loadError) : "Open MyExpenses"
        onPressed: function (buttonCode) {
            if (buttonCode === Qt.LeftButton)
                root.toggle()
        }
    }
}
