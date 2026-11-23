import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import "../platform"

// A transient message with an optional action (Undo), drawn as the board's
// compact pill (carbon-two-pane, "The moment after clicking row 3"): shrink
// to fit, a check icon, 14 px text, a soft shadow. The caller centres it
// (`anchors.horizontalCenter`) and sets `anchors.bottomMargin` to
// `theme.toastBottomMargin`; it is never wider than `maxWidth`, and long
// text elides. `show()` makes it visible and hides it again after
// `duration` ms (5000: the Undo window of 10-design.md). `actionTriggered`
// fires on a tap on the action, `expired` when the time ran out without one.
Rectangle {
    id: root
    required property Theme theme
    property string text: ""
    property string actionText: ""
    property int duration: 5000
    // Phone width: a 2 gu gutter on each side.
    property real maxWidth: parent ? parent.width - theme.gu(4) : theme.gu(40)
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
    implicitWidth: Math.min(maxWidth, row.implicitWidth + theme.gu(5))
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

    // A soft drop shadow (the board's 0 6px 20px): growing, faint layers.
    Repeater {
        model: 5
        Rectangle {
            required property int index
            z: -1
            x: -index * 2
            y: 5 - index * 2
            width: root.width + index * 4
            height: root.height + index * 4
            radius: root.theme.airmail ? height / 2 : root.theme.toastRadius
            color: Qt.rgba(root.theme.toastShadow.r, root.theme.toastShadow.g, root.theme.toastShadow.b,
                           root.theme.toastShadow.a / 5)
        }
    }

    RowLayout {
        id: row
        objectName: "toastRow"
        anchors {
            left: parent.left; leftMargin: root.theme.gu(2.5)
            right: parent.right; rightMargin: action.visible ? root.theme.gu(1) : root.theme.gu(2.5)
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
            Layout.fillWidth: true
            Layout.preferredWidth: implicitWidth
            textFormat: Text.PlainText
            text: root.text
            elide: Text.ElideRight
            color: root.theme.toastText
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
        AbstractButton {
            id: action
            visible: root.actionText !== ""
            text: root.actionText
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
}
