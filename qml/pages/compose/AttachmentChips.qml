import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"
import "../../components/fileSize.js" as FileSize

// The files that go with the message: name, size, and a button to take one
// off. `files` is a list of { path, name, size }; a size below zero is not
// known (a file the draft already held). File names come from other people
// (a forward carries the original's), so they are plain text.
Column {
    id: root
    required property Theme theme
    property var files: []
    signal removeRequested(int index)

    Repeater {
        model: root.files
        delegate: Item {
            id: chip
            required property var modelData
            required property int index
            objectName: "attachment-" + index
            width: root.width
            height: root.theme.touch
            Icon {
                id: clip
                theme: root.theme
                name: "clip"
                size: 20
                color: root.theme.secondary
                anchors { left: parent.left; leftMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
            }
            Text {
                textFormat: Text.PlainText
                objectName: "attachmentName-" + chip.index
                anchors { left: clip.right; leftMargin: root.theme.gu(1); right: size.left; rightMargin: root.theme.gu(1); verticalCenter: parent.verticalCenter }
                text: chip.modelData.name
                elide: Text.ElideMiddle
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 15
            }
            Text {
                id: size
                textFormat: Text.PlainText
                objectName: "attachmentSize-" + chip.index
                anchors { right: remove.left; verticalCenter: parent.verticalCenter }
                visible: chip.modelData.size >= 0
                text: chip.modelData.size >= 0 ? FileSize.fileSize(chip.modelData.size) : ""
                color: root.theme.secondary
                font.family: root.theme.fontMono
                font.pixelSize: 13
            }
            IconButton {
                id: remove
                objectName: "attachmentRemove-" + chip.index
                theme: root.theme
                iconName: "close"
                text: qsTr("Remove attachment")
                anchors { right: parent.right; rightMargin: root.theme.gu(0.5); verticalCenter: parent.verticalCenter }
                onClicked: root.removeRequested(chip.index)
            }
        }
    }
}
