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
    readonly property bool analyzing: view.busy === "conflicts"
    readonly property var conflicts: view.conflicts ?? null
    spacing: Theme.gap

    Flow {
        Layout.fillWidth: true
        spacing: Theme.gap
        ActionButton {
            text: "Analyze conflicts"
            enabled: !root.view.busy && root.view.loaded && root.view.selectedBase > 0
            onClicked: WizardBackend.analyzeConflicts()
        }
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
        color: Theme.muted
        font.pixelSize: 16
    }

    Rectangle {
        Layout.fillWidth: true
        visible: root.validating
        implicitHeight: progressColumn.implicitHeight + 24
        radius: Theme.radius
        color: Theme.panel
        border.color: Theme.border
        ColumnLayout {
            id: progressColumn
            anchors.fill: parent
            anchors.margins: 12
            spacing: 8
            Label {
                text: "Validating\u2026 " + (root.activity.validationProgress?.message ?? "Starting validation")
                color: Theme.text
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
        radius: Theme.radius
        color: Theme.panel
        border.width: 2
        border.color: root.validation?.success ? Theme.success : Theme.danger
        ColumnLayout {
            id: banner
            anchors.fill: parent
            anchors.margins: 12
            spacing: 4
            Label {
                text: root.validation?.failure ? "Validation stopped"
                    : root.validation?.success ? "Validation passed" : "Validation found problems"
                color: root.validation?.success ? Theme.success : Theme.danger
                font.pixelSize: 20
                font.bold: true
                Accessible.name: text
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: Theme.text
                font.pixelSize: 15
                text: root.validation?.failure
                    ? root.validation.failure
                    : (root.validation?.passed ?? 0) + " checks passed, " + (root.validation?.warnings ?? 0)
                      + " warnings, " + (root.validation?.errors ?? 0) + " errors across "
                      + (root.validation?.components ?? 0) + " mods."
            }
        }
    }

    Rectangle {
        Layout.fillWidth: true
        visible: root.analyzing
        implicitHeight: 56
        radius: Theme.radius
        color: Theme.panel
        border.color: Theme.border
        Label {
            anchors.fill: parent
            anchors.margins: 12
            text: "Analyzing file conflicts\u2026"
            color: Theme.text
            font.pixelSize: 16
            verticalAlignment: Text.AlignVCenter
            Accessible.name: text
        }
    }

    Rectangle {
        Layout.fillWidth: true
        visible: root.conflicts !== null && !root.analyzing
        implicitHeight: Math.min(260, conflictColumn.implicitHeight + 24)
        radius: Theme.radius
        color: Theme.panel
        border.width: 2
        border.color: (root.conflicts?.failure ?? null) !== null ? Theme.danger
            : (root.conflicts?.rows?.length ?? 0) > 0 ? Theme.warning : Theme.success
        ColumnLayout {
            id: conflictColumn
            anchors.fill: parent
            anchors.margins: 12
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: root.conflicts?.failure ?? root.conflicts?.summary ?? ""
                color: Theme.text
                font.pixelSize: 16
                font.bold: true
                wrapMode: Text.Wrap
                Accessible.name: text
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: Math.min(150, contentHeight)
                clip: true
                model: root.conflicts?.rows ?? []
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                delegate: Label {
                    id: conflictRow
                    required property var modelData
                    width: ListView.view.width
                    wrapMode: Text.Wrap
                    color: Theme.muted
                    font.pixelSize: 14
                    text: modelData.destinationPath + " \u2190 " + modelData.writers.map(w => w.componentName + (w.isWinner ? " (wins)" : "")).join(", ")
                }
            }
            Label {
                Layout.fillWidth: true
                visible: (root.conflicts?.limitations?.length ?? 0) > 0
                text: (root.conflicts?.limitations ?? []).join(" ")
                color: Theme.muted
                font.pixelSize: 13
                wrapMode: Text.Wrap
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
            radius: Theme.radius
            color: Theme.panel
            border.color: Theme.border
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
                            color: !stage.modelData.passed ? Theme.danger : stage.modelData.warnings ? Theme.warning : Theme.success
                        }
                        Label {
                            text: stage.modelData.name
                            color: Theme.text
                            font.pixelSize: 15
                            font.bold: true
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: 18
                        text: stage.modelData.summary
                        color: Theme.muted
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
                            color: Theme.muted
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
