import QtQuick 2.15
import "../../platform"
import "../../components"

// The compose page's "More actions" sheet and the question behind its one
// action. `open()` shows the sheet; `discarded` fires once the user has
// confirmed (Discard; "Keep editing" is the default answer).
Item {
    id: root
    required property Theme theme
    signal discarded()
    // Save draft (the compose keeps its draft in Drafts and closes); only in the
    // sheet `openClosing()` shows.
    signal saved()
    property bool closing: false

    function open() { closing = false; more.open() }
    // The pane's Close (X): Save draft and Discard draft.
    function openClosing() { closing = true; more.open() }

    Sheet {
        id: more
        objectName: "discardSheet"
        theme: root.theme
        ListRow {
            objectName: "saveDraftRow"
            visible: root.closing
            theme: root.theme
            width: more.width
            onClicked: { more.close(); root.saved() }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                width: parent.width - root.theme.gu(5)
                text: qsTr("Save draft")
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
                font.weight: Font.Medium
            }
        }
        ListRow {
            objectName: "discardRow"
            theme: root.theme
            width: more.width
            onClicked: { more.close(); discardAsk.open() }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                width: parent.width - root.theme.gu(5)
                text: qsTr("Discard draft")
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
                font.weight: Font.Medium
            }
        }
    }
    AppDialog {
        id: discardAsk
        objectName: "discardAsk"
        theme: root.theme
        title: qsTr("Discard this message?")
        message: qsTr("The draft is deleted and cannot be brought back.")
        primaryText: qsTr("Keep editing")
        secondaryText: qsTr("Discard")
        onSecondaryActivated: root.discarded()
    }
}
