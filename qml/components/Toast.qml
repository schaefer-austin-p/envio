import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import "../platform"

// A transient message with an optional action (Undo), drawn as the board's
// compact pill (carbon-two-pane, "The moment after clicking row 3"): shrink
// to fit, a check icon, 14 px text, a soft shadow. The caller centres it
// (`anchors.horizontalCenter`, or `x`) and sits it `theme.toastBottomMargin`
// above its bottom; it is never wider than `maxWidth`, and long text
// elides. `show()` makes it visible and hides it again after `duration` ms
// (5000: the Undo window of 10-design.md). `actionTriggered` fires on a tap
// on the action, `expired` when the time ran out without one.
Rectangle {
    id: root
    required property Theme theme
    property string text: ""
    property string actionText: ""
    property int duration: 5000
    // Phone width: a 2 gu gutter on each side; never wider than 60 gu, so a
    // long notice on a window-wide parent stays a pill.
    property real maxWidth: parent ? Math.min(parent.width - theme.gu(4), theme.gu(60)) : theme.gu(40)
    // Window-hosted toasts never overlap: one with a higher rank (ties: the
    // later sibling) rises above the visible ones below it, a gu(1) gap
    // apart. Pages subtract `lift` from their hosted `y`.
    property int stackRank: 0
    readonly property real lift: {
        if (!parent) return 0
        const kids = parent.children
        let mine = -1
        for (let i = 0; i < kids.length; i++) if (kids[i] === root) mine = i
        let up = 0
        for (let i = 0; i < kids.length; i++) {
            const k = kids[i]
            if (k === root || !k.visible || k.stackRank === undefined || k.maxWidth === undefined) continue
            if (k.stackRank < stackRank || (k.stackRank === stackRank && i < mine))
                up += k.height + theme.gu(1)
        }
        return up
    }
    signal actionTriggered()
    signal expired()

    function show(message, action) {
        text = message
        actionText = action === undefined ? "" : action
        visible = true
        timer.restart()
    }
    function hide() {
        timer.stop()
        visible = false
    }

    visible: false
    // The pill is sized by its text row; the Undo button (touch height) may
    // overhang it by a pixel or two, so its hit area stays 44 px.
    implicitWidth: Math.min(maxWidth, theme.gu(2.5) + row.implicitWidth
                            + (action.visible ? theme.gu(2.5) + action.implicitWidth : theme.gu(2.5)))
    implicitHeight: Math.max(row.implicitHeight + theme.gu(3), action.visible ? theme.touch : 0)
    width: implicitWidth
    height: implicitHeight
    radius: theme.airmail ? height / 2 : theme.toastRadius
    // Inverted: dark in light mode and light in dark mode.
    color: theme.toastFill

    Timer {
        id: timer
        interval: root.duration
        onTriggered: { root.visible = false; root.expired() }
    }

    // A soft drop shadow (the board's 0 6px 20px rgba(.., .3)): 14 faint
    // layers growing 1.5 px each, so the falloff reads as a blur, reaching
    // 21 px around the pill and offset 6 px down.
    Repeater {
        model: 14
        Rectangle {
            required property int index
            readonly property real grow: (index + 1) * 1.5
            z: -1
            x: -grow
            y: 6 - grow
            width: root.width + grow * 2
            height: root.height + grow * 2
            radius: root.theme.airmail ? height / 2 : root.theme.toastRadius + grow
            color: Qt.rgba(root.theme.toastShadow.r, root.theme.toastShadow.g, root.theme.toastShadow.b,
                           root.theme.toastShadow.a / 14 * 0.7)
        }
    }

    RowLayout {
        id: row
        objectName: "toastRow"
        anchors {
            left: parent.left; leftMargin: root.theme.gu(2.5)
            right: action.visible ? action.left : parent.right
            rightMargin: action.visible ? root.theme.gu(1.5) : root.theme.gu(2.5)
            verticalCenter: parent.verticalCenter
        }
        spacing: root.theme.gu(1.25)

        Icon {
            objectName: "toastIcon"
            theme: root.theme
            name: "check"
            color: root.theme.toastText
            size: 16
        }
        Text {
            objectName: "toastText"
            Layout.fillWidth: true
            Layout.preferredWidth: implicitWidth
            textFormat: Text.PlainText
            text: root.text
            elide: Text.ElideRight
            color: root.theme.toastText
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
    }
    AbstractButton {
        id: action
        objectName: "toastAction"
        visible: root.actionText !== ""
        text: root.actionText
        anchors.right: parent.right
        anchors.rightMargin: root.theme.gu(1)
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: label.implicitWidth + root.theme.gu(2.5)
        implicitHeight: root.theme.touch
        onClicked: { timer.stop(); root.visible = false; root.actionTriggered() }
        contentItem: Text {
            textFormat: Text.PlainText
            id: label
            text: action.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: root.theme.toastText
            font.family: root.theme.fontLabel
            font.pixelSize: 14
            font.weight: Font.DemiBold
            font.underline: true
        }
    }
}
