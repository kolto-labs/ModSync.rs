pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Dialogs
import Qt.labs.qmlmodels
import org.modsync.desktop

ApplicationWindow {
    id: window
    width: 1320
    height: 860
    minimumWidth: 1000
    minimumHeight: 660
    visible: true
    title: "ModSync — Easy-mode instruction builder"
    color: "#101820"
    font.family: regularFont.name
    font.pixelSize: 14
    palette.window: "#101820"
    palette.windowText: "#e4edf3"
    palette.text: "#e4edf3"
    palette.placeholderText: "#91a7b5"
    palette.buttonText: "#e4edf3"
    palette.base: "#15232d"
    palette.button: "#243643"
    palette.highlight: "#27766e"
    palette.highlightedText: "#ffffff"

    FontLoader {
        id: regularFont
        source: "qrc:/qt/qml/org/modsync/desktop/src/ui_qt/assets/fonts/NotoSans-Regular.ttf"
    }
    FontLoader {
        source: "qrc:/qt/qml/org/modsync/desktop/src/ui_qt/assets/fonts/NotoSans-Bold.ttf"
    }
    readonly property var backend: AuthoringBackend
    property var document: ({})
    property int componentIndex: 0
    property string folder: ""
    property string selectedPath: ""
    property var selectedPaths: []
    property string selectionAnchor: ""
    property string requestedAction: "Copy"
    property var rows: []
    property var history: []
    property bool immediateActions: false
    property int presentation: 0
    property int sortColumn: 0
    property bool sortAscending: true
    property bool resetGuidePosition: false
    property int snapshotGeneration: 0
    property double operationStartedAt: 0
    property int operationElapsedSeconds: 0
    property var activityHistory: []
    readonly property string elapsedText: Math.floor(operationElapsedSeconds / 60) + ":" + String(operationElapsedSeconds % 60).padStart(2, "0")
    readonly property string operationPhase: backend.status.startsWith("inventory:") ? "Reading file inventory" : backend.status.startsWith("objects:") ? "Saving recovery files" : backend.status.startsWith("committed:") ? "Recovery snapshot committed" : backend.status
    function rememberStatus() {
        if (!backend.status || activityHistory[activityHistory.length - 1] === backend.status) return;
        activityHistory = activityHistory.concat([backend.status.slice(0, 4096)]).slice(-128);
    }
    Timer {
        interval: 1000
        repeat: true
        running: backend.busy
        onTriggered: window.operationElapsedSeconds = Math.floor((Date.now() - window.operationStartedAt) / 1000)
    }
    readonly property var currentMod: window.document.components ? window.document.components[window.componentIndex] : null

    function confirmDisk(kind, target) {
        const disk = window.document.disk;
        if (!disk || backend.busy)
            return;
        diskConfirmation.kind = kind;
        diskConfirmation.target = target;
        diskConfirmation.cursor = disk.cursor;
        diskConfirmation.revision = window.document.revisionId;
        diskConfirmation.action = disk.nextAction ? Object.assign({}, disk.nextAction) : null;
        diskConfirmation.mods = window.document.mods;
        diskConfirmation.game = window.document.game;
        diskConfirmation.roots = "Mod folders: " + window.document.mods + "\nGame folder: " + window.document.game;
        diskConfirmation.open();
    }
    function cleanPath(path) {
        const normalized = (path || "").replace(/\\/g, "/");
        return normalized === "/" || /^[A-Za-z]:\/$/.test(normalized) ? normalized : normalized.replace(/\/+$/, "");
    }
    function isRoot(path) {
        return window.cleanPath(path) === window.cleanPath(window.document.mods) || window.cleanPath(path) === window.cleanPath(window.document.game);
    }
    function isUnder(path, root) {
        return !!root && (path === root || path.startsWith(root.endsWith("/") ? root : root + "/"));
    }
    function parentPath(path) {
        let normalized = window.cleanPath(path);
        let slash = normalized.lastIndexOf("/");
        if (slash === 0)
            return "/";
        if (slash === 2 && /^[A-Za-z]:/.test(normalized))
            return normalized.slice(0, 3);
        return normalized.slice(0, slash);
    }
    function nameOf(path) {
        return path.replace(/\\/g, "/").split("/").pop();
    }
    function breadcrumbs() {
        const path = window.cleanPath(window.folder);
        const mods = window.cleanPath(window.document.mods);
        const game = window.cleanPath(window.document.game);
        const inMods = window.isUnder(path, mods);
        const root = inMods ? mods : game;
        if (!window.isUnder(path, root))
            return [];
        let result = [
            {
                name: inMods ? "Mod sources" : "Game",
                path: root
            }
        ];
        let full = root;
        for (const segment of path.slice(root.length).split("/").filter(s => s.length)) {
            full += (full.endsWith("/") ? "" : "/") + segment;
            result.push({
                name: segment,
                path: full
            });
        }
        return result;
    }
    function clearSelection() {
        window.selectedPath = "";
        window.selectedPaths = [];
        window.selectionAnchor = "";
    }
    function selectEntry(path, siblings, modifiers) {
        let selected = window.selectedPaths.slice();
        const toggle = modifiers & (Qt.ControlModifier | Qt.MetaModifier);
        if ((modifiers & Qt.ShiftModifier) && window.selectionAnchor) {
            const paths = siblings.map(e => e.path);
            const first = paths.indexOf(window.selectionAnchor), last = paths.indexOf(path);
            if (first >= 0 && last >= 0) {
                const range = paths.slice(Math.min(first, last), Math.max(first, last) + 1);
                selected = toggle ? Array.from(new Set(selected.concat(range))) : range;
            } else
                selected = [path];
        } else if (toggle) {
            selected = selected.includes(path) ? selected.filter(p => p !== path) : selected.concat([path]);
            window.selectionAnchor = path;
        } else {
            selected = [path];
            window.selectionAnchor = path;
        }
        window.selectedPaths = selected;
        window.selectedPath = selected.includes(path) ? path : (selected[selected.length - 1] || "");
    }
    function openRecord(action) {
        window.requestedAction = action;
        if (action === "CreateDirectory") window.immediateActions = false;
        recordDialog.editDefinition = null;
        recordDialog.sourcePreset = "";
        recordDialog.argumentsPreset = "";
        recordDialog.componentGuid = window.currentMod.guid;
        recordDialog.position = actionPoint.value;
        recordDialog.revisionId = window.document.revisionId;
        recordDialog.open();
    }
    function selectMod(index) {
        window.componentIndex = index;
        actionPoint.value = 0;
        window.document.entries = [];
        window.clearSelection();
        window.updateRows();
    }
    function isArchive(path) {
        return /\.(zip|7z|rar|tar|tgz|gz|bz2|xz)$/i.test(path);
    }
    function inspectSelectedPatcher() {
        if (!window.currentMod || window.selectedPaths.length !== 1 || backend.busy)
            return;
        patcherDialog.component = window.componentIndex;
        patcherDialog.componentGuid = window.currentMod.guid;
        patcherDialog.point = actionPoint.value;
        patcherDialog.revisionId = window.document.revisionId;
        patcherSource.text = window.selectedPath;
        window.document = Object.assign({}, window.document, {patcher: null});
        patcherDialog.open();
    }
    function activateEntry(entry) {
        if (entry.directory)
            window.navigate(entry.path);
        else if (window.isArchive(entry.path)) {
            window.selectedPaths = [entry.path];
            window.selectedPath = entry.path;
            window.openRecord("Extract");
        }
    }
    function toggleSort(column) {
        window.sortAscending = window.sortColumn === column ? !window.sortAscending : true;
        window.sortColumn = column;
        window.updateRows();
    }
    function goBack() {
        if (!window.history.length)
            return;
        let previous = window.history.slice();
        window.folder = previous.pop();
        window.history = previous;
        window.clearSelection();
        window.updateRows();
        Qt.callLater(window.focusBrowserEntry);
    }
    function childrenOf(folder) {
        let entries = window.document.entries || [];
        let needle = folder === window.folder ? search.text.toLocaleLowerCase() : "";
        return entries.filter(e => window.parentPath(e.path) === window.cleanPath(folder) && window.cleanPath(e.path) !== window.cleanPath(folder) && window.nameOf(e.path).toLocaleLowerCase().includes(needle)).sort((a, b) => {
            if (a.directory !== b.directory)
                return a.directory ? -1 : 1;
            let result = window.sortColumn === 2 ? (a.bytes ?? -1) - (b.bytes ?? -1) : window.sortColumn === 1 ? Number(a.content_known) - Number(b.content_known) : 0;
            if (result === 0)
                result = window.nameOf(a.path).localeCompare(window.nameOf(b.path));
            return window.sortAscending ? result : -result;
        });
    }
    function updateRows() {
        window.rows = window.childrenOf(window.folder);
    }
    function formatBytes(bytes) {
        if (bytes === null || bytes === undefined)
            return "Unknown";
        let value = Number(bytes), unit = 0;
        const units = ["B", "KiB", "MiB", "GiB", "TiB"];
        while (value >= 1024 && unit < units.length - 1) {
            value /= 1024;
            unit++;
        }
        return value.toFixed(unit ? 1 : 0) + " " + units[unit];
    }
    function focusBrowserEntry() {
        let view = window.presentation === 0 ? explorerView : window.presentation === 1 ? dolphinView : null;
        if (window.presentation === 2 && finderColumns.count) {
            const column = finderColumns.itemAt(finderColumns.count - 1);
            view = column ? column.browserView : null;
        }
        if (!view || !view.count)
            return;
        view.currentIndex = 0;
        view.positionViewAtIndex(0, ListView.Contain);
        view.forceLayout();
        const item = view.itemAtIndex(0);
        if (item)
            item.forceActiveFocus(Qt.OtherFocusReason);
    }
    function navigate(path) {
        const next = window.cleanPath(path);
        if (window.folder && window.folder !== next)
            window.history = window.history.concat([window.folder]);
        window.folder = next;
        window.clearSelection();
        window.updateRows();
        Qt.callLater(window.focusBrowserEntry);
    }
    function updateTree() {
        const mods = window.cleanPath(window.document.mods), game = window.cleanPath(window.document.game);
        if (!mods || !game) {
            folderModel.rows = [];
            return;
        }
        const nodes = new Map();
        for (const item of [
            {
                name: "Mod sources",
                path: mods
            },
            {
                name: "Game",
                path: game
            }
        ])
            nodes.set(item.path, {
                name: item.name,
                path: item.path,
                rows: []
            });
        for (const entry of window.document.entries || []) {
            const path = window.cleanPath(entry.path);
            if (entry.directory && !nodes.has(path) && (window.isUnder(path, mods) || window.isUnder(path, game)))
                nodes.set(path, {
                    name: window.nameOf(path),
                    path: path,
                    rows: []
                });
        }
        for (const [path, node] of nodes) {
            if (path !== mods && path !== game) {
                const parent = nodes.get(window.parentPath(path));
                if (parent)
                    parent.rows.push(node);
            }
        }
        for (const node of nodes.values())
            node.rows.sort((a, b) => a.name.localeCompare(b.name));
        folderModel.rows = [nodes.get(mods), nodes.get(game)];
    }
    function refreshSnapshot() {
        const selectedGuid = window.resetGuidePosition ? "" : (window.currentMod ? window.currentMod.guid : "");
        const guideOffset = window.resetGuidePosition ? 0 : guideList.contentY - guideList.originY;
        window.resetGuidePosition = false;
        const generation = ++window.snapshotGeneration;
        try {
            window.document = JSON.parse(backend.snapshot);
        } catch (error) {
            window.document = {};
        }
        const components = window.document.components || [];
        const selectedIndex = selectedGuid ? components.findIndex(component => component.guid === selectedGuid) : -1;
        window.componentIndex = selectedIndex >= 0 ? selectedIndex : Math.min(window.componentIndex, Math.max(0, components.length - 1));
        Qt.callLater(function() {
            if (generation !== window.snapshotGeneration)
                return;
            guideList.forceLayout();
            guideList.contentY = guideList.originY + Math.max(0, Math.min(guideOffset, Math.max(0, guideList.contentHeight - guideList.height)));
        });
        if (!window.folder && window.document.mods)
            window.folder = window.cleanPath(window.document.mods);
        window.updateRows();
        window.updateTree();
        window.clearSelection();
        if (window.document.recordedPosition !== undefined)
            actionPoint.value = window.document.recordedPosition;
    }
    Component.onCompleted: window.refreshSnapshot()
    Connections {
        target: backend
        function onSnapshotChanged() { window.refreshSnapshot(); }
        function onStatusChanged() { window.rememberStatus(); }
        function onBusyChanged() {
            if (backend.busy) {
                window.operationStartedAt = Date.now();
                window.operationElapsedSeconds = 0;
                window.activityHistory = [];
                window.rememberStatus();
            }
        }
    }
    Shortcut {
        sequence: "Alt+Left"
        enabled: !backend.busy
        onActivated: window.goBack()
    }
    Shortcut {
        sequence: "Alt+Up"
        enabled: !backend.busy && !!window.folder && !window.isRoot(window.folder)
        onActivated: window.navigate(window.parentPath(window.folder))
    }
    Shortcut {
        sequences: [StandardKey.Undo]
        enabled: backend.ready && !backend.busy && !!window.document.canUndo
        onActivated: backend.seekRevision(-1)
    }
    Shortcut {
        sequences: [StandardKey.Redo]
        enabled: backend.ready && !backend.busy && !!window.document.canRedo
        onActivated: backend.seekRevision(1)
    }

    component ActionButton: Button {
        id: control
        implicitHeight: 36
        opacity: enabled ? 1 : 0.5
        leftPadding: 15
        rightPadding: 15
        background: Rectangle {
            radius: 7
            color: control.down ? "#365264" : control.hovered ? "#2c4353" : "#213440"
            border.color: control.activeFocus ? "#70c5b4" : "#344854"
            opacity: control.enabled ? 1 : 0.45
        }
    }
    component BrowserItem: ItemDelegate {
        id: browserItem
        enabled: backend.ready && !backend.busy
        required property var modelData
        property var siblings: window.rows
        property bool columnMode: false
        property var itemView: null
        property int itemsPerRow: 1
        function moveFocus(target, modifiers) {
            if (!itemView || !siblings.length)
                return;
            target = Math.max(0, Math.min(siblings.length - 1, target));
            itemView.currentIndex = target;
            itemView.positionViewAtIndex(target, ListView.Contain);
            itemView.forceLayout();
            const item = itemView.itemAtIndex(target);
            if (item)
                item.forceActiveFocus(Qt.OtherFocusReason);
            if (!(modifiers & (Qt.ControlModifier | Qt.MetaModifier)))
                window.selectEntry(siblings[target].path, siblings, modifiers);
        }
        Rectangle {
            anchors.fill: parent
            z: 2
            color: "transparent"
            radius: 6
            border.width: browserItem.activeFocus ? 2 : 0
            border.color: "#70c5b4"
        }
        highlighted: window.selectedPaths.includes(modelData.path)
        Accessible.name: window.nameOf(modelData.path)
        Accessible.selected: highlighted
        TapHandler {
            id: entryTap
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: (eventPoint, button) => {
                browserItem.forceActiveFocus();
                if (button === Qt.RightButton) {
                    if (!window.selectedPaths.includes(browserItem.modelData.path))
                        window.selectEntry(browserItem.modelData.path, browserItem.siblings, 0);
                    fileMenu.popup();
                } else if (browserItem.columnMode && browserItem.modelData.directory && !entryTap.point.modifiers)
                    window.navigate(browserItem.modelData.path);
                else
                    window.selectEntry(browserItem.modelData.path, browserItem.siblings, entryTap.point.modifiers);
            }
            onDoubleTapped: if (!browserItem.columnMode || !browserItem.modelData.directory)
                window.activateEntry(browserItem.modelData)
            onLongPressed: {
                window.selectEntry(browserItem.modelData.path, browserItem.siblings, 0);
                fileMenu.popup();
            }
        }
        Keys.onPressed: event => {
            const index = siblings.findIndex(entry => entry.path === modelData.path);
            const page = itemView ? Math.max(1, Math.floor(itemView.height / height)) * itemsPerRow : 1;
            let target = null;
            if (event.key === Qt.Key_Up)
                target = index - itemsPerRow;
            else if (event.key === Qt.Key_Down)
                target = index + itemsPerRow;
            else if (event.key === Qt.Key_Home)
                target = 0;
            else if (event.key === Qt.Key_End)
                target = siblings.length - 1;
            else if (event.key === Qt.Key_PageUp)
                target = index - page;
            else if (event.key === Qt.Key_PageDown)
                target = index + page;
            else if (!columnMode && itemsPerRow > 1 && event.key === Qt.Key_Left)
                target = index - 1;
            else if (!columnMode && itemsPerRow > 1 && event.key === Qt.Key_Right)
                target = index + 1;
            if (target !== null) {
                moveFocus(target, event.modifiers);
                event.accepted = true;
            } else if (columnMode && event.key === Qt.Key_Right && modelData.directory) {
                window.navigate(modelData.path);
                event.accepted = true;
            } else if (columnMode && event.key === Qt.Key_Left && !window.isRoot(window.folder)) {
                window.navigate(window.parentPath(window.folder));
                event.accepted = true;
            } else if (event.key === Qt.Key_Space) {
                window.selectEntry(modelData.path, siblings, event.modifiers);
                event.accepted = true;
            } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && (modelData.directory || window.isArchive(modelData.path))) {
                window.activateEntry(modelData);
                event.accepted = true;
            } else if (event.key === Qt.Key_A && (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier))) {
                window.selectedPaths = siblings.map(e => e.path);
                window.selectedPath = modelData.path;
                event.accepted = true;
            } else if (event.key === Qt.Key_F2 && window.selectedPaths.length === 1) {
                window.openRecord("Rename");
                event.accepted = true;
            } else if (event.key === Qt.Key_Delete && window.selectedPaths.length) {
                window.openRecord("Delete");
                event.accepted = true;
            } else if (event.key === Qt.Key_Menu) {
                fileMenu.popup();
                event.accepted = true;
            }
        }
    }
    Menu {
        id: fileMenu
        enabled: backend.ready && !backend.busy && window.selectedPaths.length > 0
        MenuItem {
            text: "Record copy…"
            onTriggered: window.openRecord("Copy")
        }
        MenuItem {
            text: "Record move…"
            onTriggered: window.openRecord("Move")
        }
        MenuItem {
            text: "Record rename…"
            enabled: window.selectedPaths.length === 1
            onTriggered: window.openRecord("Rename")
        }
        MenuSeparator {}
        MenuItem {
            text: "Extract archive…"
            enabled: !!window.currentMod && window.selectedPaths.length > 0 && window.selectedPaths.every(path => window.isArchive(path))
            onTriggered: window.openRecord("Extract")
        }
        MenuItem {
            text: "Inspect patcher…"
            enabled: !!window.currentMod && window.selectedPaths.length === 1
            onTriggered: window.inspectSelectedPatcher()
        }
        MenuSeparator {}
        MenuItem {
            text: "Record delete…"
            onTriggered: window.openRecord("Delete")
        }
    }
    component FileIcon: Canvas {
        property bool directory: false
        width: 24
        height: 24
        onDirectoryChanged: requestPaint()
        onPaint: {
            let c = getContext("2d");
            c.reset();
            c.fillStyle = directory ? "#d7ae69" : "#83adc8";
            if (directory) {
                c.beginPath();
                c.moveTo(2, 6);
                c.lineTo(10, 6);
                c.lineTo(13, 9);
                c.lineTo(22, 9);
                c.lineTo(22, 21);
                c.lineTo(2, 21);
                c.closePath();
                c.fill();
            } else {
                c.fillRect(6, 3, 13, 19);
                c.fillStyle = "#35556c";
                c.fillRect(8, 9, 9, 1);
                c.fillRect(8, 12, 9, 1);
                c.fillRect(8, 15, 7, 1);
            }
        }
    }

    header: Rectangle {
        height: 76
        color: "#172630"
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 24
            anchors.rightMargin: 24
            spacing: 18
            Rectangle {
                Layout.preferredWidth: 38
                Layout.preferredHeight: 38
                radius: 10
                color: "#79c7b6"
                Label {
                    anchors.centerIn: parent
                    text: "M"
                    color: "#102d2c"
                    font.bold: true
                    font.pixelSize: 23
                }
            }
            ColumnLayout {
                spacing: 3
                Label {
                    text: "ModSync"
                    font.bold: true
                    font.pixelSize: 22
                }
                Label {
                    text: "EASY-MODE BUILDER"
                    color: "#91a7b5"
                    font.pixelSize: 11
                    font.letterSpacing: 1.8
                }
            }
            Item {
                Layout.fillWidth: true
            }
            Label {
                text: window.immediateActions ? "Immediate disk actions" : "Virtual filesystem"
                color: "#8cd3c0"
            }
            ActionButton {
                text: "Undo"
                enabled: backend.ready && !backend.busy && !!window.document.canUndo
                onClicked: backend.seekRevision(-1)
            }
            ActionButton {
                text: "Redo"
                enabled: backend.ready && !backend.busy && !!window.document.canRedo
                onClicked: backend.seekRevision(1)
            }
            ActionButton {
                text: "Import archive"
                enabled: backend.ready && !backend.busy
                onClicked: archiveDialog.open()
            }
            ActionButton {
                text: "Session…"
                enabled: !backend.busy
                onClicked: openDialog.open()
            }
        }
    }

    FileDialog {
        id: archiveDialog
        title: "Import mod archive"
        fileMode: FileDialog.OpenFile
        options: FileDialog.DontUseNativeDialog | FileDialog.ReadOnly
        nameFilters: ["Mod archives (*.zip *.7z *.rar)", "All files (*)"]
        onAccepted: backend.importArchive(selectedFile)
    }
    footer: Rectangle {
        height: 44
        color: "#172630"
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 22
            anchors.rightMargin: 22
            spacing: 14
            BusyIndicator {
                running: backend.busy
                visible: running
                Layout.preferredWidth: 25
                Layout.preferredHeight: 25
            }
            ActionButton {
                text: "Stop"
                visible: backend.busy
                onClicked: backend.cancel()
            }
            Label {
                text: backend.busy ? window.elapsedText + " · " + window.operationPhase : backend.status
                Layout.fillWidth: true
                ToolTip.visible: statusHover.hovered
                ToolTip.text: backend.status
                HoverHandler { id: statusHover }
                TapHandler { onTapped: activityDialog.open() }
                Accessible.description: "Open Activity to read and copy the complete status and error details."
                elide: Text.ElideRight
                color: "#adc0cc"
                Accessible.role: Accessible.StaticText
            }
            ActionButton {
                text: "Guide notes"
                enabled: backend.ready && !backend.busy
                onClicked: guideNotesDialog.open()
            }
            ActionButton {
                text: "Disk actions…"
                enabled: backend.ready && !backend.busy && !!window.document.disk
                onClicked: diskDialog.open()
            }
            ActionButton {
                text: "Activity"
                onClicked: activityDialog.open()
            }
            Label {
                id: diskStateLabel
                visible: backend.ready && !!window.document.disk
                text: window.document.disk ? window.document.disk.pending ? "Disk recovery pending" : window.document.disk.restoring !== null ? "Restoring disk checkpoint" : !window.document.disk.rootsMatch ? "Disk roots changed" : "Disk: " + window.document.disk.cursor + " actions applied" : ""
                color: window.document.disk && (window.document.disk.pending || window.document.disk.restoring !== null || !window.document.disk.rootsMatch) ? "#e7b274" : "#8da5b5"
                Accessible.name: text
                ToolTip.visible: diskStateHover.hovered
                ToolTip.text: window.document.disk ? window.document.disk.orderError || (window.document.disk.nextAction ? "Next disk action: " + window.document.disk.nextAction.mod + " / " + (window.document.disk.nextAction.instruction + 1) : "No remaining recorded disk actions") : ""
                HoverHandler {
                    id: diskStateHover
                }
            }
            Label {
                text: window.document.revisionCount ? "Revision " + (window.document.cursor + 1) + " / " + window.document.revisionCount : ""
                color: "#8da5b5"
            }
        }
    }

    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal
        handle: Rectangle {
            implicitWidth: 1
            color: "#344551"
        }
        Rectangle {
            SplitView.preferredWidth: 255
            SplitView.minimumWidth: 210
            color: "#14222c"
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 18
                spacing: 12
                Label {
                    text: "GUIDE ORDER"
                    font.pixelSize: 11
                    font.letterSpacing: 1.6
                    color: "#8ea6b6"
                }
                ListView {
                    id: guideList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 6
                    model: window.document.components || []
                    ScrollBar.vertical: ScrollBar {}
                    delegate: ItemDelegate {
                        id: modItem
                        required property var modelData
                        required property int index
                        width: ListView.view.width
                        height: 57
                        highlighted: window.componentIndex === index
                        text: modelData.name
                        enabled: !backend.busy
                        contentItem: RowLayout {
                            CheckBox {
                                checked: modItem.modelData.selected
                                enabled: backend.ready && !backend.busy
                                Accessible.name: "Include " + modItem.text
                                onClicked: backend.setIncluded(modItem.modelData.guid, checked)
                            }
                            Label {
                                text: modItem.text
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                                font.pixelSize: 13
                                color: modItem.highlighted ? "#c3ece1" : "#a8bdca"
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                        background: Rectangle {
                            color: modItem.highlighted ? "#21433e" : modItem.hovered ? "#213440" : "transparent"
                            radius: 7
                            border.width: modItem.activeFocus ? 2 : 0
                            border.color: "#70c5b4"
                        }
                        onClicked: {
                            modItem.forceActiveFocus(Qt.MouseFocusReason);
                            window.selectMod(index);
                        }
                        Keys.onPressed: event => {
                            let target = null;
                            if (event.key === Qt.Key_Up) target = index - 1;
                            else if (event.key === Qt.Key_Down) target = index + 1;
                            else if (event.key === Qt.Key_Home) target = 0;
                            else if (event.key === Qt.Key_End) target = guideList.count - 1;
                            else if (event.key === Qt.Key_PageUp) target = index - Math.max(1, Math.floor(guideList.height / height));
                            else if (event.key === Qt.Key_PageDown) target = index + Math.max(1, Math.floor(guideList.height / height));
                            if (target !== null) {
                                target = Math.max(0, Math.min(guideList.count - 1, target));
                                guideList.currentIndex = target;
                                guideList.positionViewAtIndex(target, ListView.Contain);
                                guideList.forceLayout();
                                const item = guideList.itemAtIndex(target);
                                if (item) item.forceActiveFocus(Qt.OtherFocusReason);
                                window.selectMod(target);
                                event.accepted = true;
                            }
                        }
                    }
                }
                Label {
                    text: "LOCATIONS"
                    color: "#8ea6b6"
                    font.pixelSize: 11
                    font.letterSpacing: 1.6
                }
                ActionButton {
                    text: "Mod sources"
                    Layout.fillWidth: true
                    enabled: backend.ready && !backend.busy
                    onClicked: window.navigate(window.document.mods)
                }
                ActionButton {
                    text: "Game directory"
                    Layout.fillWidth: true
                    enabled: backend.ready && !backend.busy
                    onClicked: window.navigate(window.document.game)
                }
                Label {
                    text: "FOLDERS"
                    color: "#8ea6b6"
                    font.pixelSize: 11
                    font.letterSpacing: 1.6
                }
                TreeView {
                    id: folderTree
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    enabled: backend.ready && !backend.busy
                    columnWidthProvider: column => column === 0 ? width : 0
                    ScrollBar.vertical: ScrollBar {}
                    model: TreeModel {
                        id: folderModel
                        TableModelColumn {
                            display: "name"
                        }
                        TableModelColumn {
                            display: "path"
                        }
                        rows: []
                    }
                    delegate: TreeViewDelegate {
                        id: folderItem
                        implicitHeight: 36
                        highlighted: window.cleanPath(folderModel.data(folderTree.modelIndex(row, 1), "display")) === window.folder
                        background: Rectangle {
                            radius: 6
                            color: folderItem.highlighted ? "#21433e" : folderItem.hovered ? "#213440" : "transparent"
                        }
                        onClicked: window.navigate(folderModel.data(folderTree.modelIndex(row, 1), "display"))
                    }
                }
            }
        }
        Rectangle {
            id: browserPane
            SplitView.fillWidth: true
            SplitView.minimumWidth: 360
            color: "#101820"
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 18
                GridLayout {
                    id: browserHeading
                    Layout.fillWidth: true
                    columns: browserPane.width - 48 >= 640 ? 2 : 1
                    columnSpacing: 16
                    rowSpacing: 10
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        Layout.preferredWidth: Math.max(0, browserPane.width - 48 - (browserHeading.columns === 1 ? 0 : browserControls.implicitWidth + browserHeading.columnSpacing))
                        Layout.maximumWidth: Layout.preferredWidth
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: window.currentMod ? window.currentMod.name : "Your build, step by step"
                            font.pixelSize: 24
                            font.bold: true
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: "Browse the predicted files at any action in your build."
                            color: "#91a8b8"
                            font.pixelSize: 13
                            wrapMode: Text.Wrap
                        }
                    }
                    RowLayout {
                        id: browserControls
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignRight
                        Label {
                            text: "After action"
                            color: "#aec0cc"
                        }
                        SpinBox {
                            id: actionPoint
                            from: 0
                            to: window.currentMod ? window.currentMod.actions : 0
                            enabled: backend.ready && !backend.busy
                            editable: true
                            onValueModified: {
                                window.document.entries = [];
                                window.clearSelection();
                                window.updateRows();
                            }
                        }
                        ActionButton {
                            text: "Preview"
                            enabled: backend.ready && !backend.busy
                            onClicked: backend.predict(window.componentIndex, actionPoint.value)
                        }
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: "#17242e"
                    radius: 10
                    border.color: "#30424f"
                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 16
                        spacing: 12
                        RowLayout {
                            Layout.fillWidth: true
                            ActionButton {
                                text: "Back"
                                enabled: window.history.length > 0
                                onClicked: window.goBack()
                            }
                            ActionButton {
                                text: "Up"
                                enabled: !!window.folder && !window.isRoot(window.folder)
                                onClicked: window.navigate(window.parentPath(window.folder))
                            }
                            ActionButton {
                                text: "New folder…"
                                enabled: backend.ready && !backend.busy && !!window.currentMod && !!window.folder
                                onClicked: window.openRecord("CreateDirectory")
                                Accessible.description: "Record a new folder in this build. Preview does not change real files."
                            }
                            Label {
                                text: window.folder || "Open a session to browse files"
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                elide: Text.ElideMiddle
                                color: "#bdced9"
                                ToolTip.visible: hovered
                                property bool hovered: folderHover.hovered
                                ToolTip.text: text
                                HoverHandler {
                                    id: folderHover
                                }
                            }
                            ComboBox {
                                Layout.preferredWidth: 160
                                model: ["Explorer", "Dolphin", "Finder"]
                                onActivated: window.presentation = currentIndex
                                Accessible.name: "File browser presentation"
                            }
                        }
                        Flickable {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 36
                            contentWidth: crumbRow.width
                            contentHeight: height
                            clip: true
                            Row {
                                id: crumbRow
                                spacing: 4
                                Repeater {
                                    model: window.breadcrumbs()
                                    delegate: ActionButton {
                                        required property var modelData
                                        text: modelData.name
                                        onClicked: window.navigate(modelData.path)
                                    }
                                }
                            }
                        }
                        TextField {
                            id: search
                            placeholderText: "Search this folder"
                            Layout.fillWidth: true
                            onTextChanged: window.updateRows()
                            Accessible.name: "Search this folder"
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            ActionButton {
                                text: "Name" + (window.sortColumn === 0 ? (window.sortAscending ? " ↑" : " ↓") : "")
                                Layout.fillWidth: true
                                onClicked: window.toggleSort(0)
                            }
                            ActionButton {
                                text: "Type"
                                Layout.preferredWidth: 100
                                onClicked: window.toggleSort(1)
                            }
                            ActionButton {
                                text: "Size"
                                Layout.preferredWidth: 95
                                onClicked: window.toggleSort(2)
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            visible: backend.ready && !!window.currentMod
                            enabled: backend.ready && !backend.busy && !!window.currentMod && window.selectedPaths.length > 0
                            ActionButton {
                                text: "Copy…"
                                onClicked: window.openRecord("Copy")
                                Accessible.name: "Record copy of selected files"
                            }
                            ActionButton {
                                text: "Move…"
                                onClicked: window.openRecord("Move")
                                Accessible.name: "Record move of selected files"
                            }
                            ActionButton {
                                text: "Extract…"
                                enabled: window.selectedPaths.every(path => window.isArchive(path))
                                onClicked: window.openRecord("Extract")
                                Accessible.name: "Record extraction of selected archives"
                            }
                            ActionButton {
                                text: "Inspect patcher…"
                                enabled: window.selectedPaths.length === 1
                                onClicked: window.inspectSelectedPatcher()
                            }
                            Item { Layout.fillWidth: true }
                        }
                        StackLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            currentIndex: window.presentation
                            ListView {
                                clip: true
                                spacing: 3
                                model: window.rows
                                ScrollBar.vertical: ScrollBar {}
                                delegate: BrowserItem {
                                    id: entry
                                    itemView: ListView.view
                                    width: ListView.view.width
                                    height: 43
                                    highlighted: window.selectedPaths.includes(modelData.path)
                                    Accessible.name: window.nameOf(modelData.path)
                                    contentItem: RowLayout {
                                        spacing: 12
                                        FileIcon {
                                            directory: entry.modelData.directory
                                        }
                                        Label {
                                            text: window.nameOf(entry.modelData.path)
                                            Layout.fillWidth: true
                                            elide: Text.ElideRight
                                        }
                                        Label {
                                            text: entry.modelData.directory ? "Folder" : entry.modelData.content_known ? "File" : "Unobserved"
                                            Layout.preferredWidth: 100
                                            color: "#90a9b9"
                                        }
                                        Label {
                                            text: entry.modelData.directory ? "—" : entry.modelData.bytes !== null ? window.formatBytes(entry.modelData.bytes) : "Unknown"
                                            Layout.preferredWidth: 95
                                            color: "#90a9b9"
                                            horizontalAlignment: Text.AlignRight
                                        }
                                    }
                                    background: Rectangle {
                                        radius: 6
                                        color: entry.highlighted ? "#24473f" : entry.hovered ? "#243644" : "transparent"
                                    }
                                }
                                Label {
                                    anchors.centerIn: parent
                                    visible: window.rows.length === 0
                                    text: backend.busy ? "Preparing your preview…" : "No predicted files in this folder"
                                    color: "#8da5b6"
                                }
                            }
                            GridView {
                                clip: true
                                model: window.rows
                                cellWidth: 150
                                cellHeight: 130
                                ScrollBar.vertical: ScrollBar {}
                                delegate: BrowserItem {
                                    id: tile
                                    itemView: GridView.view
                                    itemsPerRow: Math.max(1, Math.floor(GridView.view.width / GridView.view.cellWidth))
                                    width: 142
                                    height: 122
                                    highlighted: window.selectedPaths.includes(modelData.path)
                                    Accessible.name: window.nameOf(modelData.path)
                                    contentItem: ColumnLayout {
                                        FileIcon {
                                            directory: tile.modelData.directory
                                            Layout.alignment: Qt.AlignHCenter
                                            scale: 1.5
                                            Layout.topMargin: 10
                                            Layout.bottomMargin: 8
                                        }
                                        Label {
                                            text: window.nameOf(tile.modelData.path)
                                            Layout.fillWidth: true
                                            horizontalAlignment: Text.AlignHCenter
                                            wrapMode: Text.Wrap
                                            maximumLineCount: 2
                                            elide: Text.ElideRight
                                        }
                                        Label {
                                            text: tile.modelData.directory ? "Folder" : window.formatBytes(tile.modelData.bytes)
                                            Layout.alignment: Qt.AlignHCenter
                                            color: "#90a9b9"
                                            font.pixelSize: 11
                                        }
                                    }
                                    background: Rectangle {
                                        radius: 8
                                        color: tile.highlighted ? "#24473f" : tile.hovered ? "#243644" : "transparent"
                                    }
                                }
                            }
                            Flickable {
                                id: columns
                                clip: true
                                contentWidth: columnRow.width
                                contentHeight: height
                                ScrollBar.horizontal: ScrollBar {}
                                onContentWidthChanged: contentX = Math.max(0, contentWidth - width)
                                Row {
                                    id: columnRow
                                    height: columns.height
                                    spacing: 1
                                    Repeater {
                                        id: finderColumns
                                        model: window.breadcrumbs()
                                        delegate: Rectangle {
                                            id: column
                                            property alias browserView: columnList
                                            required property var modelData
                                            width: 230
                                            height: columnRow.height
                                            color: "#14222c"
                                            ColumnLayout {
                                                anchors.fill: parent
                                                anchors.margins: 8
                                                Label {
                                                    text: column.modelData.name
                                                    font.bold: true
                                                    elide: Text.ElideRight
                                                    Layout.fillWidth: true
                                                }
                                                ListView {
                                                    id: columnList
                                                    Layout.fillWidth: true
                                                    Layout.fillHeight: true
                                                    clip: true
                                                    model: window.childrenOf(column.modelData.path)
                                                    ScrollBar.vertical: ScrollBar {}
                                                    delegate: BrowserItem {
                                                        id: columnEntry
                                                        itemView: ListView.view
                                                        width: ListView.view.width
                                                        height: 42
                                                        columnMode: true
                                                        siblings: window.childrenOf(column.modelData.path)
                                                        highlighted: window.selectedPaths.includes(modelData.path) || window.isUnder(window.folder, modelData.path)
                                                        Accessible.name: window.nameOf(modelData.path)
                                                        contentItem: RowLayout {
                                                            FileIcon {
                                                                directory: columnEntry.modelData.directory
                                                            }
                                                            Label {
                                                                text: window.nameOf(columnEntry.modelData.path)
                                                                Layout.fillWidth: true
                                                                elide: Text.ElideRight
                                                            }
                                                            Label {
                                                                text: columnEntry.modelData.directory ? "›" : ""
                                                                color: "#90a9b9"
                                                            }
                                                        }
                                                        background: Rectangle {
                                                            radius: 5
                                                            color: columnEntry.highlighted ? "#24473f" : columnEntry.hovered ? "#243644" : "transparent"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Label {
                            text: window.rows.length + " items · " + (window.selectedPaths.length + " selected") + " · predicted state"
                            color: "#8fa8b8"
                            font.pixelSize: 12
                        }
                    }
                }
            }
        }
        Rectangle {
            SplitView.preferredWidth: 290
            SplitView.minimumWidth: 240
            color: "#14222c"
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 20
                spacing: 16
                Label {
                    text: "MOD DIRECTIONS"
                    color: "#8ea6b6"
                    font.pixelSize: 11
                    font.letterSpacing: 1.6
                }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    TextArea {
                        text: window.currentMod ? [window.currentMod.method ? "Installation: " + window.currentMod.method : "", window.currentMod.description || "", window.currentMod.directions || ""].concat((window.currentMod.notes || []).map(note => note.field.replace(/([a-z])([A-Z])/g, "$1 $2") + ":\n" + note.text)).filter(s => s.length).join("\n\n") : "Open a saved session. Your recorded guide, inputs and revisions remain in the Rust authoring engine."
                        readOnly: true
                        wrapMode: TextEdit.Wrap
                        background: null
                        color: "#b7cad5"
                        font.pixelSize: 13
                        selectByMouse: true
                    }
                }
                Label {
                    text: "RECORDED ACTIONS"
                    color: "#8ea6b6"
                    font.pixelSize: 11
                    font.letterSpacing: 1.6
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.min(180, Math.max(48, count * 52))
                    clip: true
                    spacing: 4
                    model: window.currentMod ? window.currentMod.recipe : []
                    ScrollBar.vertical: ScrollBar {}
                    delegate: ItemDelegate {
                        id: recipeItem
                        required property var modelData
                        required property int index
                        width: ListView.view.width
                        height: 48
                        enabled: !backend.busy
                        highlighted: actionPoint.value === index + 1
                        Accessible.name: "Action " + (index + 1) + ": " + modelData.Action
                        contentItem: ColumnLayout {
                            spacing: 2
                            Label {
                                text: (recipeItem.index + 1) + " · " + recipeItem.modelData.Action
                                font.bold: true
                            }
                            Label {
                                text: (recipeItem.modelData.Source || []).map(p => window.nameOf(p)).join(", ")
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                color: "#91a7b5"
                                font.pixelSize: 11
                            }
                        }
                        background: Rectangle {
                            radius: 6
                            color: recipeItem.highlighted ? "#24473f" : recipeItem.hovered ? "#243644" : "transparent"
                        }
                        onClicked: {
                            actionPoint.value = index + 1;
                            window.document.entries = [];
                            window.clearSelection();
                            window.updateRows();
                            actionDetails.instruction = modelData;
                            actionDetails.position = index;
                            actionDetails.componentGuid = window.currentMod.guid;
                            actionDetails.revisionId = window.document.revisionId;
                            actionDetails.open();
                        }
                    }
                    Label {
                        anchors.centerIn: parent
                        visible: parent.count === 0
                        text: "No actions recorded yet"
                        color: "#91a7b5"
                    }
                }
                Label {
                    text: "Preview keeps the game unchanged."
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                    color: "#88c5b4"
                    font.pixelSize: 12
                }
                ActionButton {
                    text: "Record an action"
                    Layout.fillWidth: true
                    enabled: backend.ready && !backend.busy && !!window.currentMod
                    onClicked: window.openRecord("Copy")
                }
                ActionButton {
                    text: "Installer runner settings…"
                    Layout.fillWidth: true
                    enabled: backend.ready && !backend.busy
                    onClicked: runnerSettings.open()
                }
                ActionButton {
                    text: "Inspect patcher"
                    Layout.fillWidth: true
                    enabled: backend.ready && !backend.busy && !!window.currentMod
                    onClicked: {
                        patcherDialog.component = window.componentIndex;
                        patcherDialog.componentGuid = window.currentMod.guid;
                        patcherDialog.point = actionPoint.value;
                        patcherDialog.revisionId = window.document.revisionId;
                        patcherSource.text = window.selectedPaths.length === 1 ? window.selectedPath : "";
                        window.document = Object.assign({}, window.document, {
                            patcher: null
                        });
                        patcherDialog.open();
                    }
                }
            }
        }
    }

    Dialog {
        id: patcherDialog
        property int component: 0
        property int point: 0
        property string componentGuid: ""
        property string revisionId: ""
        readonly property var preview: window.document.patcher || null
        title: "Built-in patcher · after action " + point
        anchors.centerIn: parent
        width: 760
        height: 620
        modal: true
        standardButtons: Dialog.Close
        function inspect(namespace) {
            backend.inspectPatcher(component, point, patcherSource.text, namespace, revisionId);
        }
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            Label {
                text: "OdyPatcher reads the predicted installer data at this build point."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            TextField {
                id: patcherSource
                Layout.fillWidth: true
                placeholderText: "Patcher file or folder"
                Accessible.name: "Patcher source"
                enabled: !backend.busy
                onTextEdited: window.document = Object.assign({}, window.document, {
                    patcher: null
                })
            }
            RowLayout {
                ActionButton {
                    text: "Read installer"
                    enabled: !backend.busy && patcherSource.text.trim().length > 0
                    onClicked: patcherDialog.inspect(-1)
                }
                ComboBox {
                    model: patcherDialog.preview ? patcherDialog.preview.options : []
                    textRole: "name"
                    currentIndex: patcherDialog.preview && patcherDialog.preview.selected !== null ? patcherDialog.preview.selected : -1
                    visible: count > 0
                    enabled: !backend.busy
                    Layout.fillWidth: true
                    Accessible.name: "Installer option"
                    onActivated: patcherDialog.inspect(model[currentIndex].index)
                }
            }
            Label {
                text: patcherDialog.preview ? patcherDialog.preview.backend + " " + patcherDialog.preview.version + " · " + patcherDialog.preview.instructions : backend.status
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#91a7b5"
            }
            Label {
                visible: !!(patcherDialog.preview && patcherDialog.preview.blockedReason)
                text: patcherDialog.preview ? patcherDialog.preview.blockedReason || "" : ""
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                color: "#edc78b"
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                TextArea {
                    text: patcherDialog.preview ? [patcherDialog.preview.description, patcherDialog.preview.summary].filter(s => s.length).join("\n\n") : "Read an installer to see its description and planned changes."
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    Accessible.name: "Installer description and planned changes"
                }
            }
            ActionButton {
                text: "Record this patcher action"
                enabled: !backend.busy && !!patcherDialog.preview && typeof patcherDialog.preview.arguments === "string" && !patcherDialog.preview.blockedReason
                onClicked: {
                    const preview = patcherDialog.preview;
                    window.requestedAction = "Patcher";
                    recordDialog.editDefinition = null;
                    recordDialog.componentGuid = patcherDialog.componentGuid;
                    recordDialog.position = patcherDialog.point;
                    recordDialog.revisionId = patcherDialog.revisionId;
                    recordDialog.sourcePreset = preview.source;
                    recordDialog.argumentsPreset = preview.arguments;
                    patcherDialog.close();
                    recordDialog.open();
                }
            }
        }
    }
    Dialog {
        id: actionDetails
        property var instruction: ({})
        property int position: -1
        property string componentGuid: ""
        property string revisionId: ""
        title: "Action " + (position + 1) + " · " + (instruction.Action || "")
        anchors.centerIn: parent
        width: 650
        height: 440
        modal: true
        standardButtons: Dialog.Close
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            Label {
                text: "Saved sources and destination"
                font.bold: true
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                TextArea {
                    text: (actionDetails.instruction.Source || []).join("\n") + "\n\nDestination\n" + (actionDetails.instruction.Destination || "—") + "\n\nOptions\n" + (actionDetails.instruction.Arguments || "—") + "\n\nReplace existing files: " + (actionDetails.instruction.Overwrite ? "Yes" : "No")
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.Wrap
                    color: "#bdced9"
                    Accessible.name: "Saved instruction sources, destination and options"
                }
            }
            RowLayout {
                ActionButton {
                    text: "↑ Move up"
                    enabled: !backend.busy && actionDetails.position > 0
                    Accessible.name: "Move selected action up"
                    Accessible.description: "Move this saved action one position earlier. Undo restores its previous order."
                    onClicked: {
                        const destination = actionDetails.position - 1;
                        backend.moveAction(actionDetails.componentGuid, actionDetails.position, destination, actionDetails.instruction.Guid, actionDetails.revisionId);
                        actionDetails.close();
                    }
                }
                ActionButton {
                    text: "↓ Move down"
                    enabled: !backend.busy && window.currentMod && actionDetails.position >= 0 && actionDetails.position < window.currentMod.recipe.length - 1
                    Accessible.name: "Move selected action down"
                    Accessible.description: "Move this saved action one position later. Undo restores its previous order."
                    onClicked: {
                        const destination = actionDetails.position + 1;
                        backend.moveAction(actionDetails.componentGuid, actionDetails.position, destination, actionDetails.instruction.Guid, actionDetails.revisionId);
                        actionDetails.close();
                    }
                }
                ActionButton {
                    text: "Edit action"
                    enabled: !backend.busy && ["Copy", "Move", "Rename", "Delete", "Extract", "Patcher", "Execute", "CreateDirectory", "DelDuplicate", "CleanList", "Choose"].includes(actionDetails.instruction.Action)
                    onClicked: {
                        recordDialog.editDefinition = actionDetails.instruction;
                        recordDialog.componentGuid = actionDetails.componentGuid;
                        recordDialog.position = actionDetails.position;
                        recordDialog.revisionId = actionDetails.revisionId;
                        window.requestedAction = actionDetails.instruction.Action;
                        actionDetails.close();
                        recordDialog.open();
                    }
                }
                ActionButton {
                    text: "Remove action…"
                    enabled: !backend.busy
                    onClicked: {
                        actionDetails.close();
                        removeDialog.open();
                    }
                }
            }
        }
    }
    Dialog {
        id: removeDialog
        title: "Remove action from recipe?"
        anchors.centerIn: parent
        width: 440
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: backend.removeAction(actionDetails.componentGuid, actionDetails.position, actionDetails.instruction.Guid, actionDetails.revisionId)
        Label {
            anchors.fill: parent
            wrapMode: Text.Wrap
            text: "Remove action " + (actionDetails.position + 1) + " (" + actionDetails.instruction.Action + "). Undo restores the saved instruction."
        }
    }
    Dialog {
        id: guideNotesDialog
        title: "Guide-wide installation notes"
        anchors.centerIn: parent
        width: Math.min(820, window.width - 48)
        height: Math.min(650, window.height - 96)
        modal: true
        standardButtons: Dialog.Close
        onOpened: guideNotesFlickable.contentY = 0
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            ComboBox {
                id: guideNotesSection
                Layout.fillWidth: true
                model: ["Installation and game versions", "Widescreen requirements", "After the build"]
                Accessible.name: "Guide notes section"
                onActivated: {
                    guideNotesLoader.active = false;
                    Qt.callLater(function() {
                        guideNotesLoader.active = true;
                        guideNotesFlickable.contentY = 0;
                    });
                }
            }
            ScrollView {
                id: guideNotesScroll
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                Flickable {
                    id: guideNotesFlickable
                    contentWidth: width
                    contentHeight: guideNotesLoader.height
                    clip: true
                    Loader {
                        id: guideNotesLoader
                        width: guideNotesFlickable.width
                        height: item ? item.height : 0
                        sourceComponent: Component {
                    TextEdit {
                        id: guideNotesText
                        width: guideNotesFlickable.width
                        height: contentHeight
                        font: window.font
                        readonly property var notes: window.document.guideNotes || ({})
                        text: [notes.introduction, notes.widescreen, notes.afterBuild][guideNotesSection.currentIndex] || "No notes were saved for this section."
                        textFormat: TextEdit.MarkdownText
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        color: "#bdced9"
                        Accessible.name: "Saved guide-wide installation and version requirements"
                        onTextChanged: Qt.callLater(function() {
                            guideNotesText.cursorPosition = 0;
                            guideNotesFlickable.contentY = 0;
                        })
                    }
                        }
                    }
                }
            }
        }
    }
    Dialog {
        id: activityDialog
        title: "Authoring activity"
        anchors.centerIn: parent
        width: 700
        height: 430
        modal: true
        standardButtons: Dialog.Close
        ScrollView {
            anchors.fill: parent
            clip: true
            TextArea {
                text: backend.status + (backend.busy ? "\n\nElapsed " + window.elapsedText + " · " + window.operationPhase + "\nStop requests cancellation; checkpoint cleanup may take time." : "") + "\n\n" + window.activityHistory.join("\n") + "\n\n" + (window.document.events || []).map(e => e.component + " · action " + e.step + "\n" + e.message).join("\n\n")
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                Accessible.name: "Authoring activity details"
            }
        }
    }

    Dialog {
        id: recordDialog
        property var editDefinition: null
        property string sourcePreset: ""
        property string argumentsPreset: ""
        property string componentGuid: ""
        property int position: 0
        property string revisionId: ""
        title: actionKind.currentText === "CreateDirectory" ? (editDefinition ? "Edit folder action" : "Record new folder") : (editDefinition ? "Edit " : "Record ") + (actionKind.currentText || window.requestedAction).toLowerCase() + " action"
        anchors.centerIn: parent
        width: 620
        modal: true
        standardButtons: Dialog.Save | Dialog.Cancel
        onOpened: {
            actionKind.currentIndex = actionKind.model.indexOf(window.requestedAction);
            sourcePaths.text = window.requestedAction === "CreateDirectory" ? "" : editDefinition ? (editDefinition.Source || []).join("\n") : (sourcePreset || window.selectedPaths.join("\n"));
            destinationPath.text = editDefinition ? editDefinition.Destination : window.requestedAction === "CreateDirectory" ? window.folder + "/New folder" : window.requestedAction === "Rename" ? window.nameOf(window.selectedPath) : window.requestedAction === "Delete" ? "" : window.requestedAction === "Extract" ? window.document.mods || "" : window.document.game || "";
            actionArguments.text = editDefinition ? editDefinition.Arguments : argumentsPreset;
            overwriteFiles.checked = editDefinition ? editDefinition.Overwrite : true;
        }
        onAccepted: {
            const definition = JSON.stringify({
                Action: actionKind.currentText,
                Source: actionKind.currentText === "CreateDirectory" ? [] : sourcePaths.text.split("\n").map(s => s.trim()).filter(s => s.length > 0),
                Destination: destinationPath.text.trim(),
                Arguments: actionKind.currentText === "CreateDirectory" ? "" : actionArguments.text,
                Overwrite: overwriteFiles.checked,
                Dependencies: editDefinition ? editDefinition.Dependencies : [],
                Restrictions: editDefinition ? editDefinition.Restrictions : []
            });
            if (editDefinition)
                backend.replaceAction(componentGuid, position, definition, editDefinition.Guid, revisionId);
            else
                backend.recordAction(componentGuid, position, definition, revisionId, JSON.stringify(window.immediateActions ? [window.document.mods, window.document.game] : null));
        }
        contentItem: ScrollView {
            id: recordScroll
            clip: true
            implicitHeight: Math.min(recordForm.implicitHeight, window.height - 150)
            contentWidth: availableWidth
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                id: recordForm
                width: recordScroll.availableWidth
                spacing: 12
            Label {
                text: recordDialog.editDefinition ? "Update action " + (recordDialog.position + 1) + ". Undo restores its previous definition." : "Save after action " + recordDialog.position + (window.immediateActions ? ". Save applies this action to real files." : ". Preview the result before applying it.")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            ComboBox {
                id: actionKind
                model: ["Copy", "Move", "Rename", "Delete", "Extract", "Patcher", "Execute", "CreateDirectory", "DelDuplicate", "CleanList", "Choose"]
                onActivated: {
                    if (currentText === "CreateDirectory") {
                        sourcePaths.text = "";
                        actionArguments.text = "";
                        destinationPath.text = window.folder + "/New folder";
                        window.immediateActions = false;
                    }
                    if (currentText === "Execute" && !(window.document.allowProcesses && window.document.providedRunner))
                        window.immediateActions = false;
                }
                Accessible.name: "Action type"
                Layout.fillWidth: true
            }
            Label {
                visible: actionKind.currentText !== "CreateDirectory"
                text: "Source files or folders · " + sourcePaths.text.split("\n").filter(path => path.trim().length > 0).length + " paths · one per line"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            ScrollView {
                id: sourceScroll
                visible: actionKind.currentText !== "CreateDirectory"
                Layout.fillWidth: true
                Layout.preferredHeight: 90
                clip: true
                contentWidth: availableWidth
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                TextArea {
                    id: sourcePaths
                    width: sourceScroll.availableWidth
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                    Accessible.name: "Source files or folders"
                    Accessible.description: "One path per line. " + text.split("\n").filter(path => path.trim().length > 0).length + " paths. Scroll to review all sources."
                }
            }
            Label {
                text: actionKind.currentText === "CreateDirectory" ? "New folder path · parent folder must already exist" : actionKind.currentText === "Rename" ? "New filename or destination path" : "Destination"
            }
            TextField {
                id: destinationPath
                Layout.fillWidth: true
                Accessible.name: "Action destination"
            }
            RowLayout {
                ActionButton {
                    text: "Game root"
                    onClicked: destinationPath.text = window.document.game
                }
                ActionButton {
                    text: "Mod sources"
                    onClicked: destinationPath.text = window.document.mods
                }
                ActionButton {
                    text: "Browse folders…"
                    enabled: ["Copy", "Move", "Extract", "Patcher"].includes(actionKind.currentText)
                    onClicked: {
                        destinationBrowser.folder = window.isUnder(destinationPath.text, window.document.mods) || window.isUnder(destinationPath.text, window.document.game) ? destinationPath.text : window.document.mods;
                        destinationBrowser.open();
                    }
                }
            }
            Label {
                visible: actionKind.currentText !== "CreateDirectory"
                text: actionKind.currentText === "Execute" ? "Executable arguments" : "Options"
            }
            Label {
                visible: actionKind.currentText === "Execute"
                text: "An executable's future file changes remain unknown until it runs. Real execution requires explicit trust and a configured headless runner."
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#edc78b"
            }
            TextField {
                id: actionArguments
                visible: actionKind.currentText !== "CreateDirectory"
                Layout.fillWidth: true
                Accessible.name: "Action options"
            }
            CheckBox {
                visible: !recordDialog.editDefinition
                text: "Apply new actions immediately with disk checkpoints"
                enabled: actionKind.currentText !== "Execute" || (!!window.document.allowProcesses && !!window.document.providedRunner)
                checked: window.immediateActions
                onClicked: window.immediateActions = checked
                Accessible.description: "Changes real files when you save. Only the next ordered action can run."
            }
            Label {
                visible: !recordDialog.editDefinition && window.immediateActions
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Saving will change real files in these folders:\n" + window.document.mods + "\n" + window.document.game + "\nRecipe Undo does not restore files; use Disk actions for checkpoints."
            }
            CheckBox {
                id: overwriteFiles
                visible: actionKind.currentText !== "CreateDirectory"
                text: "Replace existing files"
                checked: true
            }
            }
        }
    }

    Dialog {
        id: runnerSettings
        property string revisionId: ""
        title: "Headless installer runner · this session only"
        anchors.centerIn: parent
        width: 700
        height: Math.min(660, window.height - 80)
        modal: true
        standardButtons: Dialog.Save | Dialog.Cancel
        onOpened: {
            revisionId = window.document.revisionId;
            const runner = window.document.providedRunner || {};
            const wine = runner.windows || {};
            runnerRequests.text = runner.requestDir || "";
            runnerXvfb.text = runner.xvfb || "";
            runnerWine.text = wine.executable || "";
            runnerWineserver.text = wine.wineserver || "";
            runnerPrefix.text = wine.prefix || "";
            runnerTrust.checked = !!window.document.allowProcesses;
        }
        onAccepted: backend.configureProvidedRunner(JSON.stringify({
            trusted: runnerTrust.checked, requestDir: runnerRequests.text.trim(), xvfb: runnerXvfb.text.trim(),
            wine: runnerWine.text.trim(), wineserver: runnerWineserver.text.trim(), prefix: runnerPrefix.text.trim()
        }), revisionId)
        ScrollView {
            id: runnerSettingsScroll
            anchors.fill: parent
            clip: true
            contentWidth: availableWidth
            ColumnLayout {
                width: runnerSettingsScroll.availableWidth
                spacing: 10
                Label {
                    text: "Provided installers open in their own private display on Linux. Saving these settings does not launch anything. They reset when the session is reopened."
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                }
                Label { text: "Private installer request directory (outside game and source folders)"; wrapMode: Text.Wrap; Layout.fillWidth: true }
                TextField { id: runnerRequests; Accessible.name: "Private installer request directory"; Layout.fillWidth: true }
                Label { text: "Xvfb executable" }
                TextField { id: runnerXvfb; Accessible.name: "Xvfb executable"; Layout.fillWidth: true }
                Label { text: "Wine executable · optional for native installers" }
                TextField { id: runnerWine; Accessible.name: "Wine executable"; Layout.fillWidth: true }
                Label { text: "Matching wineserver executable" }
                TextField { id: runnerWineserver; Accessible.name: "Wineserver executable"; Layout.fillWidth: true }
                Label { text: "Dedicated Wine prefix inside the request directory"; wrapMode: Text.Wrap; Layout.fillWidth: true }
                TextField { id: runnerPrefix; Accessible.name: "Dedicated Wine prefix"; Layout.fillWidth: true }
                CheckBox {
                    id: runnerTrust
                    text: "I trust the supplied executables recorded in this session"
                    Layout.fillWidth: true
                }
                ActionButton {
                    text: "Disable executable actions"
                    onClicked: {
                        backend.configureProvidedRunner("null", runnerSettings.revisionId);
                        runnerSettings.close();
                    }
                }
            }
        }
    }
    Dialog {
        id: destinationBrowser
        property string folder: ""
        title: "Choose destination folder"
        anchors.centerIn: parent
        width: 560
        height: 440
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: destinationPath.text = folder
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            Label {
                text: destinationBrowser.folder
                Layout.fillWidth: true
                wrapMode: Text.WrapAnywhere
                Accessible.name: "Selected destination folder"
            }
            RowLayout {
                ActionButton {
                    text: "Up"
                    enabled: !window.isRoot(destinationBrowser.folder)
                    onClicked: destinationBrowser.folder = window.parentPath(destinationBrowser.folder)
                }
                ActionButton { text: "Mod sources"; onClicked: destinationBrowser.folder = window.document.mods }
                ActionButton { text: "Game"; onClicked: destinationBrowser.folder = window.document.game }
            }
            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: (window.document.entries || []).filter(entry => entry.directory && window.parentPath(entry.path) === window.cleanPath(destinationBrowser.folder) && window.cleanPath(entry.path) !== window.cleanPath(destinationBrowser.folder))
                ScrollBar.vertical: ScrollBar {}
                delegate: ItemDelegate {
                    required property var modelData
                    width: ListView.view.width
                    text: window.nameOf(modelData.path)
                    Accessible.name: "Open destination folder " + text
                    onClicked: destinationBrowser.folder = modelData.path
                }
                Label {
                    anchors.centerIn: parent
                    visible: parent.count === 0
                    text: "No subfolders at this preview point"
                    color: "#91a7b5"
                }
            }
            Label {
                text: "Choose this folder with OK. This only fills the destination; Save records the action."
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#91a7b5"
            }
        }
    }
    Dialog {
        id: diskDialog
        title: "Recorded disk actions"
        anchors.centerIn: parent
        width: 650
        modal: true
        standardButtons: Dialog.Close
        readonly property var disk: window.document.disk || ({})
        readonly property bool available: backend.ready && !backend.busy && disk.rootsMatch && !disk.pending && disk.restoring === null
        readonly property bool nextRunnerReady: !disk.nextAction || !disk.nextAction.requiresRunner || (!!window.document.allowProcesses && !!window.document.providedRunner)
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Virtual preview is the default. Applying an action changes your configured folders and saves before/after checkpoints. Recipe Undo and Redo do not restore installed files."
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: diskDialog.disk.pending ? "A disk action is pending. Recover its checkpoint before continuing." : diskDialog.disk.restoring !== null ? "A checkpoint restore was interrupted. Resume that restore before continuing." : !diskDialog.disk.rootsMatch ? "These checkpoints belong to different folders. Reopen the session with its original folders." : diskDialog.disk.orderError || (diskDialog.disk.nextAction ? "Next: " + diskDialog.disk.nextAction.mod + " / action " + (diskDialog.disk.nextAction.instruction + 1) : "No recorded actions remain.")
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: backend.status
                Accessible.role: Accessible.StaticText
            }
            Label {
                visible: !diskDialog.nextRunnerReady
                text: "Configure and trust the headless installer runner before applying this executable action."
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                color: "#edc78b"
            }
            ActionButton {
                text: "Apply next recorded action…"
                enabled: diskDialog.available && diskDialog.nextRunnerReady && !diskDialog.disk.orderError && !!diskDialog.disk.nextAction
                onClicked: window.confirmDisk("apply", 0)
            }
            RowLayout {
                ActionButton {
                    text: "Undo disk action…"
                    enabled: diskDialog.available && diskDialog.disk.cursor > 0
                    onClicked: window.confirmDisk("restore", diskDialog.disk.cursor - 1)
                }
                ActionButton {
                    text: "Redo disk action…"
                    enabled: diskDialog.available && diskDialog.disk.cursor < diskDialog.disk.checkpoints
                    onClicked: window.confirmDisk("restore", diskDialog.disk.cursor + 1)
                }
            }
            ActionButton {
                text: "Resume checkpoint restore…"
                visible: diskDialog.disk.restoring !== null
                enabled: backend.ready && !backend.busy && diskDialog.disk.rootsMatch && !diskDialog.disk.pending
                onClicked: window.confirmDisk("restore", diskDialog.disk.restoring)
            }
            ActionButton {
                text: "Recover pending action…"
                visible: !!diskDialog.disk.pending
                enabled: backend.ready && !backend.busy && diskDialog.disk.rootsMatch
                onClicked: window.confirmDisk("recover", 0)
            }
        }
    }
    Dialog {
        id: diskConfirmation
        property string kind: "apply"
        property int target: 0
        property int cursor: 0
        property string revision: ""
        property var action: null
        property string roots: ""
        property string mods: ""
        property string game: ""
        title: kind === "apply" ? "Apply recorded action to disk" : kind === "recover" ? "Recover pending disk action" : "Restore disk checkpoint"
        anchors.centerIn: parent
        width: 650
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            if (kind === "apply" && action)
                backend.applyDiskAction(action.component, action.instruction, action.guid, revision, mods, game);
            else if (kind === "recover")
                backend.recoverDisk(cursor, revision, mods, game);
            else if (kind === "restore")
                backend.seekDisk(target, cursor, revision, mods, game);
        }
        ColumnLayout {
            anchors.fill: parent
            spacing: 14
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: diskConfirmation.kind === "apply" && diskConfirmation.action ? "Apply " + diskConfirmation.action.mod + " / action " + (diskConfirmation.action.instruction + 1) + "?" : diskConfirmation.kind === "recover" ? "Recover the pending action through its recorded checkpoint?" : "Restore your folders to disk checkpoint " + diskConfirmation.target + "? Your saved recipe is retained."
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: diskConfirmation.roots
            }
        }
    }

    Dialog {
        id: setupGuideDialog
        property string revisionId: ""
        title: "Include missing guide mods"
        anchors.centerIn: parent
        width: 600
        modal: true
        standardButtons: Dialog.Save | Dialog.Cancel
        onOpened: revisionId = window.document.revisionId
        onAccepted: backend.addGuideSetup(setupGuidePath.text, revisionId)
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Read the original guide to include named setup and widescreen-tail mods in source order. Existing actions, selections and imported archives stay intact. Undo restores the prior recipe. Review game-version and widescreen requirements before selecting these mods. No disk actions run."
            }
            Label { text: "Original full guide file" }
            TextField {
                id: setupGuidePath
                Layout.fillWidth: true
                Accessible.name: "Original guide containing setup mods"
            }
        }
    }
    Dialog {
        id: openDialog
        title: newSession.checked ? "Start from a guide" : "Open authoring session"
        anchors.centerIn: parent
        width: 600
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: {
            window.resetGuidePosition = true;
            window.folder = "";
            window.history = [];
            window.componentIndex = 0;
            window.clearSelection();
            if (newSession.checked)
                backend.createSession(guidePath.text, sessionPath.text, modsPath.text, gamePath.text);
            else
                backend.openSession(sessionPath.text, modsPath.text, gamePath.text);
        }
        ColumnLayout {
            anchors.fill: parent
            spacing: 12
            CheckBox {
                id: newSession
                text: "Create a new session from a guide"
            }
            ActionButton {
                text: "Resolve known guide masters"
                visible: backend.ready
                enabled: !backend.busy
                onClicked: {
                    const revision = window.document.revisionId;
                    openDialog.close();
                    backend.reconcileGuideMasters(revision);
                }
            }
            ActionButton {
                text: "Include missing guide mods…"
                visible: backend.ready
                enabled: !backend.busy
                onClicked: {
                    openDialog.close();
                    setupGuideDialog.open();
                }
            }
            Label {
                visible: newSession.checked
                text: "Guide file · Markdown, JSON, YAML or TOML"
            }
            TextField {
                id: guidePath
                visible: newSession.checked
                Layout.fillWidth: true
                Accessible.name: "Guide file"
            }
            Label {
                text: newSession.checked ? "New session directory" : "Saved session directory"
            }
            TextField {
                id: sessionPath
                Layout.fillWidth: true
                Accessible.name: newSession.checked ? "New session directory" : "Saved session directory"
            }
            Label {
                text: "Mod source directory"
            }
            TextField {
                id: modsPath
                Layout.fillWidth: true
                Accessible.name: "Mod source directory"
            }
            Label {
                text: "Game directory"
            }
            TextField {
                id: gamePath
                Layout.fillWidth: true
                Accessible.name: "Game directory"
            }
            Label {
                text: newSession.checked ? "Use a new session directory outside game/mod folders; its parent and both roots must exist. Your guide is preserved. Preview is read-only." : "Directories must already exist. Preview is read-only."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                color: "#96aebe"
            }
        }
    }
}
