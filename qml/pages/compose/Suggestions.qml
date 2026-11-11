import QtQuick 2.15
import "../../platform"
import "../../components"

// The address suggestions under a recipient field: name, address, and where
// the core knows them from (a contact, or the last time you wrote to them).
// A tap picks one; the field takes the entry. Names come from other people,
// so they are shown as plain text only.
Column {
    id: root
    required property Theme theme
    required property var core
    required property var model
    // What the rows must answer: a row for another query is an earlier,
    // slower answer and stays hidden.
    property string query: ""
    signal chosen(string entry)

    // "Name <address>", the name quoted when it holds a separator.
    function entry(name, address) {
        if (name === "" || name === address) return address
        const plain = /^[^",;<>@]+$/.test(name)
        return (plain ? name : "\"" + name.replace(/(["\\])/g, "\\$1") + "\"") + " <" + address + ">"
    }

    Repeater {
        id: rows
        model: root.model
        delegate: ListRow {
            id: row
            required property var model
            required property int index
            objectName: "suggestion-" + index
            visible: model.query === root.query
            theme: root.theme
            width: root.width
            height: visible ? implicitHeight : 0
            focusPolicy: Qt.NoFocus
            onClicked: root.chosen(root.entry(model.name, model.address))
            Column {
                width: parent.width
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    visible: row.model.name !== ""
                    text: row.model.name
                    elide: Text.ElideRight
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                }
                Text {
                    textFormat: Text.PlainText
                    objectName: "suggestionAddress-" + row.index
                    width: parent.width
                    text: row.model.address
                    elide: Text.ElideRight
                    color: root.theme.secondary
                    font.family: root.theme.fontMono
                    font.pixelSize: 13
                }
                Text {
                    textFormat: Text.PlainText
                    objectName: "suggestionNote-" + row.index
                    width: parent.width
                    visible: text !== ""
                    text: row.model.fromContacts ? qsTr("from Contacts")
                        : row.model.lastSeen > 0 ? qsTr("written to %1").arg(when.text) : ""
                    color: root.theme.secondary
                    font.family: root.theme.fontUi
                    font.pixelSize: 12
                    DateText {
                        id: when
                        visible: false
                        theme: root.theme
                        bucket: row.model.lastSeen > 0 ? root.core.dateBucket(row.model.lastSeen) : "none"
                        date: row.model.lastSeen
                    }
                }
            }
        }
    }
}
