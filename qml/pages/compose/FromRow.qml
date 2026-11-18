import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import "../../platform"
import "../../components"

// Which account the message goes out from. With one account there is
// nothing to pick; with more, a tap opens the list.
Item {
    id: root
    required property Theme theme
    required property var accounts
    // The chosen account's key; `chosen` fires when the user picks another.
    property string account: ""
    property string email: ""
    // Where the address starts: the page margin, the label and a gap.
    property real labelColumn: theme.gu(9.5)
    signal chosen(string key)

    implicitHeight: theme.touch + 1

    Repeater {
        id: rows
        model: root.accounts
        delegate: Item {
            id: row
            required property var model
            visible: false
            Binding {
                target: root
                property: "email"
                value: row.model.email
                restoreMode: Binding.RestoreNone
                when: row.model.key === root.account
            }
        }
    }

    ListRow {
        id: fromRow
        objectName: "fromRow"
        theme: root.theme
        width: parent.width
        enabled: rows.count > 1
        chevron: rows.count > 1
        chevronDirection: "down"
        onClicked: sheet.open()
        Row {
            objectName: "fromContent"
            x: root.theme.gu(2)
            y: root.theme.gu(1.75)
            height: root.theme.touch - root.theme.gu(3.5)
            Text {
                textFormat: Text.PlainText
                objectName: "fromLabel"
                width: root.labelColumn - root.theme.gu(2)
                height: parent.height
                verticalAlignment: Text.AlignVCenter
                text: qsTr("From")
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 14
            }
            Text {
                textFormat: Text.PlainText
                objectName: "fromValue"
                width: root.width - root.labelColumn - fromRow.chevronSpace
                height: parent.height
                verticalAlignment: Text.AlignVCenter
                text: root.email
                elide: Text.ElideRight
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
            }
        }
    }

    Sheet {
        id: sheet
        theme: root.theme
        title: qsTr("Send from")
        Repeater {
            model: root.accounts
            delegate: ListRow {
                id: pick
                required property var model
                objectName: "account-" + model.key
                theme: root.theme
                width: sheet.width
                onClicked: { sheet.close(); root.chosen(model.key) }
                // Inset like the sheet's title and centred in the row.
                Column {
                    objectName: "accountText-" + pick.model.key
                    x: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                    width: parent.width - root.theme.gu(5)
                    Text {
                        textFormat: Text.PlainText
                        width: parent.width
                        text: pick.model.name
                        elide: Text.ElideRight
                        color: root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 16
                    }
                    Text {
                        textFormat: Text.PlainText
                        width: parent.width
                        text: pick.model.label !== "" ? pick.model.label + " \u00b7 " + pick.model.email : pick.model.email
                        elide: Text.ElideRight
                        color: root.theme.secondary
                        font.family: root.theme.fontLabel
                        font.pixelSize: 13
                    }
                }
            }
        }
    }
}
