import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../components/fileSize.js" as FileSize

// The message's attachments as rows (board 4): name and size, no count (a
// count would be counted text). A tap says which one; the page opens the
// attachment sheet. `files` is the view's `attachments`.
Column {
    id: root
    required property Theme theme
    property var files: []
    signal fileClicked(var file)

    visible: files.length > 0
    spacing: theme.gu(1)

    Text {
        textFormat: Text.PlainText
        text: qsTr("Attachments")
        color: root.theme.secondary
        font.family: root.theme.fontMono
        font.pixelSize: 11
        font.capitalization: Font.AllUppercase
        font.letterSpacing: 0.6
    }
    Repeater {
        model: root.files
        delegate: ListRow {
            id: file
            required property var modelData
            objectName: "attachmentRow-" + modelData.index
            theme: root.theme
            width: root.width
            onClicked: root.fileClicked(file.modelData)
            Icon {
                id: clip
                theme: root.theme
                name: "clip"
                x: root.theme.gu(1.5)
                y: root.theme.gu(1.75)
                color: root.theme.text
            }
            Column {
                x: clip.x + clip.width + root.theme.gu(1.5)
                y: root.theme.gu(1.25)
                width: parent.width - x - root.theme.gu(1.5)
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: file.modelData.name
                    elide: Text.ElideMiddle
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 15
                    font.weight: Font.Medium
                }
                Text {
                    textFormat: Text.PlainText
                    text: FileSize.fileSize(file.modelData.size)
                    color: root.theme.secondary
                    font.family: root.theme.fontMono
                    font.pixelSize: 12
                }
            }
        }
    }
}
