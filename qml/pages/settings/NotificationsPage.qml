import QtQuick 2.15
import "../../platform"
import "../../components"

// Notifications (board 13d): a switch per account, whether a notification
// shows the sender and subject, and how to keep Envio checking while it is
// in the background (instructions only: a confined app cannot set it).
Item {
    id: root
    objectName: "notificationsPage"
    required property Theme theme
    required property var accounts
    required property var core
    signal backRequested()

    // Confirmed on the phone 2026-11-18 (Plan 9a Task 8): the list holds package
    // names ("music.ubports"), so Envio's entry is 'envio.aps', not the
    // app id "envio.aps_envio"; qtmir stops suspending it at once.
    readonly property string exemptCommand:
        "gsettings get com.canonical.qtmir lifecycle-exempt-appids\n" +
        "gsettings set com.canonical.qtmir lifecycle-exempt-appids \"[<the apps it printed>, 'envio.aps']\""

    // The command as shown: a zero-width space after "-" and "." lets a
    // narrow block break there, not inside a word. `exemptCommand` itself
    // stays exact.
    readonly property string shownCommand: exemptCommand.replace(/([-.])/g, "$1\u200b")

    // Whether any account notifies; the text switch and the help need one.
    property bool anyOn: false
    function recount() {
        let on = false
        for (let i = 0; i < col.children.length; i++)
            if (col.children[i].notifyOn === true) on = true
        anyOn = on
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Notifications")
        onBackClicked: root.backRequested()
    }

    Flickable {
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        contentHeight: col.height + root.theme.gu(2)
        Column {
            id: col
            width: parent.width
            spacing: root.theme.gu(0.75)

            component Eyebrow: Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2)
                topPadding: root.theme.gu(2)
                width: col.width - root.theme.gu(4)
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.eyebrowFamily
                font.pixelSize: root.theme.eyebrowSize
                font.weight: root.theme.eyebrowWeight
                font.letterSpacing: root.theme.eyebrowSpacing
                font.capitalization: root.theme.eyebrowCaps ? Font.AllUppercase : Font.MixedCase
            }
            component Help: Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2)
                width: col.width - root.theme.gu(4)
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 13
            }

            Eyebrow { text: qsTr("Notify me about new mail") }
            Repeater {
                model: root.accounts
                delegate: ListRow {
                    id: acct
                    required property var model
                    readonly property bool notifyOn: acct.model.notify === true
                    objectName: "notifyRow-" + acct.model.key
                    theme: root.theme
                    width: col.width
                    separator: false
                    implicitHeight: root.theme.touch + root.theme.gu(2.5)
                    onNotifyOnChanged: root.recount()
                    Component.onCompleted: root.recount()
                    Component.onDestruction: Qt.callLater(root.recount)
                    onClicked: root.core.setNotify(acct.model.key, !acct.notifyOn)
                    Text {
                        textFormat: Text.PlainText
                        x: root.theme.gu(2)
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - x - accountToggle.width - root.theme.gu(4)
                        text: acct.model.email
                        elide: Text.ElideRight
                        color: root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 16
                    }
                    Toggle {
                        id: accountToggle
                        objectName: "notify-" + acct.model.key
                        theme: root.theme
                        anchors { right: parent.right; rightMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                        text: acct.model.email
                        checked: acct.notifyOn
                        onToggled: root.core.setNotify(acct.model.key, checked)
                        // A save that failed leaves the model as it was, so
                        // the binding would not run again: restore it.
                        Connections {
                            target: root.core
                            function onNotice() { accountToggle.checked = Qt.binding(function() { return acct.notifyOn }) }
                        }
                    }
                }
            }
            Help {
                objectName: "noNotifications"
                visible: !root.anyOn
                height: visible ? implicitHeight + root.theme.gu(1) : 0
                text: qsTr("No notifications. New mail still arrives while Envio is open.")
            }
            Rectangle {
                x: root.theme.gu(2)
                width: col.width - root.theme.gu(4)
                height: 1
                color: root.theme.hairline
            }

            ListRow {
                id: textRow
                objectName: "notifyTextRow"
                theme: root.theme
                width: col.width
                separator: false
                implicitHeight: Math.max(root.theme.touch + root.theme.gu(2.5), textCol.height + root.theme.gu(2.5))
                enabled: root.anyOn
                onClicked: root.core.setNotifyText(!root.core.notifyShowText)
                Column {
                    id: textCol
                    x: root.theme.gu(2)
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - textToggle.width - root.theme.gu(4)
                    opacity: root.anyOn ? 1 : 0.45
                    Text {
                        objectName: "notifyTextLabel"
                        textFormat: Text.PlainText
                        width: parent.width
                        text: qsTr("Show sender and subject")
                        wrapMode: Text.WordWrap
                        color: root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 16
                    }
                    Text {
                        objectName: "notifyTextOff"
                        visible: !root.anyOn
                        textFormat: Text.PlainText
                        width: parent.width
                        text: qsTr("Turn on an account first.")
                        wrapMode: Text.WordWrap
                        color: root.theme.secondary
                        font.family: root.theme.fontUi
                        font.pixelSize: 13
                    }
                }
                Toggle {
                    id: textToggle
                    objectName: "notifyText"
                    theme: root.theme
                    anchors { right: parent.right; rightMargin: root.theme.gu(2); verticalCenter: parent.verticalCenter }
                    text: qsTr("Show sender and subject")
                    enabled: root.anyOn
                    checked: root.core.notifyShowText
                    onToggled: root.core.setNotifyText(checked)
                    Connections {
                        target: root.core
                        function onNotice() { textToggle.checked = Qt.binding(function() { return root.core.notifyShowText }) }
                    }
                }
            }
            Help {
                objectName: "notifyTextCaption"
                visible: root.anyOn
                height: visible ? implicitHeight + root.theme.gu(1) : 0
                text: qsTr("Off: a notification says only the account and how many new messages. On: it also shows who wrote and the subject, even on the lock screen.")
            }

            Column {
                id: background
                objectName: "backgroundHelp"
                visible: root.anyOn
                width: col.width
                height: visible ? implicitHeight : 0
                spacing: root.theme.gu(1)
                Rectangle {
                    x: root.theme.gu(2)
                    width: col.width - root.theme.gu(4)
                    height: 1
                    color: root.theme.hairline
                }
                Eyebrow { text: qsTr("Keep checking in the background") }
                Help {
                    text: qsTr("Ubuntu Touch suspends apps that are not on screen, so Envio stops checking. To keep it running, either:")
                }
                Help {
                    textFormat: Text.RichText
                    text: qsTr("<b>UT Tweak Tool:</b> Apps + Scopes, Envio, then turn on Prevent app suspension.")
                }
                Help {
                    textFormat: Text.RichText
                    text: qsTr("<b>Terminal:</b> run these. The second keeps the apps already in the list.")
                }
                Rectangle {
                    x: root.theme.gu(2)
                    width: col.width - root.theme.gu(4)
                    height: commandText.implicitHeight + root.theme.gu(3)
                    radius: root.theme.fieldRadius
                    color: root.theme.selected
                    border.width: 1
                    border.color: root.theme.hairline
                    Text {
                        id: commandText
                        objectName: "exemptCommand"
                        textFormat: Text.PlainText
                        anchors { fill: parent; margins: root.theme.gu(1.5) }
                        text: root.shownCommand
                        wrapMode: Text.Wrap
                        color: root.theme.text
                        font.family: root.theme.fontAddress
                        font.pixelSize: 11
                    }
                }
            }
        }
    }
}
