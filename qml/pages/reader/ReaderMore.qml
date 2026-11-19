import QtQuick 2.15
import "../../platform"
import "../../components"

// The reader's overflow: the actions that do not fit the top bar. Each row
// closes the sheet and says what was chosen; the page does the work.
Sheet {
    id: root
    objectName: "readerMore"
    signal replyAllRequested()
    signal moveRequested()
    // true to flag the message, false to remove the flag.
    signal flagRequested(bool flagged)
    signal contactRequested()

    Repeater {
        model: [
            { name: "moreReplyAll", label: qsTr("Reply all"), pick: 0, icon: "reply-all" },
            { name: "moreMove", label: qsTr("Move to folder"), pick: 1, icon: "folder-input" },
            { name: "moreFlag", label: qsTr("Flag"), pick: 2, icon: "flag" },
            { name: "moreUnflag", label: qsTr("Remove flag"), pick: 3, icon: "flag-off" },
            { name: "moreContact", label: qsTr("Add sender to Contacts"), pick: 4, icon: "user-plus" }
        ]
        delegate: ListRow {
            id: row
            required property var modelData
            objectName: modelData.name
            iconName: modelData.icon
            theme: root.theme
            width: root.width
            onClicked: {
                root.close()
                switch (row.modelData.pick) {
                case 0: root.replyAllRequested(); break
                case 1: root.moveRequested(); break
                case 2: root.flagRequested(true); break
                case 3: root.flagRequested(false); break
                default: root.contactRequested()
                }
            }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                width: parent.width - root.theme.gu(5)
                text: row.modelData.label
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
                font.weight: Font.Medium
            }
        }
    }
}
