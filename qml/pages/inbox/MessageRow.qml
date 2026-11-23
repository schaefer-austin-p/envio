import QtQuick 2.15
import "../../platform"
import "../../bindle/Behaviours" as Behaviours
import "../../components"
import "../../platform/stampTint.js" as StampTint

// One message in the list (board 1; board 9 in selection mode, where a
// checkbox takes the unread dot's place). No swipe: tap opens, long-press
// selects (both come from ListRow). `model` is the row's roles.
ListRow {
    id: root
    required property var model
    lightRule: true
    required property int index
    // Any row is selected: show the checkboxes.
    property bool selecting: false
    // The unified inbox names each message's account.
    property bool showAccount: false
    // False while a compose fills the wide pane: no row marks the reader's message then.
    property bool showOpen: true

    objectName: "row-" + model.id
    // The open row (the one the reader shows) has the selected fill too.
    selected: model.selected || (showOpen && model.open === true)
    // The keyboard-focus row (Up/Down): hover fill and a 2 px outline in the
    // text colour, apart from the open row's fill and accent bar.
    readonly property bool kbdFocus: model.kbdFocus === true
    // Mouse hover fills like the keyboard cue; touch gives none. On the
    // phone Qt turns a touch drag into mouse moves that leave a plain
    // HoverHandler hovered for good (#397), so the hover is bindle's
    // TouchSafeHover: a touch clears it until the mouse moves again.
    // `selected` and the open fill win over it.
    readonly property bool pointerOver: hover.hovered
    cued: kbdFocus || pointerOver
    // The row's context request (right-click), at a point in the row.
    signal contextRequested(int id, real x, real y)
    // Parented to the row, not its content: a fill-parent Item in the
    // content would count in the content's height and grow the row.
    Behaviours.TouchSafeHover {
        id: hover
        objectName: "rowHover"
        parent: root
    }
    TapHandler {
        objectName: "rowRightClick"
        // Mouse only: a touch tap has no button, so without this a finger
        // tap opened the menu instead of the message on the phone.
        acceptedDevices: PointerDevice.Mouse
        acceptedButtons: Qt.RightButton
        onTapped: function(point) { root.contextRequested(root.model.id, point.position.x, point.position.y) }
    }
    Rectangle {
        objectName: "focusOutline"
        parent: root
        // Inset: off the pane edge and divider, and clear of the 3 px open bar.
        anchors { fill: parent; leftMargin: 4; topMargin: 2; rightMargin: 2; bottomMargin: 2 }
        color: "transparent"
        border.width: 2
        border.color: root.theme.text
        visible: root.kbdFocus
    }

    // The open row's 3 px accent bar. Parented to the row, not its content,
    // so its height (the row's) does not feed back into the row's height.
    Rectangle {
        objectName: "openBar"
        parent: root
        x: 0
        y: 0
        width: 3
        height: root.height
        color: root.theme.accent
        visible: root.showOpen && root.model.open === true
    }

    // The slot: an 8 px dot, or the checkbox.
    Rectangle {
        objectName: "unreadDot"
        x: root.theme.gu(2) + (root.theme.airmail ? (root.theme.stampWidth - width) / 2 : 0)
        y: root.theme.gu(1.75) + (root.selecting ? (root.theme.airmail ? 13 : 2) : 7)
        width: root.selecting ? 22 : 8
        height: width
        radius: root.selecting ? root.theme.checkRadius : width / 2
        visible: root.selecting || (root.theme.showUnreadDot && root.model.unread)
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
    // Airmail: the sender's stamp (the checkbox replaces it while selecting).
    Loader {
        objectName: "stamp"
        active: root.theme.airmail
        visible: !root.selecting
        x: root.theme.gu(2)
        y: root.theme.gu(1.75)
        sourceComponent: Avatar {
            theme: root.theme
            initials: root.model.fromAddress === "" ? "" : StampTint.initials(root.model.from)
            blue: StampTint.isBlue(root.model.fromAddress)
        }
    }
    Column {
        id: col
        objectName: "rowText"
        x: root.theme.gu(2) + (root.theme.airmail ? root.theme.stampWidth : 22) + root.theme.gu(1.5)
        y: root.theme.gu(1.75)
        width: parent.width - x - root.theme.gu(2)
        spacing: 3
        Item {
            width: parent.width
            height: sender.implicitHeight
            Text {
                textFormat: Text.PlainText
                id: sender
                objectName: "rowSender"
                anchors { left: parent.left; right: tail.left; rightMargin: root.theme.gu(1) }
                text: root.model.from
                elide: Text.ElideRight
                color: root.theme.heading
                font.family: root.theme.fontLabel
                font.pixelSize: 14
                font.weight: root.model.unread ? root.theme.unreadWeight : Font.Normal
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
        // A local draft with no server copy (Drafts list). The core gives the
        // words: "Not uploaded", or the reason the server refused (never empty
        // while notUploaded, see envio-core text/draft_chip.rs).
        Item {
            visible: root.model.notUploaded === true
            width: parent.width
            // 8 px above the chip: the Column's 3 px plus 5.
            height: visible ? statusChip.height + 5 : 0
            StatusChip {
                id: statusChip
                objectName: "statusChip"
                y: 5
                theme: root.theme
                maxWidth: parent.width
                text: root.model.notUploadedText
            }
        }
        Text {
            textFormat: Text.PlainText
            visible: root.showAccount
            width: parent.width
            topPadding: 3
            text: root.model.accountName.toUpperCase()
            elide: Text.ElideRight
            color: root.theme.secondary
            font.family: root.theme.fontLabel
            font.pixelSize: 11
            font.letterSpacing: 0.5
        }
    }
}
