pragma Singleton
import QtQuick

QtObject {
    id: palette
    property string mode: "dark"
    readonly property var schemes: ({
        "dark": {
            window: "#0f1a22", panel: "#16252f", raised: "#1e3140", border: "#34505f",
            text: "#eef4f8", muted: "#a3b7c4", accent: "#35b3a4", accentText: "#04201c",
            danger: "#ff7b72", success: "#56d39b", warning: "#f0c14b", focus: "#8fd0ff"
        },
        "light": {
            window: "#f3f6f8", panel: "#ffffff", raised: "#e6edf1", border: "#b8c7d1",
            text: "#14212a", muted: "#4a5f6d", accent: "#0b7a6f", accentText: "#ffffff",
            danger: "#b3261e", success: "#1b7f4f", warning: "#8a5a00", focus: "#0b5cad"
        },
        "k1": {
            window: "#0b1226", panel: "#131c38", raised: "#1c2850", border: "#3a4a85",
            text: "#f2f5ff", muted: "#aab6e0", accent: "#e8b84a", accentText: "#1c1400",
            danger: "#ff8a80", success: "#6fe0a8", warning: "#ffd166", focus: "#9ec1ff"
        },
        "k2": {
            window: "#150b0e", panel: "#22131a", raised: "#321c26", border: "#6a3446",
            text: "#fbf1f3", muted: "#d0aab4", accent: "#d6455d", accentText: "#ffffff",
            danger: "#ff9a8f", success: "#7ee0a6", warning: "#f2c35b", focus: "#ffb3c1"
        }
    })
    readonly property var scheme: schemes[mode] ?? schemes["dark"]
    readonly property color window: scheme.window
    readonly property color panel: scheme.panel
    readonly property color raised: scheme.raised
    readonly property color border: scheme.border
    readonly property color text: scheme.text
    readonly property color muted: scheme.muted
    readonly property color accent: scheme.accent
    readonly property color accentText: scheme.accentText
    readonly property color danger: scheme.danger
    readonly property color success: scheme.success
    readonly property color warning: scheme.warning
    readonly property color focus: scheme.focus
    readonly property int radius: 8
    readonly property int gap: 12
    readonly property int pad: 20
}
