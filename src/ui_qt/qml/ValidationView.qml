pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

ColumnLayout {
    id: root
    required property var view
    required property var activity
    property bool embedded: false
    readonly property var validation: view.validation ?? null
    readonly property bool validating: view.busy === "validate"
    spacing: Palette.gap

    Flow {
        Layout.fillWidth: true
        spacing: Palette.gap
        ActionButton {
            text: "Run validation"
            primary: true
            enabled: !root.view.busy && root.view.loaded && root.view.selectedBase > 0
            onClicked: WizardBackend.runValidation()
        }
        ActionButton {
            text: "Install selected mods"
            visible: !root.embedded
            enabled: root.view.canInstallReview ?? false
            onClicked: WizardBackend.openInstallReview()
        }
        ActionButton {
            text: "Restore failed install"
            visible: !root.embedded && (root.view.install?.pending ?? false) && !root.view.busy
            danger: true
            onClicked: WizardBackend.restoreSnapshot()
        }
    }

    Label {
        Layout.fillWidth: true
        visible: !root.view.loaded
        text: "Load a guide first."
        color: Palette.muted
        font.pixelSize: 16
    }

    Rectangle {
        Layout.fillWidth: true
        visible: root.validating
        implicitHeight: progressColumn.implicitHeight + 24
        radius: Palette.radius
        color: Palette.panel
        border.color: Palette.border
        ColumnLayout {
            id: progressColumn
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            Label {
                text: "Validating\u2026 " + (root.activity.validationProgress?.message ?? "Starting validation")
                color: Palette.text
                font.pixelSize: 16
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            ProgressBar {
                Layout.fillWidth: true
                indeterminate: (root.activity.validationProgress?.total ?? 0) === 0
                from: 0
                to: Math.max(1, root.activity.validationProgress?.total ?? 1)
                value: root.activity.validationProgress?.step ?? 0
                Accessible.name: "Validation progress"
            }
        }
    }

    Rectangle {
        Layout.fillWidth: true
        visible: root.validation !== null && !root.validating
        implicitHeight: banner.implicitHeight + 24
        radius: Palette.radius
        color: Palette.panel
        border.width: 2
        border.color: root.validation?.success ? Palette.success : Palette.danger
        ColumnLayout {
            id: banner
            anchors.fill: parent
            anchors.margins: 12
            spacing: 4
            Label {
                text: root.validation?.failure ? "Validation stopped"
                    : root.validation?.success ? "Validation passed" : "Validation found problems"
                color: root.validation?.success ? Palette.success : Palette.danger
                font.pixelSize: 20
                font.bold: true
                Accessible.name: text
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Palette.text
                font.pixelSize: 15
                text: root.validation?.failure
                    ? root.validation.failure
                    : (root.validation?.passed ?? 0) + " checks passed, " + (root.validation?.warnings ?? 0)
                      + " warnings, " + (root.validation?.errors ?? 0) + " errors across "
                      + (root.validation?.components ?? 0) + " mods."
            }
        }
    }

    Frame {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: 120
        visible: root.validation !== null && !root.validating && (root.validation?.failure ?? null) === null
        padding: 0
        background: Rectangle {
            radius: Palette.radius
            color: Palette.panel
            border.color: Palette.border
        }
        ListView {
            id: stages
            anchors.fill: parent
            anchors.margins: 1
            clip: true
            model: (root.validation?.stages ?? []).concat((root.validation?.issues ?? []).map(i => ({
                name: i.severity, passed: i.severity !== "Error", warnings: i.severity === "Warning",
                summary: i.message, messages: i.component ? [i.component] : []
            })))
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: Item {
                id: stage
                required property var modelData
                width: ListView.view.width
                implicitHeight: stageColumn.implicitHeight + 16
                ColumnLayout {
                    id: stageColumn
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 2
                    RowLayout {
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 10
                            Layout.preferredHeight: 10
                            radius: 5
                            color: !stage.modelData.passed ? Palette.danger : stage.modelData.warnings ? Palette.warning : Palette.success
                        }
                        Label {
                            text: stage.modelData.name
                            color: Palette.text
                            font.pixelSize: 15
                            font.bold: true
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 18
                        text: stage.modelData.summary
                        color: Palette.muted
                        font.pixelSize: 14
                        wrapMode: Text.Wrap
                    }
                    Repeater {
                        model: stage.modelData.messages ?? []
                        Label {
                            required property string modelData
                            Layout.fillWidth: true
                            Layout.leftMargin: 18
                            text: modelData
                            color: Palette.muted
                            font.pixelSize: 13
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }
    }
    Item { Layout.fillHeight: root.validation === null; visible: root.validation === null }
}
