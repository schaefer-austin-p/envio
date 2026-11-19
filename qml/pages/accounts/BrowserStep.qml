import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"

// Waiting for the browser (board `carbon-ms-oauth`, frame 2): the user signs
// in on Microsoft's page and the redirect brings them back. The one button is
// Cancel, outlined; there is no primary action while the browser has the user.
// `session` opens the browser and ends the wait; this draws it.
Item {
    id: root
    required property Theme theme
    required property var session
    signal cancelRequested()

    Column {
        x: root.theme.gu(2.5)
        y: root.theme.gu(1)
        width: root.width - root.theme.gu(5)
        spacing: root.theme.gu(1.75)

        // An open-ended wait: a bar of the accent runs along its track.
        Rectangle {
            objectName: "waitBar"
            width: parent.width
            height: 3
            radius: height / 2
            color: root.theme.hairline
            clip: true
            Rectangle {
                id: run
                width: parent.width * 0.38
                height: parent.height
                color: root.theme.accent
                NumberAnimation on x {
                    from: -run.width
                    to: run.parent.width
                    duration: 1400
                    loops: Animation.Infinite
                    running: root.visible
                }
            }
        }
        Text {
            textFormat: Text.PlainText
            objectName: "waitTitle"
            width: parent.width
            text: qsTr("Waiting for Microsoft")
            wrapMode: Text.WordWrap
            color: root.theme.heading
            font.family: root.theme.fontTitle
            font.pixelSize: 18
            font.weight: Font.DemiBold
            lineHeight: 1.2
        }
        Text {
            textFormat: Text.PlainText
            objectName: "waitText"
            width: parent.width
            text: qsTr("Finish signing in to %1 in the browser. Envio carries on by itself when you are done.").arg(root.session.email)
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        AbstractButton {
            id: again
            objectName: "openBrowserAgain"
            height: Math.max(44, root.theme.touch)
            implicitWidth: row.implicitWidth
            enabled: root.session.url !== ""
            opacity: enabled ? 1 : 0.45
            contentItem: Row {
                id: row
                spacing: root.theme.gu(0.75)
                Text {
                    textFormat: Text.PlainText
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Open the browser again")
                    color: root.theme.primary
                    font.family: root.theme.fontUi
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                }
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    theme: root.theme
                    name: "external"
                    size: 18
                    color: root.theme.primary
                }
            }
            onClicked: root.session.reopen()
        }
        Text {
            textFormat: Text.PlainText
            objectName: "waitHint"
            width: parent.width
            text: qsTr("This stops by itself after 5 minutes.")
            wrapMode: Text.WordWrap
            color: root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 13
            lineHeight: 1.3
        }
    }

    PageFooter {
        theme: root.theme
        AppButton {
            objectName: "cancelSignIn"
            width: parent.width
            theme: root.theme
            text: qsTr("Cancel")
            onClicked: root.cancelRequested()
        }
    }
}
