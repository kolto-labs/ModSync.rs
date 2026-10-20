import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

Rectangle {
    id: root
    required property var host
    readonly property var sharing: host.sharing ?? ({})
    visible: sharing.gated ?? false
    implicitHeight: row.implicitHeight + 20
    color: Theme.raised
    border.color: Theme.warning
    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 10
        spacing: Theme.gap
        Label {
            Layout.fillWidth: true
            text: "Pause sharing before editing the guide, folders, or install settings."
            color: Theme.text
            font.pixelSize: 15
            wrapMode: Text.Wrap
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        BusyIndicator {
            running: root.sharing.busy ?? false
            visible: running
            Layout.preferredWidth: 28
            Layout.preferredHeight: 28
        }
        Label {
            visible: (root.sharing.busy ?? false) && (root.sharing.status ?? "").length > 0
            text: root.sharing.status ?? ""
            color: Theme.muted
            font.pixelSize: 14
        }
        ActionButton {
            text: "Pause sharing to edit"
            primary: true
            enabled: !(root.sharing.busy ?? false)
            onClicked: WizardBackend.pauseSharing()
        }
    }
}
