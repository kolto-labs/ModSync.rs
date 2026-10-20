import QtQuick
import QtQuick.Controls.Basic
import org.modsync.desktop

TextField {
    id: field
    implicitHeight: 40
    font.pixelSize: 15
    placeholderTextColor: Theme.muted
    color: Theme.text
    selectByMouse: true
    background: Rectangle {
        radius: Theme.radius
        color: Theme.raised
        border.width: field.activeFocus ? 2 : 1
        border.color: field.activeFocus ? Theme.focus : Theme.border
    }
}
