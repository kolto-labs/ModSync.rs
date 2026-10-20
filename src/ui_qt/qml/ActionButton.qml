import QtQuick
import QtQuick.Controls.Basic

Button {
    id: control
    property bool primary: false
    property bool danger: false
    property bool quiet: false
    implicitHeight: 40
    implicitWidth: Math.max(104, label.implicitWidth + 36)
    font.pixelSize: 15
    font.bold: primary
    focusPolicy: Qt.StrongFocus
    hoverEnabled: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: clicked()

    contentItem: Text {
        id: label
        text: control.text
        font: control.font
        color: !control.enabled ? Theme.muted
            : control.primary ? Theme.accentText
            : control.danger ? Theme.danger : Theme.text
        opacity: control.enabled ? 1 : 0.55
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: Theme.radius
        color: {
            if (control.primary) {
                const base = control.enabled ? Theme.accent : Theme.raised
                return control.down ? Qt.darker(base, 1.2) : control.hovered ? Qt.lighter(base, 1.1) : base
            }
            if (control.checked) return Theme.raised
            if (control.quiet) return control.hovered ? Theme.raised : "transparent"
            return control.down ? Qt.darker(Theme.raised, 1.2) : control.hovered ? Qt.lighter(Theme.raised, 1.15) : Theme.raised
        }
        border.width: control.visualFocus ? 3 : 1
        border.color: control.visualFocus ? Theme.focus : control.checked ? Theme.accent : Theme.border
    }
}
