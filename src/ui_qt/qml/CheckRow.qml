import QtQuick
import QtQuick.Controls.Basic
import org.modsync.desktop

CheckBox {
    id: control
    implicitHeight: 36
    spacing: 10
    font.pixelSize: 15
    focusPolicy: Qt.StrongFocus
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checked: checked

    indicator: Rectangle {
        x: control.leftPadding
        y: (control.height - height) / 2
        implicitWidth: 22
        implicitHeight: 22
        radius: 5
        color: control.checked ? Theme.accent : Theme.window
        border.width: control.visualFocus ? 3 : 1
        border.color: control.visualFocus ? Theme.focus : control.checked ? Theme.accent : Theme.border
        Text {
            anchors.centerIn: parent
            visible: control.checked
            text: "\u2713"
            color: Theme.accentText
            font.pixelSize: 15
            font.bold: true
        }
    }
    contentItem: Text {
        leftPadding: control.indicator.width + control.spacing
        text: control.text
        font: control.font
        color: control.enabled ? Theme.text : Theme.muted
        verticalAlignment: Text.AlignVCenter
        wrapMode: Text.Wrap
    }
}
