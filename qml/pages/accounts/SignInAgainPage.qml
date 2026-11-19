import QtQuick 2.15
import QtQml 2.15
import "../../platform"
import "../../components"

// "Sign in again" for an account whose Microsoft sign-in was withdrawn
// (board `carbon-ms-oauth`, frame 7): the same shape as Update password. The
// button opens the browser; waiting, cancelling and errors are the wizard's
// (BrowserStep, NoticeCard). The page closes when the core says the account
// signs in again (`resumed`).
Item {
    id: root
    required property Theme theme
    required property var core
    // The accounts model, to find the name and address of `account`.
    required property var accounts
    property string account: ""
    readonly property alias session: session
    readonly property string accountName: found.name
    readonly property string accountEmail: found.email
    signal closed()

    function close() {
        session.reset()
        root.closed()
    }
    // Back: while the browser has the user it is Cancel, and the page stays.
    function back() {
        if (session.state === "waiting" || session.state === "confirm") session.cancel()
        else root.close()
    }

    QtObject {
        id: found
        property string name: ""
        property string email: ""
    }
    // The model has no lookup by key; a row that matches copies its fields.
    Repeater {
        model: root.accounts
        delegate: Item {
            id: row
            required property var model
            visible: false
            readonly property bool mine: row.model.key === root.account
            Binding { target: found; property: "name"; value: row.model.display; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "email"; value: row.model.email; when: row.mine; restoreMode: Binding.RestoreNone }
        }
    }

    OAuthSession {
        id: session
        objectName: "oauthSession"
        core: root.core
        email: found.email
        account: root.account
        onResumed: root.close()
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Sign in again")
        onBackClicked: root.back()
    }

    Column {
        visible: session.state !== "waiting"
        anchors { top: bar.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2.5) }
        anchors.topMargin: root.theme.gu(2)
        spacing: root.theme.gu(2)

        Text {
            textFormat: Text.PlainText
            objectName: "accountName"
            width: parent.width
            text: root.accountName
            elide: Text.ElideRight
            color: root.theme.heading
            font.family: root.theme.fontUi
            font.pixelSize: 16
            font.weight: Font.Medium
        }
        // How the last attempt ended, else why the page is here.
        NoticeCard {
            objectName: "signInAgainNotice"
            width: parent.width
            theme: root.theme
            kind: session.noticeKind !== "" ? session.noticeKind : "warning"
            title: session.noticeKind !== "" ? session.noticeTitle : qsTr("Microsoft needs you to sign in again")
            text: session.noticeKind !== "" ? session.noticeText
                : qsTr("The saved sign-in for %1 expired or was withdrawn. Mail on this phone stays, and messages waiting in the Outbox go out after you sign in.").arg(root.accountEmail)
        }
        Text {
            textFormat: Text.PlainText
            width: parent.width
            visible: session.state !== "confirm"
            text: qsTr("Microsoft opens in the browser. Sign in with %1.").arg(root.accountEmail)
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        Rectangle {
            objectName: "signInAgainPrivacy"
            visible: session.state !== "confirm"
            width: parent.width
            height: privacy.implicitHeight + root.theme.gu(3)
            radius: root.theme.cardRadius
            color: root.theme.surface
            border.width: 1
            border.color: root.theme.hairline
            Icon {
                id: lock
                theme: root.theme
                name: "lock"
                size: 20
                color: root.theme.text
                x: root.theme.gu(1.75)
                y: root.theme.gu(1.5)
            }
            Text {
                textFormat: Text.PlainText
                id: privacy
                x: lock.x + lock.width + root.theme.gu(1.25)
                y: root.theme.gu(1.5)
                width: parent.width - x - root.theme.gu(1.75)
                text: qsTr("Your password never passes through Envio.")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
                lineHeight: 1.3
            }
        }
    }

    PageFooter {
        theme: root.theme
        visible: session.state !== "waiting"
        busy: session.state === "saving"
        busyText: qsTr("Signing in…")
        AppButton {
            objectName: "signInAgainButton"
            visible: session.state === "idle"
            width: parent.width
            theme: root.theme
            primary: true
            iconName: "external"
            text: qsTr("Sign in with Microsoft")
            enabled: root.accountEmail !== ""
            onClicked: session.start()
        }
        AppButton {
            objectName: "useAccount"
            visible: session.state === "confirm"
            width: parent.width
            theme: root.theme
            primary: true
            iconName: "external"
            text: qsTr("Use this account")
            onClicked: session.answer(true)
        }
    }

    BrowserStep {
        objectName: "browserStep"
        theme: root.theme
        session: session
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: session.state === "waiting"
        onCancelRequested: session.cancel()
    }
}
