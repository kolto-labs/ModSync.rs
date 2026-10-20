pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

Popup {
    id: popup
    required property var host
    required property var view
    property string tab: "general"
    property string profileName: ""
    property bool confirmDelete: false
    readonly property var settings: host.settings ?? ({})
    readonly property var telemetry: host.telemetry ?? ({})
    readonly property var peer: host.peer ?? ({})
    readonly property var sharing: host.sharing ?? ({})
    readonly property var profiles: host.profiles ?? ({names: [], active: ""})
    readonly property var patcher: host.patcher ?? ({engine: "OdyPatcher", path: ""})
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(820, (parent ? parent.width : 820) - 48)
    height: Math.min(640, (parent ? parent.height : 640) - 48)
    padding: Theme.pad
    closePolicy: Popup.CloseOnEscape
    background: Rectangle {
        radius: Theme.radius + 4
        color: Theme.panel
        border.color: Theme.border
    }
    Overlay.modal: Rectangle { color: "#99000000" }

    function save(name, value) {
        const edit = {}
        edit[name] = value
        WizardBackend.saveSettings(JSON.stringify(edit))
    }
    function saveTelemetry(name, value) {
        const edit = {}
        edit[name] = value
        WizardBackend.saveTelemetry(JSON.stringify(edit))
    }
    function peerChange(key, value) {
        WizardBackend.peerChange(key, JSON.stringify(value))
    }

    contentItem: ColumnLayout {
        spacing: Theme.gap
        RowLayout {
            Layout.fillWidth: true
            Label {
                Layout.fillWidth: true
                text: "Settings"
                color: Theme.text
                font.pixelSize: 24
                font.bold: true
                Accessible.role: Accessible.Heading
                Accessible.name: "Settings"
            }
            ActionButton {
                text: "Close"
                onClicked: popup.close()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Repeater {
                model: [
                    {label: "General", key: "general"},
                    {label: "Privacy", key: "privacy"},
                    {label: "Sharing", key: "sharing"},
                    {label: "Profiles", key: "profiles"},
                    {label: "Patcher and Nexus", key: "tools"}
                ]
                ActionButton {
                    required property var modelData
                    text: modelData.label
                    checkable: true
                    checked: popup.tab === modelData.key
                    quiet: true
                    onClicked: popup.tab = modelData.key
                }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: (popup.host.status ?? "").length > 0
            text: popup.host.status ?? ""
            color: Theme.muted
            font.pixelSize: 14
            wrapMode: Text.Wrap
            Accessible.role: Accessible.StaticText
            Accessible.name: text
        }
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: ["general", "privacy", "sharing", "profiles", "tools"].indexOf(popup.tab)

            ScrollView {
                clip: true
                ColumnLayout {
                    width: popup.width - 2 * Theme.pad - 20
                    spacing: 2
                    Label {
                        text: "Desktop preferences are stored in " + (popup.host.directory ?? "")
                        color: Theme.muted
                        font.pixelSize: 13
                        Layout.fillWidth: true
                        wrapMode: Text.WrapAnywhere
                    }
                    CheckRow { text: "Spoiler-free mode"; checked: popup.settings.spoilerFree ?? false; onToggled: popup.save("spoilerFree", checked) }
                    CheckRow { text: "Debug logging"; checked: popup.settings.debugLogging ?? false; onToggled: popup.save("debugLogging", checked) }
                    CheckRow { text: "Watch files for changes"; checked: popup.settings.fileWatcher ?? false; onToggled: popup.save("fileWatcher", checked) }
                    CheckRow { text: "Case-insensitive pathing"; checked: popup.settings.caseInsensitive ?? false; onToggled: popup.save("caseInsensitive", checked) }
                    CheckRow { text: "Attempt automatic fixes"; checked: popup.settings.attemptFixes ?? false; onToggled: popup.save("attemptFixes", checked) }
                    CheckRow { text: "Do not request administrator rights"; checked: popup.settings.noAdmin ?? false; onToggled: popup.save("noAdmin", checked) }
                    CheckRow { text: "Deep archive check"; checked: popup.settings.archiveDeepCheck ?? false; onToggled: popup.save("archiveDeepCheck", checked) }
                    CheckRow { text: "Multi-threaded file operations"; checked: popup.settings.multiThreadedIo ?? false; onToggled: popup.save("multiThreadedIo", checked) }
                    CheckRow { text: "Copy instead of move for move actions"; checked: popup.settings.copyForMove ?? false; onToggled: popup.save("copyForMove", checked) }
                    CheckRow { text: "Validate and replace invalid archives"; checked: popup.settings.validateArchives ?? false; onToggled: popup.save("validateArchives", checked) }
                    CheckRow { text: "Filter downloads by resolution"; checked: popup.settings.filterByResolution ?? false; onToggled: popup.save("filterByResolution", checked) }
                    CheckRow { text: "Managed deployment"; checked: popup.settings.managed ?? false; onToggled: popup.save("managed", checked) }
                }
            }

            ScrollView {
                clip: true
                ColumnLayout {
                    width: popup.width - 2 * Theme.pad - 20
                    spacing: 4
                    Label {
                        text: "Desktop settings and privacy"
                        color: Theme.text
                        font.pixelSize: 18
                        font.bold: true
                    }
                    CheckRow { text: "Allow configured telemetry"; checked: popup.telemetry.enabled ?? false; onToggled: popup.saveTelemetry("enabled", checked) }
                    CheckRow { text: "Usage events"; checked: popup.telemetry.usage ?? false; onToggled: popup.saveTelemetry("usage", checked) }
                    CheckRow { text: "Performance metrics"; checked: popup.telemetry.performance ?? false; onToggled: popup.saveTelemetry("performance", checked) }
                    CheckRow { text: "Hashed error reports"; checked: popup.telemetry.crash ?? false; onToggled: popup.saveTelemetry("crash", checked) }
                    Label {
                        Layout.fillWidth: true
                        text: popup.telemetry.summary ?? ""
                        color: Theme.muted
                        font.pixelSize: 13
                        wrapMode: Text.Wrap
                    }
                }
            }

            ScrollView {
                clip: true
                ColumnLayout {
                    width: popup.width - 2 * Theme.pad - 20
                    spacing: 6
                    Label {
                        text: "Completed sharing"
                        color: Theme.text
                        font.pixelSize: 18
                        font.bold: true
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 14
                        text: popup.sharing.available
                            ? (popup.sharing.state ?? "Idle") + ": " + (popup.sharing.uploadedBytes ?? 0) + " bytes uploaded. " + (popup.sharing.status ?? "")
                            : (popup.sharing.status ?? "Sharing is not running in this session.")
                    }
                    RowLayout {
                        spacing: Theme.gap
                        ActionButton {
                            text: "Stop sharing"
                            enabled: (popup.sharing.available ?? false) && !(popup.sharing.busy ?? false)
                            onClicked: WizardBackend.stopSharing()
                        }
                        ActionButton {
                            text: "Resume sharing"
                            enabled: (popup.sharing.available ?? false) && !(popup.sharing.busy ?? false)
                            onClicked: WizardBackend.resumeSharing()
                        }
                    }
                    Label {
                        text: "Peer settings"
                        color: Theme.text
                        font.pixelSize: 18
                        font.bold: true
                        Layout.topMargin: 10
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 13
                        text: "Sharing Off prevents peer discovery and transfers. Changes apply to the next run."
                    }
                    CheckRow { text: "Sharing enabled"; checked: popup.peer.sharing ?? false; onToggled: popup.peerChange("sharing", checked) }
                    RowLayout {
                        spacing: Theme.gap
                        Label { text: "Mode:"; color: Theme.text; font.pixelSize: 15 }
                        Repeater {
                            model: [{label: "Off", key: "off"}, {label: "Race", key: "race"}, {label: "Fallback", key: "fallback"}]
                            ActionButton {
                                required property var modelData
                                text: modelData.label
                                checkable: true
                                checked: (popup.peer.mode ?? "race") === modelData.key
                                onClicked: popup.peerChange("mode", modelData.key)
                            }
                        }
                    }
                    RowLayout {
                        spacing: Theme.gap
                        Label { text: "Completed-sharing upload cap (KiB/s):"; color: Theme.text; font.pixelSize: 15 }
                        SpinBox {
                            from: 16
                            to: 1000000
                            editable: true
                            value: popup.peer.uploadKiBps ?? 100
                            Accessible.name: "Completed-sharing upload cap in KiB per second"
                            onValueModified: popup.peerChange("uploadKiBps", value)
                        }
                    }
                    CheckRow { text: "Keep sharing verified completed archives"; checked: popup.peer.keepSharing ?? false; onToggled: popup.peerChange("keepSharing", checked) }
                }
            }

            ColumnLayout {
                spacing: Theme.gap
                RowLayout {
                    spacing: Theme.gap
                    InputField {
                        id: profileField
                        Layout.fillWidth: true
                        placeholderText: "Profile name"
                        text: popup.profileName
                        onTextEdited: {
                            popup.profileName = text
                            popup.confirmDelete = false
                        }
                        Accessible.name: "Profile name"
                    }
                    ActionButton {
                        text: "Save current selection"
                        primary: true
                        enabled: popup.profileName.trim().length > 0 && (popup.view.loaded ?? false)
                        onClicked: WizardBackend.profileAction("save", popup.profileName, "")
                    }
                }
                Label {
                    visible: (popup.profiles.active ?? "").length > 0
                    text: "Active profile: " + (popup.profiles.active ?? "")
                    color: Theme.success
                    font.pixelSize: 14
                }
                Frame {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    padding: 0
                    background: Rectangle { radius: Theme.radius; color: Theme.window; border.color: Theme.border }
                    ListView {
                        anchors.fill: parent
                        anchors.margins: 1
                        clip: true
                        model: popup.profiles.names ?? []
                        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                        delegate: ItemDelegate {
                            id: profileRow
                            required property string modelData
                            width: ListView.view.width
                            height: 40
                            highlighted: popup.profileName === modelData
                            Accessible.name: modelData
                            contentItem: Text {
                                text: profileRow.modelData + (profileRow.modelData === popup.profiles.active ? "  (active)" : "")
                                color: Theme.text
                                font.pixelSize: 15
                                verticalAlignment: Text.AlignVCenter
                            }
                            background: Rectangle { color: profileRow.highlighted ? Theme.raised : "transparent" }
                            onClicked: {
                                popup.profileName = modelData
                                popup.confirmDelete = false
                            }
                        }
                    }
                }
                RowLayout {
                    spacing: Theme.gap
                    InputField {
                        id: targetField
                        Layout.fillWidth: true
                        placeholderText: "New name for Clone or Rename"
                        Accessible.name: "New profile name"
                    }
                    ActionButton { text: "Clone"; enabled: popup.profileName.length > 0 && targetField.text.length > 0; onClicked: WizardBackend.profileAction("clone", popup.profileName, targetField.text) }
                    ActionButton { text: "Rename"; enabled: popup.profileName.length > 0 && targetField.text.length > 0; onClicked: WizardBackend.profileAction("rename", popup.profileName, targetField.text) }
                }
                RowLayout {
                    spacing: Theme.gap
                    ActionButton { text: "Activate"; primary: true; enabled: popup.profileName.length > 0 && (popup.view.loaded ?? false); onClicked: WizardBackend.profileAction("activate", popup.profileName, "") }
                    ActionButton { text: "Refresh"; onClicked: WizardBackend.profileAction("refresh", "", "") }
                    Item { Layout.fillWidth: true }
                    ActionButton {
                        text: popup.confirmDelete ? "Confirm delete" : "Delete"
                        danger: true
                        enabled: popup.profileName.length > 0
                        onClicked: {
                            if (popup.confirmDelete) {
                                WizardBackend.profileAction("delete", popup.profileName, "")
                                popup.confirmDelete = false
                                popup.profileName = ""
                            } else {
                                popup.confirmDelete = true
                            }
                        }
                    }
                }
            }

            ScrollView {
                clip: true
                ColumnLayout {
                    width: popup.width - 2 * Theme.pad - 20
                    spacing: Theme.gap
                    Label { text: "Patcher"; color: Theme.text; font.pixelSize: 18; font.bold: true }
                    RowLayout {
                        spacing: Theme.gap
                        Repeater {
                            model: ["OdyPatcher", "KPatcher", "HoloPatcher"]
                            ActionButton {
                                required property string modelData
                                text: modelData
                                checkable: true
                                checked: popup.patcher.engine === modelData
                                onClicked: WizardBackend.saveSettings(JSON.stringify({patcherEngine: modelData, patcherPath: patcherPath.text}))
                            }
                        }
                    }
                    RowLayout {
                        spacing: Theme.gap
                        InputField {
                            id: patcherPath
                            Layout.fillWidth: true
                            placeholderText: "Patcher executable (optional)"
                            text: popup.patcher.path ?? ""
                            Accessible.name: "Patcher executable"
                        }
                        ActionButton {
                            text: "Save patcher"
                            onClicked: WizardBackend.saveSettings(JSON.stringify({patcherEngine: popup.patcher.engine, patcherPath: patcherPath.text}))
                        }
                    }
                    Label { text: "Nexus Mods"; color: Theme.text; font.pixelSize: 18; font.bold: true; Layout.topMargin: 10 }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.muted
                        font.pixelSize: 14
                        text: popup.settings.hasNexusKey ? "A Nexus key is stored." : "No Nexus key is stored."
                    }
                    RowLayout {
                        spacing: Theme.gap
                        InputField {
                            id: keyField
                            Layout.fillWidth: true
                            echoMode: TextInput.Password
                            placeholderText: "Nexus Mods API key"
                            Accessible.name: "Nexus Mods API key"
                        }
                        ActionButton {
                            text: "Validate and save key"
                            primary: true
                            enabled: keyField.text.trim().length > 0 && !(popup.settings.keyBusy ?? false)
                            onClicked: {
                                WizardBackend.saveNexusKey(keyField.text)
                                keyField.text = ""
                            }
                        }
                    }
                }
            }
        }
    }
}
