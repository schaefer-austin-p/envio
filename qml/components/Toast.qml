import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A transient message with an optional action (Undo). `show()` makes it
// visible and hides it again after `duration` ms (5000: the Undo window of
// 10-design.md). `actionTriggered` fires on a tap on the action, `expired`
// when the time ran out without one.
Rectangle {
    id: root
    required property Theme theme
    property string text: ""
    property string actionText: ""
    property int duration: 5000
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
    implicitHeight: theme.touch + theme.gu(1)
    radius: theme.controlRadius
    // Inverted: the primary role is dark in light mode and light in dark mode.
    color: theme.primary

    Timer {
        id: timer
        interval: root.duration
        onTriggered: { root.visible = false; root.expired() }
    }

    Text {
        textFormat: Text.PlainText
        anchors.left: parent.left
        anchors.leftMargin: root.theme.gu(2)
        anchors.right: action.visible ? action.left : parent.right
        anchors.rightMargin: root.theme.gu(1)
        anchors.verticalCenter: parent.verticalCenter
        text: root.text
        elide: Text.ElideRight
        color: root.theme.primaryText
        font.family: root.theme.fontUi
        font.pixelSize: 15
    }
    AbstractButton {
        id: action
        visible: root.actionText !== ""
        text: root.actionText
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: label.implicitWidth + root.theme.gu(3.5)
        implicitHeight: root.theme.touch
        onClicked: { timer.stop(); root.visible = false; root.actionTriggered() }
        contentItem: Text {
            textFormat: Text.PlainText
            id: label
            text: action.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: root.theme.primaryText
            font.family: root.theme.fontLabel
            font.pixelSize: 15
            font.weight: Font.Bold
        }
    }
}
