import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../platform"

// A centered dialog over the scrim with a `primary` (filled, listed first)
// and an optional `secondary` (outlined) action. Either may be the safe
// default: the certificate prompt makes Cancel its primary. Choosing an
// action closes the dialog, then emits its signal from the event loop
// (see choose()); the scrim does not dismiss it.
QC.Popup {
    id: root
    required property Theme theme
    property string title: ""
    property string message: ""
    property string primaryText: ""
    property string secondaryText: ""
    signal primaryActivated()
    signal secondaryActivated()
    default property alias content: extra.data

    // True from the tap on an action until the dialog opens again: `closed`
    // then comes from a choice, whose signal follows a moment later.
    property bool chosen: false
    onAboutToShow: chosen = false
    // Close now, emit from the event loop. The signal may pop the page that
    // owns this dialog: calling close() on a popup whose page is going (it
    // was after the signal) segfaulted in QtQml's deferred execution (#329),
    // and the button must not be destroyed inside its own onClicked.
    function choose(signalFn) {
        chosen = true
        close()
        Qt.callLater(signalFn)
    }

    parent: QC.Overlay.overlay
    modal: true
    closePolicy: QC.Popup.CloseOnEscape
    x: theme.frameInset + theme.gu(2)
    y: theme.frameInset + theme.gu(15)
    width: parent ? parent.width - 2 * theme.frameInset - theme.gu(4) : 0
    padding: theme.gu(2.5)

    // The scrim covers the ring's inner area only: the ring stays undimmed.
    QC.Overlay.modal: Item {
        Rectangle {
            anchors.fill: parent
            anchors.margins: root.theme.frameInset
            radius: root.theme.frameInnerRadius
            color: root.theme.scrim
        }
    }

    background: Rectangle {
        color: root.theme.ground
        radius: root.theme.dialogRadius
        border.width: 1
        border.color: root.theme.hairline
    }

    contentItem: Column {
        spacing: root.theme.gu(1.75)
        Text {
            textFormat: Text.PlainText
            width: parent.width
            visible: root.title !== ""
            text: root.title
            wrapMode: Text.Wrap
            color: root.theme.heading
            font.family: root.theme.fontTitle
            font.pixelSize: 18
            font.weight: Font.DemiBold
        }
        Text {
            textFormat: Text.PlainText
            width: parent.width
            visible: root.message !== ""
            text: root.message
            wrapMode: Text.Wrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        Column { id: extra; width: parent.width; spacing: root.theme.gu(1) }
        AppButton {
            objectName: "primaryAction"
            width: parent.width
            theme: root.theme
            primary: true
            text: root.primaryText
            onClicked: root.choose(root.primaryActivated)
        }
        AppButton {
            objectName: "secondaryAction"
            width: parent.width
            visible: root.secondaryText !== ""
            theme: root.theme
            text: root.secondaryText
            onClicked: root.choose(root.secondaryActivated)
        }
    }
}
