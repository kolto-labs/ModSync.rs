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
    padding: Theme.pad
    closePolicy: Popup.CloseOnEscape
    onWantedChanged: wanted ? open() : close()
    onClosed: if (wanted) dismissed()
    background: Rectangle {
        radius: Theme.radius + 4
        color: Theme.panel
        border.color: Theme.border
        border.width: 1
    }
    Overlay.modal: Rectangle { color: "#99000000" }
    contentItem: ColumnLayout {
        spacing: Theme.gap
        Label {
            text: popup.heading
            color: Theme.text
            font.pixelSize: 22
            font.bold: true
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        Label {
            text: popup.body
            color: Theme.muted
            font.pixelSize: 15
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: Theme.gap
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
