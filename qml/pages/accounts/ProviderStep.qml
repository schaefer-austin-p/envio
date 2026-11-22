import QtQuick 2.15
import "../../platform"
import "../../components"

// Step 1 (board carbon-provider): where the mail lives. A tap picks; the
// wizard decides what the pick means. A card says how the last sign-in
// ended when it was refused.
Item {
    id: root
    required property Theme theme
    property string noticeKind: ""
    property string noticeTitle: ""
    property string noticeText: ""
    signal picked(string provider)
    signal cancelRequested()

    readonly property var choices: [
        { "id": "outlook", "name": "Outlook.com", "hint": qsTr("Hotmail, Live and MSN too") },
        { "id": "m365", "name": "Microsoft 365", "hint": qsTr("Work or school") },
        { "id": "icloud", "name": "iCloud", "hint": qsTr("Apple mail address") },
        { "id": "gmail", "name": "Gmail", "hint": qsTr("Google mail address") },
        { "id": "yahoo", "name": "Yahoo", "hint": qsTr("Yahoo mail address") },
        { "id": "other", "name": qsTr("Other"), "hint": qsTr("Any mail server; Envio finds the settings") }
    ]

    Flickable {
        anchors { top: parent.top; bottom: footer.top; left: parent.left; right: parent.right }
        contentHeight: col.height
        clip: true

        Column {
            id: col
            width: parent.width
            spacing: root.theme.gu(1.5)
            topPadding: root.theme.gu(0.5)

            NoticeCard {
                objectName: "providerNotice"
                visible: root.noticeKind !== ""
                x: root.theme.gu(2.5)
                width: parent.width - root.theme.gu(5)
                theme: root.theme
                kind: root.noticeKind
                title: root.noticeTitle
                text: root.noticeText
            }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                width: parent.width - root.theme.gu(5)
                text: qsTr("Choose where your mail lives. You can add as many accounts as you like.")
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 15
                lineHeight: 1.3
            }
            Column {
                width: parent.width
                Repeater {
                    model: root.choices
                    ListRow {
                        id: row
                        required property var modelData
                        objectName: "provider_" + modelData.id
                        theme: root.theme
                        chevron: true
                        onClicked: root.picked(modelData.id)
                        Column {
                            objectName: "providerText"
                            x: root.theme.gu(2.5)
                            anchors.verticalCenter: parent.verticalCenter
                            width: row.width - root.theme.gu(2.5) - row.chevronSpace
                            Text {
                                textFormat: Text.PlainText
                                width: parent.width
                                text: row.modelData.name
                                elide: Text.ElideRight
                                color: root.theme.text
                                font.family: root.theme.fontUi
                                font.pixelSize: 16
                            }
                            Text {
                                textFormat: Text.PlainText
                                width: parent.width
                                text: row.modelData.hint
                                elide: Text.ElideRight
                                color: root.theme.secondary
                                font.family: root.theme.fontUi
                                font.pixelSize: 13
                            }
                        }
                    }
                }
            }
        }
    }

    PageFooter {
        id: footer
        theme: root.theme
        AppButton {
            objectName: "providerCancel"
            width: parent.width
            theme: root.theme
            text: qsTr("Cancel")
            onClicked: root.cancelRequested()
        }
    }
}
