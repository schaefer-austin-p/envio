import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../platform"

// A centered dialog over the scrim with a `primary` (filled, listed first)
// and an optional `secondary` (outlined) action. Either may be the safe
// default: the certificate prompt makes Cancel its primary. Choosing an
// action closes the dialog after emitting its signal; the scrim does not
// dismiss it.
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

    parent: QC.Overlay.overlay
    modal: true
    closePolicy: QC.Popup.CloseOnEscape
    x: theme.gu(2)
    y: theme.gu(15)
    width: parent ? parent.width - theme.gu(4) : 0
    padding: theme.gu(2.5)

    QC.Overlay.modal: Rectangle { color: root.theme.scrim }

    background: Rectangle {
        color: root.theme.ground
        radius: root.theme.gu(1.5)
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
            wrapMode: Text.WordWrap
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
            wrapMode: Text.WordWrap
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
            onClicked: { root.primaryActivated(); root.close() }
        }
        AppButton {
            objectName: "secondaryAction"
            width: parent.width
            visible: root.secondaryText !== ""
            theme: root.theme
            text: root.secondaryText
            onClicked: { root.secondaryActivated(); root.close() }
        }
    }
}
