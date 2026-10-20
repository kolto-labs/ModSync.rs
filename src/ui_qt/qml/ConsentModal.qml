import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

Popup {
    id: popup
    required property var host
    readonly property var consent: host.consent ?? null
    readonly property bool wanted: consent !== null
    property bool usage: true
    property bool performance: true
    property bool crash: true
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(600, (parent ? parent.width : 600) - 48)
    padding: Theme.pad
    closePolicy: Popup.NoAutoClose
    onWantedChanged: {
        if (wanted) {
            usage = consent.usage
            performance = consent.performance
            crash = consent.crash
            open()
        } else {
            close()
        }
    }
    background: Rectangle {
        radius: Theme.radius + 4
        color: Theme.panel
        border.color: Theme.border
    }
    Overlay.modal: Rectangle { color: "#99000000" }
    function edit() {
        return JSON.stringify({usage: usage, performance: performance, crash: crash})
    }
    contentItem: ColumnLayout {
        spacing: Theme.gap
        Label {
            text: "Telemetry preference"
            color: Theme.text
            font.pixelSize: 24
            font.bold: true
            Layout.fillWidth: true
            Accessible.role: Accessible.Heading
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.text
            font.pixelSize: 15
            text: "Choose whether ModSync may send anonymous usage, performance and hashed error reports. You can change this later in Desktop settings and privacy."
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.muted
            font.pixelSize: 14
            text: "Remote export also requires a configured endpoint and signing key. No remote server is selected by default."
        }
        CheckRow {
            text: "Usage events"
            checked: popup.usage
            enabled: !(popup.consent?.saving ?? false)
            onToggled: popup.usage = checked
        }
        CheckRow {
            text: "Performance metrics"
            checked: popup.performance
            enabled: !(popup.consent?.saving ?? false)
            onToggled: popup.performance = checked
        }
        CheckRow {
            text: "Hashed error reports"
            checked: popup.crash
            enabled: !(popup.consent?.saving ?? false)
            onToggled: popup.crash = checked
        }
        ActionButton {
            id: detailsToggle
            text: checked ? "Hide privacy details" : "Privacy details"
            checkable: true
            quiet: true
        }
        ScrollView {
            visible: detailsToggle.checked
            Layout.fillWidth: true
            Layout.preferredHeight: 150
            clip: true
            TextArea {
                readOnly: true
                wrapMode: Text.Wrap
                color: Theme.muted
                font.pixelSize: 13
                text: popup.consent?.summary ?? ""
                background: Rectangle { color: Theme.window; border.color: Theme.border; radius: Theme.radius }
                Accessible.name: "Privacy details"
            }
        }
        Label {
            Layout.fillWidth: true
            visible: (popup.host.status ?? "").length > 0
            text: popup.host.status ?? ""
            color: Theme.muted
            font.pixelSize: 14
            wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: Theme.gap
            ActionButton {
                text: "Keep telemetry off"
                focus: true
                enabled: !(popup.consent?.saving ?? false)
                onClicked: WizardBackend.answerConsent(false, popup.edit())
            }
            ActionButton {
                text: "Allow configured telemetry"
                primary: true
                enabled: !(popup.consent?.saving ?? false)
                onClicked: WizardBackend.answerConsent(true, popup.edit())
            }
        }
    }
}
