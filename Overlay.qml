import Quickshell
import Quickshell.Wayland
import QtQuick
import qs.Commons
import qs.Ui

// Fullscreen overlay entry point. Walker / app menu and
// `omarchy-shell shell toggle mybudget.expenses` open this surface.
// The bar chip still uses BarWidget → Panel (anchored popup).
Item {
    id: root

    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var shell: null
    property var manifest: null
    property bool opened: false

    property color background: Color.menu.background
    property color foreground: Color.menu.text
    property color border: Color.menu.border
    property var borderSpec: Border.surfaceSpec("menu", "border", border, Math.max(1, Style.space(2)))
    property color scrim: Color.menu.scrim
    readonly property int cornerRadius: Style.cornerRadius
    property int contentMargin: Style.spacing.panelPadding

    function open(payloadJson) {
        root.opened = true
        if (app)
            app.reloadFromDisk()
        Qt.callLater(function () {
            keyCatcher.forceActiveFocus()
        })
    }

    function close() {
        root.opened = false
    }

    function dismiss() {
        root.opened = false
        if (root.shell && typeof root.shell.hide === "function")
            root.shell.hide((root.manifest && root.manifest.id) || "mybudget.expenses")
    }

    function toggle() {
        if (root.opened)
            root.dismiss()
        else
            root.open("{}")
    }

    PanelWindow {
        id: panel
        visible: root.opened
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        color: "transparent"
        WlrLayershell.namespace: "mybudget-expenses"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        exclusionMode: ExclusionMode.Ignore

        Rectangle {
            anchors.fill: parent
            color: root.scrim
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismiss()
        }

        BorderSurface {
            id: card
            width: Math.min(Style.space(960), panel.width - Style.gapsOut * 2)
            height: Math.min(Style.space(720), panel.height - Style.gapsOut * 2)
            radius: root.cornerRadius
            anchors.centerIn: parent
            color: root.background
            borderSpec: root.borderSpec
            padding: root.contentMargin

            // Eat clicks so they do not dismiss via the scrim MouseArea.
            MouseArea {
                anchors.fill: parent
                onClicked: {}
            }

            Item {
                id: keyCatcher
                anchors.fill: parent
                anchors.topMargin: card.contentTopInset
                anchors.rightMargin: card.contentRightInset
                anchors.bottomMargin: card.contentBottomInset
                anchors.leftMargin: card.contentLeftInset
                focus: true

                Keys.priority: Keys.BeforeItem
                Keys.onPressed: function (event) {
                    if (event.key === Qt.Key_Escape) {
                        root.dismiss()
                        event.accepted = true
                    }
                }

                Item {
                    anchors.fill: parent

                    Text {
                        id: titleLabel
                        anchors.left: parent.left
                        anchors.top: parent.top
                        text: "MyExpenses"
                        color: root.foreground
                        font.pixelSize: Style.font.heading
                        font.bold: true
                    }

                    Button {
                        anchors.right: parent.right
                        anchors.verticalCenter: titleLabel.verticalCenter
                        text: "Close"
                        onClicked: root.dismiss()
                    }

                    ExpensesApp {
                        id: app
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: titleLabel.bottom
                        anchors.bottom: parent.bottom
                        anchors.topMargin: Style.space(12)
                        foreground: root.foreground
                        versionLabel: "MyExpenses v0.1.4"
                    }
                }
            }
        }
    }
}
