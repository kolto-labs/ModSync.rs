import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

Popup {
    id: popup
    property bool wanted: false
    property string game: ""
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(560, (parent ? parent.width : 560) - 48)
    padding: Palette.pad
    closePolicy: Popup.CloseOnEscape
    onWantedChanged: wanted ? open() : close()
    onClosed: if (wanted) WizardBackend.cancelConfirmation()
    Accessible.role: Accessible.Dialog
    Accessible.name: "Install selected mods?"
    background: Rectangle {
        radius: Palette.radius + 4
        color: Palette.panel
        border.color: Palette.border
        border.width: 1
    }
    Overlay.modal: Rectangle { color: "#99000000" }
    contentItem: ColumnLayout {
        spacing: Palette.gap
        Label {
            text: "Install selected mods?"
            color: Palette.text
            font.pixelSize: 24
            font.bold: true
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        Label {
            text: "Modify " + popup.game
            color: Palette.text
            font.pixelSize: 16
            Layout.fillWidth: true
            wrapMode: Text.WrapAnywhere
        }
        Label {
            text: "The snapshot needs space for both folders. The install stops on the first error."
            color: Palette.muted
            font.pixelSize: 15
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: Palette.gap
            ActionButton {
                text: "Cancel"
                focus: true
                onClicked: WizardBackend.cancelConfirmation()
            }
            ActionButton {
                text: "Create snapshot and install"
                primary: true
                onClicked: WizardBackend.confirmInstall()
            }
        }
    }
}
