import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

// Step 3: what the new account is called and whether it is the default
// sender. Sync and retention are fixed in 0.1.0 and shown as text.
Item {
    id: root
    required property Theme theme
    property string accountName: ""
    property alias isDefault: defaultToggle.checked
    signal done()

    Platform.KeyboardAvoider { objectName: "keyboardAvoider"; flickable: flick }

    component Caption: Text {
        textFormat: Text.PlainText
        width: parent.width
        wrapMode: Text.WordWrap
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 13
        lineHeight: 1.3
    }
    component Heading: Text {
        textFormat: Text.PlainText
        width: parent.width
        color: root.theme.text
        font.family: root.theme.fontUi
        font.pixelSize: 16
        font.weight: Font.Medium
    }

    Flickable {
        id: flick
        objectName: "stepFlick"
        anchors { top: parent.top; bottom: footer.top; left: parent.left; right: parent.right }
        contentHeight: col.height + root.theme.gu(4)
        clip: true

        Column {
            id: col
            x: root.theme.gu(2.5)
            y: root.theme.gu(2)
            width: root.width - root.theme.gu(5)
            spacing: theme.gu(2)

            // Envio has no rename call yet: the name comes from the settings
            // that were found, so it is shown, not edited.
            Field {
                objectName: "accountName"
                width: parent.width
                theme: root.theme
                label: qsTr("Account name")
                text: root.accountName
                readOnly: true
            }
            Row {
                width: parent.width
                spacing: root.theme.gu(1)
                Column {
                    width: parent.width - defaultToggle.width - root.theme.gu(1)
                    anchors.verticalCenter: parent.verticalCenter
                    Heading { text: qsTr("Use as default sender") }
                    Caption { text: qsTr("New messages start from this account") }
                }
                Toggle { id: defaultToggle; objectName: "defaultToggle"; theme: root.theme; text: qsTr("Use as default sender") }
            }
            Column {
                width: parent.width
                spacing: 2
                Heading { text: qsTr("Sync") }
                Text {
                    textFormat: Text.PlainText
                    objectName: "syncText"
                    width: parent.width
                    text: qsTr("While Envio is open")
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 15
                }
                Caption { text: qsTr("Envio checks for new mail while it is open. Push is not available yet.") }
            }
            Column {
                width: parent.width
                spacing: 2
                Heading { text: qsTr("Keep mail for") }
                Text {
                    textFormat: Text.PlainText
                    objectName: "keepText"
                    width: parent.width
                    text: qsTr("Last 30 days")
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 15
                }
            }
        }
    }

    PageFooter {
        id: footer
        theme: root.theme
    AppButton {
        objectName: "doneButton"
        width: parent.width
        theme: root.theme
        primary: true
        text: qsTr("Done")
        onClicked: root.done()
    }
    }
}
