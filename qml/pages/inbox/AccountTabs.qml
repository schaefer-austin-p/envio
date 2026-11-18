import QtQuick 2.15
import "../../platform"
import "../../components"

// "All" (the unified inbox), then one chip per account with its unread
// count. `chosen("")` is All, otherwise the account's key.
Item {
    id: root
    required property Theme theme
    required property var accounts
    property string current: ""
    signal chosen(string key)
    // The tab of an account whose sync is stopped was tapped while already
    // shown: the way on is the Accounts page.
    signal attentionRequested()

    implicitHeight: theme.touch + theme.gu(2.5)

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: root.theme.hairline
    }
    ListView {
        id: tabs
        anchors { fill: parent; topMargin: root.theme.gu(1); bottomMargin: root.theme.gu(1.5) }
        leftMargin: root.theme.gu(2)
        rightMargin: root.theme.gu(2)
        orientation: ListView.Horizontal
        spacing: root.theme.gu(0.75)
        clip: true
        model: root.accounts
        header: Item {
            width: all.width + tabs.spacing
            height: tabs.height
            Chip {
                id: all
                objectName: "tab-all"
                theme: root.theme
                text: qsTr("All")
                selected: root.current === ""
                height: parent.height
                onClicked: root.chosen("")
            }
        }
        delegate: Chip {
            id: tab
            required property var model
            objectName: "tab-" + model.key
            theme: root.theme
            // The count is a number beside the name, not counted prose.
            text: model.unreadText !== "" ? model.display + "  " + model.unreadText : model.display
            selected: root.current === model.key
            readonly property bool offline: model.state === "offline"
            // Sign-in failed or certificate changed: sync is stopped.
            readonly property bool attention: model.attention === true
            iconName: attention ? "alert" : (offline ? "offline" : "")
            // On a selected Carbon chip (filled primary) the accent would not read.
            iconColor: selected && !root.theme.airmail ? root.theme.primaryText : root.theme.accent
            Accessible.name: attention ? qsTr("%1, sync stopped").arg(text)
                                       : (offline ? qsTr("%1, offline").arg(text) : text)
            height: tabs.height
            // A long label elides instead of filling the strip.
            width: Math.min(implicitWidth, root.width * 0.6)
            onClicked: {
                // The first tap shows the account's saved mail, the next one
                // leads to the problem.
                if (attention && selected) root.attentionRequested()
                else root.chosen(tab.model.key)
            }
        }
    }
}
