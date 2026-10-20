import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

Popup {
    id: popup
    required property var categories
    property var chosen: ({})
    signal accepted(var picked)
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(520, (parent ? parent.width : 520) - 48)
    height: Math.min(560, (parent ? parent.height : 560) - 48)
    padding: Theme.pad
    closePolicy: Popup.CloseOnEscape
    onAboutToShow: chosen = ({})
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
            text: "Select multiple categories"
            color: Theme.text
            font.pixelSize: 22
            font.bold: true
            Layout.fillWidth: true
            Accessible.role: Accessible.Heading
        }
        Label {
            text: "Choose the categories to add to the base build."
            color: Theme.muted
            font.pixelSize: 15
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: popup.categories
            boundsBehavior: Flickable.StopAtBounds
            activeFocusOnTab: true
            Accessible.role: Accessible.List
            Accessible.name: "Categories"
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: CheckRow {
                required property string modelData
                width: ListView.view.width
                text: modelData
                checked: popup.chosen[modelData] === true
                onToggled: {
                    const next = Object.assign({}, popup.chosen)
                    next[modelData] = checked
                    popup.chosen = next
                }
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: Theme.gap
            ActionButton {
                text: "Cancel"
                onClicked: popup.close()
            }
            ActionButton {
                text: "Select categories"
                primary: true
                enabled: Object.values(popup.chosen).some(value => value)
                onClicked: {
                    popup.accepted(Object.keys(popup.chosen).filter(key => popup.chosen[key]))
                    popup.close()
                }
            }
        }
    }
}
