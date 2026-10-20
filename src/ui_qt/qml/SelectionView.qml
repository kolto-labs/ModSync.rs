pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import org.modsync.desktop

ColumnLayout {
    id: root
    required property var view
    property bool widescreen: false
    spacing: Theme.gap

    ListModel { id: rows }

    function rebuild() {
        const source = (root.view.components ?? []).filter(c => c.widescreen === root.widescreen && c.visible)
        let same = source.length === rows.count
        for (let i = 0; same && i < source.length; ++i) same = rows.get(i).modelIndex === source[i].index
        if (!same) {
            rows.clear()
            for (const c of source) {
                rows.append({
                    modelIndex: c.index, name: c.name, description: c.description, tier: c.tier,
                    categoriesText: c.categories.join(", "), selected: c.selected, requires: c.requires
                })
            }
            return
        }
        for (let i = 0; i < source.length; ++i) {
            if (rows.get(i).selected !== source[i].selected) rows.setProperty(i, "selected", source[i].selected)
        }
    }
    onViewChanged: rebuild()
    Component.onCompleted: rebuild()

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.gap
        visible: !root.widescreen
        TextField {
            id: search
            Layout.fillWidth: true
            Layout.minimumWidth: 160
            placeholderText: "Search mods"
            text: root.view.search ?? ""
            color: Theme.text
            placeholderTextColor: Theme.muted
            font.pixelSize: 15
            implicitHeight: 40
            Accessible.name: "Search mods"
            onTextEdited: WizardBackend.setSearch(text)
            background: Rectangle {
                radius: Theme.radius
                color: Theme.raised
                border.width: search.activeFocus ? 2 : 1
                border.color: search.activeFocus ? Theme.focus : Theme.border
            }
        }
        ComboBox {
            id: categoryBox
            Layout.preferredWidth: 220
            implicitHeight: 40
            font.pixelSize: 15
            model: ["All categories"].concat(root.view.categories ?? [])
            currentIndex: Math.max(0, model.indexOf(root.view.category ?? ""))
            Accessible.name: "Category filter"
            onActivated: WizardBackend.setCategoryFilter(currentIndex === 0 ? "" : currentText)
            contentItem: Text {
                leftPadding: 12
                text: categoryBox.displayText
                color: Theme.text
                font: categoryBox.font
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            background: Rectangle {
                radius: Theme.radius
                color: Theme.raised
                border.width: categoryBox.visualFocus ? 3 : 1
                border.color: categoryBox.visualFocus ? Theme.focus : Theme.border
            }
            delegate: ItemDelegate {
                id: option
                required property var modelData
                required property int index
                width: categoryBox.width
                highlighted: categoryBox.highlightedIndex === index
                contentItem: Text {
                    text: option.modelData
                    color: Theme.text
                    font.pixelSize: 15
                    elide: Text.ElideRight
                }
                background: Rectangle { color: option.highlighted ? Theme.raised : Theme.panel }
            }
            popup.background: Rectangle {
                color: Theme.panel
                border.color: Theme.border
                radius: Theme.radius
            }
        }
        ActionButton {
            text: "Select category"
            enabled: !root.view.busy && categoryBox.currentIndex > 0
            onClicked: WizardBackend.selectCategory(categoryBox.currentText)
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.gap
        ActionButton {
            text: "Select all"
            visible: !root.widescreen
            enabled: !root.view.busy
            onClicked: WizardBackend.selectAll()
        }
        ActionButton {
            text: "Deselect all"
            visible: !root.widescreen
            enabled: !root.view.busy
            onClicked: WizardBackend.deselectAll()
        }
        Item { Layout.fillWidth: true }
        Label {
            text: "Required: " + (root.view.required ?? 0) + "   Optional: " + (root.view.optional ?? 0)
            color: Theme.muted
            font.pixelSize: 14
            visible: !root.widescreen
        }
    }

    Label {
        Layout.fillWidth: true
        text: root.widescreen
            ? "Widescreen build: " + (root.view.selectedWidescreen ?? 0) + " mods selected"
            : "Base build: " + (root.view.summary ?? "")
        color: Theme.text
        font.pixelSize: 17
        font.bold: true
        wrapMode: Text.Wrap
        Accessible.name: text
    }

    Frame {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: 160
        padding: 0
        background: Rectangle {
            radius: Theme.radius
            color: Theme.panel
            border.color: Theme.border
        }
        ListView {
            id: list
            anchors.fill: parent
            anchors.margins: 1
            clip: true
            model: rows
            boundsBehavior: Flickable.StopAtBounds
            activeFocusOnTab: true
            keyNavigationEnabled: true
            Accessible.role: Accessible.List
            Accessible.name: "Mods"
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: ItemDelegate {
                id: row
                required property int index
                required property int modelIndex
                required property string name
                required property string description
                required property string tier
                required property string categoriesText
                required property bool selected
                required property int requires
                width: ListView.view.width
                implicitHeight: Math.max(56, body.implicitHeight + 16)
                hoverEnabled: true
                Accessible.role: Accessible.CheckBox
                Accessible.name: name
                Accessible.checked: selected
                Accessible.checkable: true
                Accessible.onToggleAction: WizardBackend.setComponent(modelIndex, !selected)
                enabled: !root.view.busy
                onClicked: WizardBackend.setComponent(modelIndex, !selected)
                background: Rectangle {
                    color: row.highlighted || row.visualFocus ? Theme.raised : row.hovered ? Qt.rgba(1, 1, 1, 0.04) : "transparent"
                    border.width: row.visualFocus ? 2 : 0
                    border.color: Theme.focus
                }
                contentItem: RowLayout {
                    id: body
                    spacing: Theme.gap
                    Rectangle {
                        Layout.preferredWidth: 22
                        Layout.preferredHeight: 22
                        Layout.alignment: Qt.AlignTop
                        radius: 5
                        color: row.selected ? Theme.accent : "transparent"
                        border.width: 2
                        border.color: row.selected ? Theme.accent : Theme.muted
                        Text {
                            anchors.centerIn: parent
                            text: "\u2713"
                            visible: row.selected
                            color: Theme.accentText
                            font.pixelSize: 15
                            font.bold: true
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Label {
                            Layout.fillWidth: true
                            text: row.name
                            color: Theme.text
                            font.pixelSize: 15
                            font.bold: true
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            text: row.description
                            visible: text.length > 0
                            color: Theme.muted
                            font.pixelSize: 13
                            maximumLineCount: 2
                            wrapMode: Text.Wrap
                            elide: Text.ElideRight
                        }
                    }
                    Label {
                        text: [row.tier, row.categoriesText].filter(s => s.length > 0).join(" \u00b7 ")
                        visible: text.length > 0
                        color: Theme.muted
                        font.pixelSize: 12
                        Layout.maximumWidth: 240
                        elide: Text.ElideRight
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                visible: list.count === 0
                text: root.view.loaded ? "No mods match the current filter." : "Load a guide first."
                color: Theme.muted
                font.pixelSize: 16
            }
        }
    }
}
