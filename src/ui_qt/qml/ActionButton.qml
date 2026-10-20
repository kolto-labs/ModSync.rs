import QtQuick
import QtQuick.Controls.Basic

Button {
    id: control
    property bool primary: false
    property bool danger: false
    property bool flat: false
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
        color: !control.enabled ? Palette.muted
            : control.primary ? Palette.accentText
            : control.danger ? Palette.danger : Palette.text
        opacity: control.enabled ? 1 : 0.55
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: Palette.radius
        color: {
            if (control.primary) {
                const base = control.enabled ? Palette.accent : Palette.raised
                return control.down ? Qt.darker(base, 1.2) : control.hovered ? Qt.lighter(base, 1.1) : base
            }
            if (control.checked) return Palette.raised
            if (control.flat) return control.hovered ? Palette.raised : "transparent"
            return control.down ? Qt.darker(Palette.raised, 1.2) : control.hovered ? Qt.lighter(Palette.raised, 1.15) : Palette.raised
        }
        border.width: control.visualFocus ? 3 : 1
        border.color: control.visualFocus ? Palette.focus : control.checked ? Palette.accent : Palette.border
    }
}
