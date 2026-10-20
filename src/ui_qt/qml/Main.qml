pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

ApplicationWindow {
    id: window
    width: 1200
    height: 800
    minimumWidth: 940
    minimumHeight: 640
    visible: true
    title: "ModSync"
    color: Theme.window
    font.family: regularFont.name
    font.pixelSize: 15
    palette.window: Theme.window
    palette.windowText: Theme.text
    palette.text: Theme.text
    palette.base: Theme.panel
    palette.button: Theme.raised
    palette.buttonText: Theme.text
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.accentText
    palette.placeholderText: Theme.muted

    function parse(text) {
        try {
            return JSON.parse(text)
        } catch (error) {
            return {}
        }
    }
    readonly property var view: parse(WizardBackend.snapshot)
    readonly property var activity: parse(WizardBackend.activity)
    readonly property var host: parse(WizardBackend.desktop)
    readonly property string surface: view.surface ?? "wizard"

    FontLoader {
        id: regularFont
        source: "qrc:/qt/qml/org/modsync/desktop/src/ui_qt/assets/fonts/NotoSans-Regular.ttf"
    }
    FontLoader {
        source: "qrc:/qt/qml/org/modsync/desktop/src/ui_qt/assets/fonts/NotoSans-Bold.ttf"
    }

    Binding {
        target: Theme
        property: "mode"
        value: window.view.theme ?? "dark"
    }

    Component.onCompleted: {
        WizardBackend.bootstrap()
        if (WizardBackend.easy) easyLoader.active = true
    }

    Loader {
        id: easyLoader
        active: false
        sourceComponent: EasyBuilder {}
        Connections {
            target: easyLoader.item
            ignoreUnknownSignals: true
            function onClosing() {
                easyLoader.active = false
            }
        }
    }

    header: Rectangle {
        implicitHeight: 64
        color: Theme.panel
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.border
        }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 20
            anchors.rightMargin: 20
            spacing: Theme.gap
            Label {
                text: "ModSync"
                color: Theme.accent
                font.pixelSize: 24
                font.bold: true
                Accessible.role: Accessible.Heading
                Accessible.name: "ModSync"
            }
            Item { Layout.preferredWidth: 12 }
            Repeater {
                model: [
                    {label: "Getting started", key: "wizard"},
                    {label: "Summary / selection", key: "selection"},
                    {label: "Validation / install", key: "validation"}
                ]
                ActionButton {
                    id: navButton
                    required property var modelData
                    text: modelData.label
                    checkable: true
                    checked: window.surface === modelData.key
                    quiet: true
                    enabled: window.view.loaded ?? false
                    onClicked: WizardBackend.setSurface(modelData.key)
                }
            }
            Item { Layout.fillWidth: true }
            ActionButton {
                text: "Easy mode"
                quiet: true
                onClicked: easyLoader.active = true
            }
            ActionButton {
                text: "Settings"
                quiet: true
                onClicked: settingsDialog.open()
            }
            ComboBox {
                id: themeBox
                Layout.preferredWidth: 110
                implicitHeight: 40
                font.pixelSize: 14
                model: ["Dark", "Light", "K1", "K2"]
                currentIndex: Math.max(0, ["dark", "light", "k1", "k2"].indexOf(window.view.theme ?? "dark"))
                Accessible.name: "Theme"
                onActivated: WizardBackend.setTheme(currentText.toLowerCase())
                contentItem: Text {
                    leftPadding: 12
                    text: themeBox.displayText
                    color: Theme.text
                    font: themeBox.font
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
                background: Rectangle {
                    radius: Theme.radius
                    color: Theme.raised
                    border.width: themeBox.visualFocus ? 3 : 1
                    border.color: themeBox.visualFocus ? Theme.focus : Theme.border
                }
                delegate: ItemDelegate {
                    id: themeOption
                    required property var modelData
                    required property int index
                    width: themeBox.width
                    highlighted: themeBox.highlightedIndex === index
                    contentItem: Text {
                        text: themeOption.modelData
                        color: Theme.text
                        font.pixelSize: 14
                    }
                    background: Rectangle { color: themeOption.highlighted ? Theme.raised : Theme.panel }
                }
                popup.background: Rectangle {
                    color: Theme.panel
                    border.color: Theme.border
                    radius: Theme.radius
                }
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            Layout.fillWidth: true
            visible: (WizardBackend.message ?? "").length > 0 && !(sharingGate.visible && (WizardBackend.message ?? "").startsWith("Pause sharing before editing"))
            implicitHeight: messageLabel.implicitHeight + 20
            color: Theme.raised
            border.color: Theme.warning
            Label {
                id: messageLabel
                anchors.fill: parent
                anchors.margins: 10
                text: WizardBackend.message
                color: Theme.text
                font.pixelSize: 15
                wrapMode: Text.Wrap
                verticalAlignment: Text.AlignVCenter
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
        }
        SharingGate {
            id: sharingGate
            Layout.fillWidth: true
            host: window.host
            visible: (window.host.sharing?.gated ?? false) && window.surface !== "validation"
        }
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: window.surface === "selection" ? 1 : window.surface === "validation" ? 2 : 0
            WizardView {
                view: window.view
                activity: window.activity
            }
            ColumnLayout {
                SelectionView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.margins: 28
                    view: window.view
                }
            }
            ColumnLayout {
                ValidationView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.margins: 28
                    view: window.view
                    activity: window.activity
                }
            }
        }
    }

    ConsentModal {
        host: window.host
    }
    SettingsDialog {
        id: settingsDialog
        host: window.host
        view: window.view
    }
    InstallModal {
        wanted: window.view.confirmInstall ?? false
        game: window.view.game ?? ""
    }
    StopModal {
        wanted: window.view.stopPrompt ?? false
        onConfirmed: WizardBackend.stop()
        onDismissed: WizardBackend.dismissStop()
    }
}
