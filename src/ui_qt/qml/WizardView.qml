pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import org.modsync.desktop

RowLayout {
    id: root
    required property var view
    required property var activity
    spacing: 0

    readonly property string page: view.page ?? ""
    readonly property var installProgress: activity.install ?? null
    readonly property bool installFailed: view.install?.failed ?? false
    readonly property bool installDone: page === "WidescreenInstalling" ? (view.install?.finished ?? false) : (view.install?.baseComplete ?? false)
    readonly property bool lastPage: view.loaded && view.pageIndex === (view.pages?.length ?? 0) - 1

    function fileUrlToPath(url) {
        const text = url.toString()
        return decodeURIComponent(text.replace(/^file:\/\/(?=\/[A-Za-z]:)/, "").replace(/^file:\/\//, ""))
    }

    FileDialog {
        id: guideDialog
        title: "Choose a guide"
        nameFilters: ["Guides (*.md *.markdown *.toml *.yaml *.yml *.json)", "All files (*)"]
        onAccepted: WizardBackend.loadGuide(root.fileUrlToPath(selectedFile))
    }
    FolderDialog {
        id: modsDialog
        title: "Choose the mod workspace folder"
        onAccepted: WizardBackend.setMods(root.fileUrlToPath(selectedFolder))
    }
    FolderDialog {
        id: gameDialog
        title: "Choose the game folder"
        onAccepted: WizardBackend.setGame(root.fileUrlToPath(selectedFolder))
    }

    Rectangle {
        Layout.fillHeight: true
        Layout.preferredWidth: 248
        color: Theme.panel
        visible: root.view.loaded
        Rectangle {
            anchors.right: parent.right
            width: 1
            height: parent.height
            color: Theme.border
        }
        ListView {
            id: steps
            anchors.fill: parent
            anchors.margins: 16
            spacing: 4
            clip: true
            model: root.view.pages ?? []
            interactive: contentHeight > height
            Accessible.role: Accessible.List
            Accessible.name: "Wizard steps"
            delegate: Rectangle {
                id: step
                required property var modelData
                required property int index
                readonly property bool current: index === root.view.pageIndex
                readonly property bool done: index < root.view.pageIndex
                width: ListView.view.width
                height: 40
                radius: Theme.radius
                color: current ? Theme.raised : "transparent"
                border.width: current ? 1 : 0
                border.color: Theme.accent
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.title + (current ? ", current step" : done ? ", done" : "")
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 10
                    Rectangle {
                        Layout.preferredWidth: 22
                        Layout.preferredHeight: 22
                        radius: 11
                        color: step.done ? Theme.success : step.current ? Theme.accent : "transparent"
                        border.width: 2
                        border.color: step.done ? Theme.success : step.current ? Theme.accent : Theme.muted
                        Text {
                            anchors.centerIn: parent
                            text: step.done ? "\u2713" : step.index + 1
                            color: step.done || step.current ? Theme.accentText : Theme.muted
                            font.pixelSize: 12
                            font.bold: true
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        text: step.modelData.title
                        color: step.current ? Theme.text : Theme.muted
                        font.pixelSize: 14
                        font.bold: step.current
                        elide: Text.ElideRight
                    }
                }
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 0

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: 28
            spacing: Theme.gap

            Label {
                text: root.view.loaded ? root.view.pageTitle : "Welcome"
                color: Theme.text
                font.pixelSize: 30
                font.bold: true
                Layout.fillWidth: true
                elide: Text.ElideRight
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
            Label {
                text: root.view.loaded ? root.view.pageSubtitle : "Welcome to the ModSync Installation Wizard"
                color: Theme.muted
                font.pixelSize: 16
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: Theme.border
            }

            StackLayout {
                id: pages
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: {
                    switch (root.page) {
                    case "": case "Welcome": return 0
                    case "Preamble": case "AspyrNotice": case "WidescreenNotice": return 1
                    case "ModDirectory": return 2
                    case "GameDirectory": return 3
                    case "ModSelection": return 4
                    case "DownloadsExplain": return 5
                    case "Validate": return 6
                    case "InstallStart": return 7
                    case "Installing": case "WidescreenInstalling": return 8
                    case "WidescreenModSelection": return 9
                    default: return 10
                    }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.text
                        font.pixelSize: 16
                        text: "ModSync installs a curated set of mods into your game folder, checks every step first, and keeps a snapshot so a failed install can be undone."
                    }
                    Label {
                        Layout.fillWidth: true
                        visible: root.view.loaded
                        color: Theme.success
                        font.pixelSize: 16
                        font.bold: true
                        text: (root.view.guideName ? root.view.guideName + " \u2014 " : "") + (root.view.components?.length ?? 0) + " mods loaded. Choose Next to continue."
                        wrapMode: Text.Wrap
                    }
                    RowLayout {
                        spacing: Theme.gap
                        TextField {
                            id: guidePath
                            Layout.fillWidth: true
                            implicitHeight: 40
                            font.pixelSize: 15
                            placeholderText: "Path to a guide file"
                            placeholderTextColor: Theme.muted
                            color: Theme.text
                            text: root.view.input ?? ""
                            Accessible.name: "Guide file path"
                            onAccepted: WizardBackend.loadGuide(text)
                            background: Rectangle {
                                radius: Theme.radius
                                color: Theme.raised
                                border.width: guidePath.activeFocus ? 2 : 1
                                border.color: guidePath.activeFocus ? Theme.focus : Theme.border
                            }
                        }
                        ActionButton {
                            text: "Browse\u2026"
                            onClicked: guideDialog.open()
                        }
                        ActionButton {
                            text: "Load guide"
                            primary: true
                            enabled: !root.view.busy && guidePath.text.trim().length > 0
                            onClicked: WizardBackend.loadGuide(guidePath.text)
                        }
                    }
                    Item { Layout.fillHeight: true }
                }

                ScrollView {
                    id: notice
                    clip: true
                    contentWidth: availableWidth
                    Label {
                        width: notice.availableWidth
                        wrapMode: Text.Wrap
                        textFormat: Text.MarkdownText
                        color: Theme.text
                        linkColor: Theme.accent
                        font.pixelSize: 15
                        text: root.page === "AspyrNotice" ? root.view.aspyr
                            : root.page === "WidescreenNotice" ? root.view.widescreenWarning : root.view.preamble
                        onLinkActivated: link => Qt.openUrlExternally(link)
                    }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 15
                        text: "Pick a folder for downloaded archives and temporary work. It must already exist and have room for the mod archives."
                    }
                    RowLayout {
                        spacing: Theme.gap
                        TextField {
                            id: modsPath
                            Layout.fillWidth: true
                            implicitHeight: 40
                            font.pixelSize: 15
                            placeholderText: "Mod workspace folder"
                            placeholderTextColor: Theme.muted
                            color: Theme.text
                            text: root.view.mods ?? ""
                            Accessible.name: "Mod workspace folder"
                            onEditingFinished: WizardBackend.setMods(text)
                            background: Rectangle {
                                radius: Theme.radius
                                color: Theme.raised
                                border.width: modsPath.activeFocus ? 2 : 1
                                border.color: modsPath.activeFocus ? Theme.focus : Theme.border
                            }
                        }
                        ActionButton {
                            text: "Browse\u2026"
                            onClicked: modsDialog.open()
                        }
                    }
                    Label {
                        text: root.view.modsValid ? "Folder found." : "Choose an existing folder."
                        color: root.view.modsValid ? Theme.success : Theme.warning
                        font.pixelSize: 15
                        font.bold: true
                    }
                    Item { Layout.fillHeight: true }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 15
                        text: "Point to your KOTOR installation folder. ModSync checks it before installing and snapshots it so the install can be undone."
                    }
                    RowLayout {
                        spacing: Theme.gap
                        TextField {
                            id: gamePath
                            Layout.fillWidth: true
                            implicitHeight: 40
                            font.pixelSize: 15
                            placeholderText: "Game folder"
                            placeholderTextColor: Theme.muted
                            color: Theme.text
                            text: root.view.game ?? ""
                            Accessible.name: "Game folder"
                            onEditingFinished: WizardBackend.setGame(text)
                            background: Rectangle {
                                radius: Theme.radius
                                color: Theme.raised
                                border.width: gamePath.activeFocus ? 2 : 1
                                border.color: gamePath.activeFocus ? Theme.focus : Theme.border
                            }
                        }
                        ActionButton {
                            text: "Browse\u2026"
                            onClicked: gameDialog.open()
                        }
                    }
                    Label {
                        text: root.view.gameDetected ? "Detected: " + root.view.gameDetected : "No supported game found in that folder."
                        color: root.view.gameDetected ? Theme.success : Theme.warning
                        font.pixelSize: 15
                        font.bold: true
                    }
                    Item { Layout.fillHeight: true }
                }

                SelectionView {
                    view: root.view
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.text
                        font.pixelSize: 16
                        text: "Mod archives must be present in the workspace folder before validation. Place the archives your guide lists there, then continue to validation. Installation always needs a fresh passing validation."
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.warning
                        font.pixelSize: 14
                        text: "In-app downloading is not available in this build yet."
                    }
                    Item { Layout.fillHeight: true }
                }

                ValidationView {
                    view: root.view
                    activity: root.activity
                    embedded: true
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: reviewColumn.implicitHeight + 32
                        radius: Theme.radius
                        color: Theme.panel
                        border.color: Theme.border
                        ColumnLayout {
                            id: reviewColumn
                            anchors.fill: parent
                            anchors.margins: 16
                            spacing: 6
                            Label {
                                text: (root.view.selectedBase ?? 0) + " mods selected"
                                color: Theme.text
                                font.pixelSize: 20
                                font.bold: true
                            }
                            Label {
                                Layout.fillWidth: true
                                text: "Game folder: " + (root.view.game ?? "")
                                color: Theme.muted
                                font.pixelSize: 15
                                wrapMode: Text.WrapAnywhere
                            }
                            Label {
                                Layout.fillWidth: true
                                text: "Mod workspace: " + (root.view.mods ?? "")
                                color: Theme.muted
                                font.pixelSize: 15
                                wrapMode: Text.WrapAnywhere
                            }
                            Label {
                                text: "Validation passed for this selection and these folders."
                                color: Theme.success
                                font.pixelSize: 15
                                visible: root.view.validation?.success ?? false
                            }
                        }
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 15
                        text: "Choose Next to review the snapshot and start the installation."
                    }
                    Item { Layout.fillHeight: true }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: statusColumn.implicitHeight + 28
                        radius: Theme.radius
                        color: Theme.panel
                        border.width: 2
                        border.color: root.installFailed ? Theme.danger : root.installDone ? Theme.success : Theme.border
                        ColumnLayout {
                            id: statusColumn
                            anchors.fill: parent
                            anchors.margins: 14
                            spacing: 8
                            Label {
                                text: root.installFailed ? "Installation stopped"
                                    : root.installDone ? "Installation complete" : "Installing\u2026"
                                color: root.installFailed ? Theme.danger : root.installDone ? Theme.success : Theme.text
                                font.pixelSize: 22
                                font.bold: true
                                Accessible.name: text
                            }
                            Label {
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                color: Theme.text
                                font.pixelSize: 15
                                visible: text.length > 0
                                text: root.installFailed ? (root.view.install?.error ?? "")
                                    : root.installProgress
                                      ? (root.installProgress.component ? root.installProgress.component + " \u2014 " : "") + root.installProgress.message : ""
                            }
                            ProgressBar {
                                Layout.fillWidth: true
                                visible: !root.installFailed && !root.installDone
                                from: 0
                                to: Math.max(1, root.installProgress?.componentTotal ?? 1)
                                value: root.installProgress?.componentIndex ?? 0
                                indeterminate: (root.installProgress?.componentTotal ?? 0) === 0
                                Accessible.name: "Installation progress"
                            }
                            Label {
                                visible: !root.installFailed && !root.installDone && (root.installProgress?.componentTotal ?? 0) > 0
                                color: Theme.muted
                                font.pixelSize: 14
                                text: "Mod " + (root.installProgress?.componentIndex ?? 0) + " of " + (root.installProgress?.componentTotal ?? 0)
                            }
                            Flow {
                                Layout.fillWidth: true
                                spacing: Theme.gap
                                ActionButton {
                                    text: "Stop Install"
                                    danger: true
                                    visible: root.view.install?.active ?? false
                                    onClicked: WizardBackend.requestStop()
                                }
                                ActionButton {
                                    text: "Restore snapshot"
                                    danger: true
                                    visible: (root.view.install?.pending ?? false) && !(root.view.install?.active ?? false)
                                    enabled: !root.view.busy
                                    onClicked: WizardBackend.restoreSnapshot()
                                }
                                ActionButton {
                                    text: "Revalidate phase"
                                    visible: (root.view.install?.failed ?? false) && !(root.view.install?.pending ?? false)
                                    enabled: !root.view.busy
                                    onClicked: WizardBackend.revalidatePhase()
                                }
                            }
                        }
                    }
                    Label {
                        text: "What's Happening"
                        color: Theme.text
                        font.pixelSize: 16
                        font.bold: true
                    }
                    Frame {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumHeight: 120
                        padding: 0
                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.panel
                            border.color: Theme.border
                        }
                        ListView {
                            id: logView
                            anchors.fill: parent
                            anchors.margins: 8
                            clip: true
                            model: root.activity.logs ?? []
                            onCountChanged: positionViewAtEnd()
                            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                            Accessible.role: Accessible.List
                            Accessible.name: "Activity log"
                            delegate: Label {
                                required property string modelData
                                width: ListView.view.width
                                text: modelData
                                color: Theme.muted
                                font.pixelSize: 13
                                wrapMode: Text.WrapAnywhere
                            }
                        }
                    }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    SelectionView {
                        Layout.fillHeight: true
                        Layout.fillWidth: true
                        view: root.view
                        widescreen: true
                    }
                    ValidationView {
                        Layout.fillWidth: true
                        visible: (root.view.selectedWidescreen ?? 0) > 0
                        view: root.view
                        activity: root.activity
                        embedded: true
                    }
                }

                ColumnLayout {
                    spacing: Theme.gap
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.text
                        font.pixelSize: 16
                        text: root.page === "Finished" ? "All selected mods are installed. You can close ModSync, or start again with another guide."
                            : root.page === "WidescreenComplete" ? "Every selected widescreen mod is installed." : "The base mod installation finished successfully. Choose Next to continue."
                    }
                    ScrollView {
                        id: epilogue
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        visible: root.page === "Finished" && (root.view.epilogue ?? "").length > 0
                        clip: true
                        contentWidth: availableWidth
                        Label {
                            width: epilogue.availableWidth
                            wrapMode: Text.Wrap
                            textFormat: Text.MarkdownText
                            color: Theme.text
                            font.pixelSize: 15
                            text: root.view.epilogue ?? ""
                            onLinkActivated: link => Qt.openUrlExternally(link)
                        }
                    }
                    Item { Layout.fillHeight: true }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.border
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 16
            spacing: Theme.gap
            ActionButton {
                text: "Back"
                enabled: root.view.canBack ?? false
                onClicked: WizardBackend.back()
            }
            Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                text: root.view.loaded && !(root.view.canNext ?? false) && !root.lastPage ? root.view.hint : ""
                color: Theme.muted
                font.pixelSize: 14
                elide: Text.ElideRight
            }
            ActionButton {
                text: "Restart wizard"
                visible: root.lastPage || root.page === "BaseInstallComplete"
                enabled: !root.view.busy
                onClicked: WizardBackend.restart()
            }
            ActionButton {
                text: root.lastPage ? "Finish" : "Next"
                primary: true
                enabled: root.lastPage ? true : (root.view.canNext ?? false)
                onClicked: root.lastPage ? Qt.quit() : WizardBackend.next()
            }
        }
    }

    Shortcut {
        sequence: "Alt+Right"
        enabled: (root.view.canNext ?? false) && !root.lastPage
        onActivated: WizardBackend.next()
    }
    Shortcut {
        sequence: "Alt+Left"
        enabled: root.view.canBack ?? false
        onActivated: WizardBackend.back()
    }
}
