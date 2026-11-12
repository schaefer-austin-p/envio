import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../components/fileSize.js" as FileSize

// What to do with one attachment (board 4): Open, Save or Share, each of
// which saves it to the app's cache first and then hands it to the Content
// Hub. `attachment` is one entry of the message view's `attachments`
// ({ index, name, mime, size }).
Sheet {
    id: root
    objectName: "attachmentSheet"
    property var attachment: ({ index: 0, name: "", mime: "", size: 0 })
    // "open", "save" or "share".
    signal chosen(string mode)

    title: attachment.name

    function showFor(item) {
        attachment = item
        open()
    }

    Text {
        textFormat: Text.PlainText
        x: root.theme.gu(2.5)
        width: parent.width - root.theme.gu(5)
        text: root.attachment.mime + " · " + FileSize.fileSize(root.attachment.size)
        elide: Text.ElideRight
        color: root.theme.secondary
        font.family: root.theme.fontLabel
        font.pixelSize: 12
        bottomPadding: root.theme.gu(1)
    }
    Repeater {
        model: [
            { mode: "open", label: qsTr("Open"), caption: qsTr("Pick an app, like Document Viewer") },
            { mode: "save", label: qsTr("Save"), caption: qsTr("Into Files or another app") },
            { mode: "share", label: qsTr("Share"), caption: qsTr("Send it with another app") }
        ]
        delegate: ListRow {
            id: row
            required property var modelData
            objectName: "attachment-" + modelData.mode
            theme: root.theme
            width: root.width
            onClicked: { root.close(); root.chosen(row.modelData.mode) }
            Column {
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.5)
                width: parent.width - root.theme.gu(5)
                spacing: 2
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: row.modelData.label
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                    font.weight: Font.Medium
                }
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: row.modelData.caption
                    wrapMode: Text.WordWrap
                    color: root.theme.secondary
                    font.family: root.theme.fontUi
                    font.pixelSize: 13
                }
            }
        }
    }
    AppButton {
        objectName: "attachmentCancel"
        x: root.theme.gu(2)
        width: parent.width - root.theme.gu(4)
        theme: root.theme
        text: qsTr("Cancel")
        onClicked: root.close()
    }
}
