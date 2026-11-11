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

    function open() { more.open() }

    Sheet {
        id: more
        theme: root.theme
        ListRow {
            objectName: "discardRow"
            theme: root.theme
            width: more.width
            onClicked: { more.close(); discardAsk.open() }
            Text {
                textFormat: Text.PlainText
                text: qsTr("Discard draft")
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
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
