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
            text: model.unread > 0 ? model.name + "  " + model.unread : model.name
            selected: root.current === model.key
            height: tabs.height
            onClicked: root.chosen(tab.model.key)
        }
    }
}
