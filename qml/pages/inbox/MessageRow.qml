import QtQuick 2.15
import "../../platform"
import "../../components"

// One message in the list (board 1; board 9 in selection mode, where a
// checkbox takes the unread dot's place). No swipe: tap opens, long-press
// selects (both come from ListRow). `model` is the row's roles.
ListRow {
    id: root
    required property var model
    required property int index
    // Any row is selected: show the checkboxes.
    property bool selecting: false
    // The unified inbox names each message's account.
    property bool showAccount: false

    objectName: "row-" + model.id
    selected: model.selected

    // The slot: an 8 px dot, or the checkbox.
    Rectangle {
        x: root.theme.gu(2)
        y: root.theme.gu(1.75) + (root.selecting ? 2 : 7)
        width: root.selecting ? 22 : 8
        height: width
        radius: 4
        visible: root.selecting || root.model.unread
        color: root.selecting ? (root.model.selected ? root.theme.primary : "transparent") : root.theme.accent
        border.width: root.selecting ? (root.model.selected ? 0 : 2) : 0
        border.color: root.theme.field
        Icon {
            visible: root.selecting && root.model.selected
            theme: root.theme
            name: "check"
            size: root.theme.iconSmall
            color: root.theme.primaryText
            anchors.centerIn: parent
        }
    }
    Column {
        id: col
        x: root.theme.gu(2) + 22 + root.theme.gu(1.5)
        y: root.theme.gu(1.75)
        width: parent.width - x - root.theme.gu(2)
        spacing: 3
        Item {
            width: parent.width
            height: sender.implicitHeight
            Text {
                textFormat: Text.PlainText
                id: sender
                anchors { left: parent.left; right: tail.left; rightMargin: root.theme.gu(1) }
                text: root.model.from
                elide: Text.ElideRight
                color: root.theme.heading
                font.family: root.theme.airmail ? root.theme.fontUi : root.theme.fontMono
                font.pixelSize: 14
                font.weight: root.model.unread ? Font.DemiBold : Font.Normal
            }
            Row {
                id: tail
                anchors { right: parent.right; verticalCenter: sender.verticalCenter }
                spacing: root.theme.gu(0.75)
                Icon {
                    visible: root.model.flagged
                    objectName: "flagIcon"
                    theme: root.theme
                    name: "flag"
                    size: root.theme.iconSmall
                    color: root.theme.accent
                    anchors.verticalCenter: parent.verticalCenter
                }
                Icon {
                    visible: root.model.hasAttachments
                    objectName: "clipIcon"
                    theme: root.theme
                    name: "clip"
                    size: root.theme.iconSmall
                    color: root.theme.secondary
                    anchors.verticalCenter: parent.verticalCenter
                }
                DateText {
                    theme: root.theme
                    bucket: root.model.dateBucket
                    date: root.model.date
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.model.subject
            elide: Text.ElideRight
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            font.weight: root.model.unread ? Font.DemiBold : Font.Normal
        }
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.model.preview
            elide: Text.ElideRight
            color: root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
        Text {
            textFormat: Text.PlainText
            visible: root.showAccount
            width: parent.width
            topPadding: 3
            text: root.model.accountName.toUpperCase()
            elide: Text.ElideRight
            color: root.theme.secondary
            font.family: root.theme.fontMono
            font.pixelSize: 11
            font.letterSpacing: 0.5
        }
    }
}
