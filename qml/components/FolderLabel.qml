import QtQuick 2.15
import "../platform"

// A folder's name for people: the translated role name when the folder has
// a role, never its raw path ("INBOX", "[Gmail]/Sent Mail"); the server's
// name for a folder without one.
Text {
    id: root
    required property Theme theme
    property string role: ""
    property string folderName: ""

    text: {
        switch (role) {
        case "inbox": return qsTr("Inbox")
        case "sent": return qsTr("Sent")
        case "drafts": return qsTr("Drafts")
        case "trash": return qsTr("Trash")
        case "archive": return qsTr("Archive")
        case "junk": return qsTr("Junk")
        default: return folderName
        }
    }
    elide: Text.ElideRight
    color: theme.text
    font.family: theme.fontUi
    font.pixelSize: 16
}
