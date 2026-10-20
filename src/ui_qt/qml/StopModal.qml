import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

Popup {
    id: popup
    property bool wanted: false
    property string heading: "Stop the install?"
    property string body: "The install stops after the current step. Restore the snapshot afterwards to return the folders to their earlier state."
    signal confirmed()
    signal dismissed()
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(520, (parent ? parent.width : 520) - 48)
    padding: Palette.pad
    closePolicy: Popup.CloseOnEscape
    onWantedChanged: wanted ? open() : close()
    onClosed: if (wanted) dismissed()
    Accessible.role: Accessible.Dialog
    Accessible.name: heading
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
            text: popup.heading
            color: Palette.text
            font.pixelSize: 22
            font.bold: true
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        Label {
            text: popup.body
            color: Palette.muted
            font.pixelSize: 15
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: Palette.gap
            ActionButton {
                text: "Keep installing"
                focus: true
                onClicked: popup.dismissed()
            }
            ActionButton {
                text: "Stop install"
                danger: true
                onClicked: popup.confirmed()
            }
        }
    }
}
