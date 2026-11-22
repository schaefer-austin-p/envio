import QtQuick 2.15
import QtQml 2.15
import "../../platform"
import "../../components"

// "Update sign-in" for an Outlook.com account on IMAP (board
// `carbon-provider`): one more Microsoft sign-in with the Graph scopes, then
// the core moves the account (#342). The page has two halves. Before the
// move it explains and opens the browser (the wizard's BrowserStep, waiting,
// cancelling and errors). Once the core reports the move (the account row's
// `moveStage`: "moving", "stuck", "signin", "done") the page shows the core's
// own words for it; "Switch anyway" is offered only when the core says so.
Item {
    id: root
    required property Theme theme
    required property var core
    // The accounts model, to find the row of `account`.
    required property var accounts
    property string account: ""
    readonly property alias session: session
    readonly property string accountName: found.name
    readonly property string accountEmail: found.email
    readonly property string stage: found.stage
    signal closed()
    // "Sign in again" (the stuck frame where IMAP refuses the sign-in).
    signal signInAgainRequested(string account)

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
        property string stage: ""
        property string title: ""
        property string text: ""
        property string hint: ""
        property string noteTitle: ""
        property string noteText: ""
        property bool canSwitch: false
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
            Binding { target: found; property: "stage"; value: row.model.moveStage || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "title"; value: row.model.moveTitle || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "text"; value: row.model.moveText || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "hint"; value: row.model.moveHint || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "noteTitle"; value: row.model.moveNoteTitle || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "noteText"; value: row.model.moveNoteText || ""; when: row.mine; restoreMode: Binding.RestoreNone }
            Binding { target: found; property: "canSwitch"; value: !!row.model.canSwitch; when: row.mine; restoreMode: Binding.RestoreNone }
        }
    }

    OAuthSession {
        id: session
        objectName: "oauthSession"
        core: root.core
        email: found.email
        account: root.account
        kind: "graph"
        // The move's own card takes over once the core reports it.
        // The admin block, a work account and a wrong account end here:
        // the page says why.
        onDeclined: function(outcome, text) {
            session.noticeKind = "warning"
            session.noticeTitle = ""
            session.noticeText = text
        }
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Update sign-in")
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
        // Moving: a progress strip, a plain heading and the core's words
        // (board `carbon-provider`, "Moving in progress").
        Rectangle {
            objectName: "moveStrip"
            visible: root.stage === "moving"
            width: parent.width
            height: 3
            radius: height / 2
            color: root.theme.hairline
            clip: true
            Rectangle {
                id: fill
                width: parent.width * 0.38
                height: parent.height
                color: root.theme.accent
                x: parent.width * 0.3
                SequentialAnimation on x {
                    running: root.stage === "moving"
                    loops: Animation.Infinite
                    NumberAnimation { from: -fill.width; to: fill.parent.width; duration: 1600; easing.type: Easing.InOutQuad }
                }
            }
        }
        Text {
            textFormat: Text.PlainText
            objectName: "moveHeading"
            visible: root.stage === "moving"
            width: parent.width
            text: found.title
            wrapMode: Text.WordWrap
            color: root.theme.heading
            font.family: root.theme.fontTitle
            font.pixelSize: 18
            font.weight: Font.DemiBold
            lineHeight: 1.3
        }
        Text {
            textFormat: Text.PlainText
            objectName: "moveBody"
            visible: root.stage === "moving"
            width: parent.width
            text: found.text
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.45
        }
        // Stuck, sign-in and done keep the board's banner.
        NoticeCard {
            objectName: "moveCard"
            visible: root.stage !== "" && root.stage !== "moving"
            width: parent.width
            theme: root.theme
            kind: root.stage === "done" ? "info" : "warning"
            title: found.title
            text: found.text
        }
        NoticeCard {
            objectName: "droppedNotice"
            visible: root.stage === "done" && found.noteTitle !== ""
            width: parent.width
            theme: root.theme
            kind: "warning"
            title: found.noteTitle
            text: found.noteText
        }
        Text {
            textFormat: Text.PlainText
            objectName: "moveHint"
            visible: root.stage !== "" && found.hint !== ""
            width: parent.width
            text: found.hint
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        // How the last sign-in attempt ended (cancelled, admin, work account).
        NoticeCard {
            objectName: "updateSignInNotice"
            visible: root.stage === "" && session.noticeKind !== ""
            width: parent.width
            theme: root.theme
            kind: session.noticeKind
            title: session.noticeTitle
            text: session.noticeText
        }
        // Before the move: why, and what changes.
        NoticeCard {
            objectName: "whyBanner"
            visible: root.stage === "" && session.state !== "confirm"
            width: parent.width
            theme: root.theme
            kind: "info"
            title: qsTr("Outlook.com works better with a new connection.")
            text: qsTr("Your mail, Outbox and drafts stay.")
        }
        Text {
            textFormat: Text.PlainText
            objectName: "explanationFirst"
            visible: root.stage === "" && session.state !== "confirm"
            width: parent.width
            text: qsTr("Envio sends any waiting changes first. Then it loads your mail again. This can take a few minutes.")
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        Text {
            textFormat: Text.PlainText
            objectName: "explanation"
            visible: root.stage === "" && session.state !== "confirm"
            width: parent.width
            text: qsTr("Microsoft opens in the browser. Sign in with %1.").arg(root.accountEmail)
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 15
            lineHeight: 1.3
        }
        Rectangle {
            objectName: "updateSignInPrivacy"
            visible: root.stage === "" && session.state !== "confirm"
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
        // Sign in again, Keep waiting and Switch anyway stack, 10 px apart.
        readonly property int stacked: (signInAgainBtn.visible ? 1 : 0) + (keepWaitingBtn.visible ? 1 : 0)
                                       + (switchAnywayBtn.visible ? 1 : 0)
        contentHeight: Math.max(1, stacked) * root.theme.controlHeight + Math.max(0, stacked - 1) * 10
        AppButton {
            objectName: "updateSignInButton"
            visible: root.stage === "" && session.state === "idle"
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
        AppButton {
            id: signInAgainBtn
            objectName: "signInAgain"
            visible: root.stage === "signin"
            width: parent.width
            theme: root.theme
            primary: true
            iconName: "log-in"
            text: qsTr("Sign in again")
            onClicked: root.signInAgainRequested(root.account)
        }
        AppButton {
            id: keepWaitingBtn
            objectName: "keepWaiting"
            y: signInAgainBtn.visible ? signInAgainBtn.height + 10 : 0
            visible: root.stage === "stuck" || root.stage === "signin"
            width: parent.width
            theme: root.theme
            primary: root.stage === "stuck" && !found.canSwitch
            text: qsTr("Keep waiting")
            onClicked: root.close()
        }
        AppButton {
            objectName: "moveClose"
            visible: root.stage === "moving"
            width: parent.width
            theme: root.theme
            text: qsTr("Close")
            onClicked: root.close()
        }
        AppButton {
            id: switchAnywayBtn
            objectName: "switchAnyway"
            y: (signInAgainBtn.visible ? signInAgainBtn.height + 10 : 0) + (keepWaitingBtn.visible ? keepWaitingBtn.height + 10 : 0)
            visible: root.stage === "stuck" && found.canSwitch
            width: parent.width
            theme: root.theme
            text: qsTr("Switch anyway")
            onClicked: root.core.switchAnyway(root.account)
        }
        AppButton {
            objectName: "moveDone"
            visible: root.stage === "done"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Done")
            onClicked: root.close()
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
