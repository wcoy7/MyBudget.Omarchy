import QtQuick
import Quickshell
import qs.Commons
import qs.Ui

Panel {
    id: root
    moduleName: "mybudget.expenses"
    manageIpc: false

    Component.onCompleted: console.warn("MyExpenses 0.1.4 panel ready")

    property var anchorItem: null
    property var hostWidget: null

    readonly property string barLabel: dataApp.barLabel
    readonly property bool anchorsReady: anchorItem !== null && bar !== null

    function open() {
        // Show first so a slow disk read cannot leave the user with nothing.
        root.controller.show()
        console.info("MyExpenses: open anchorsReady=" + anchorsReady
                     + " bar=" + !!root.bar
                     + " anchor=" + !!root.anchorItem)
        Qt.callLater(function () {
            dataApp.reloadFromDisk()
            if (popupLoader.item && popupLoader.item.panelApp)
                popupLoader.item.panelApp.reloadFromDisk()
        })
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

    // Always-on instance so the bar label updates without opening the panel.
    ExpensesApp {
        id: dataApp
        visible: false
        width: 1
        height: 1
        foreground: root.barForeground
        versionLabel: "MyExpenses v0.1.4"
    }

    // Create the popup only after the bar injects anchor + bar. Building
    // KeyboardPanel with null required anchors can leave a window that never
    // maps, while shell summon still returns ok.
    Loader {
        id: popupLoader
        active: root.anchorsReady
        asynchronous: false
        sourceComponent: keyboardPanelComponent
    }

    Component {
        id: keyboardPanelComponent
        KeyboardPanel {
            id: panel
            // Exposed for root.open() reload.
            readonly property alias panelApp: panelApp

            anchorItem: root.anchorItem
            owner: root.hostWidget || root
            bar: root.bar
            open: root.opened
            centerOnBar: false
            focusTarget: keyCatcher
            contentWidth: panel.fittedContentWidth(Style.space(760))
            contentHeight: panel.fittedContentHeight(Math.max(Style.space(520), Style.space(420)))

            PanelKeyCatcher {
                id: keyCatcher
                anchors.fill: parent
                onCloseRequested: root.close()
                onTabRequested: function (direction) { root.switchPanel(direction) }

                ExpensesApp {
                    id: panelApp
                    anchors.fill: parent
                    anchors.margins: Style.space(8)
                    foreground: root.barForeground
                    versionLabel: "MyExpenses v0.1.4"
                    Component.onCompleted: reloadFromDisk()
                }
            }
        }
    }
}
