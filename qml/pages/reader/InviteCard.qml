import QtQuick 2.15
import "../../components"
import "../../platform"

// The meeting invite card above the body (Plan 10, boards "Meeting invite
// card"). `view` is the `messageOpened` map; `view.invite` is the card
// contract (method, state, stateText, times, canAnswer, canAddToCalendar, ...). Rust picks and
// translates the state text; this only shows it. Answers go to the core,
// which re-emits the message with the new card.
Rectangle {
    id: root
    required property Theme theme
    required property var core
    property var view: null
    // Where attachments are written, and whether Content Hub can take the
    // calendar file (false on the desktop). Both come from the page.
    property string attachmentsDir: ""
    property bool hubAvailable: false
    // The page keeps the pending export; it is told before the core writes
    // the calendar file (made importable there, issue #206).
    signal calendarRequested()

    readonly property var invite: view !== null && view.invite !== undefined ? view.invite : null
    // "Change" was tapped: the buttons are back although we answered.
    property bool changing: false
    property bool noteOpen: false

    readonly property string cardState: invite !== null ? invite.state : ""
    readonly property bool buttonsShown: invite !== null && invite.canAnswer
        && (cardState === "open" || cardState === "updated" || (cardState === "answered" && changing))
    readonly property bool answeredShown: invite !== null && cardState === "answered" && !changing
    readonly property bool replyShown: invite !== null && cardState === "replied"
    readonly property string eyebrow: {
        if (invite === null) return ""
        if (invite.method === "reply") return qsTr("Reply")
        if (cardState === "updated") return invite.stateText !== "" ? invite.stateText : qsTr("Updated")
        if (cardState === "cancelled") return invite.stateText !== "" ? invite.stateText : qsTr("Cancelled")
        return qsTr("Invitation")
    }

    // `YYYY-MM-DD` and `YYYY-MM-DDTHH:MM:SS` as local wall time.
    function wall(text) {
        const m = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?/.exec(text)
        if (m === null) return null
        return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Number(m[4] || 0), Number(m[5] || 0))
    }
    function dateTimes(inv) {
        if (inv.allDay) {
            const s = Date.fromLocaleString(Qt.locale(), inv.startDate, "yyyy-MM-dd")
            const x = Date.fromLocaleString(Qt.locale(), inv.endDate, "yyyy-MM-dd")
            const sd = s.toLocaleDateString(Qt.locale(), Locale.ShortFormat)
            // The end is exclusive; show the last day, and only for a longer event.
            const last = isNaN(x.getTime()) ? s : new Date(x.getFullYear(), x.getMonth(), x.getDate() - 1)
            return last.getTime() > s.getTime()
                ? qsTr("%1 to %2").arg(sd).arg(last.toLocaleDateString(Qt.locale(), Locale.ShortFormat))
                : sd
        }
        const s = inv.floating ? wall(inv.startDate) : new Date(inv.start * 1000)
        const e = inv.floating ? wall(inv.endDate) : (inv.end > 0 ? new Date(inv.end * 1000) : null)
        if (s === null) return ""
        const start = s.toLocaleString(Qt.locale(), Locale.ShortFormat)
        if (e === null || e.getTime() <= s.getTime()) return start
        const end = e.toDateString() === s.toDateString()
            ? e.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)
            : e.toLocaleString(Qt.locale(), Locale.ShortFormat)
        return qsTr("%1 to %2").arg(start).arg(end)
    }
    readonly property string whenText: {
        if (invite === null) return ""
        const t = dateTimes(invite)
        return invite.repeats ? qsTr("%1 · Repeats").arg(t) : t
    }
    // The name can say anything, so the address an answer goes to is shown
    // beside it (the review 729 fix).
    readonly property string organizerText: {
        if (invite === null) return ""
        const email = invite.organizerEmail || ""
        if (email === "" || email.toLowerCase() === invite.organizer.toLowerCase()) return invite.organizer
        return qsTr("%1 · %2").arg(invite.organizer).arg(email)
    }
    readonly property var facts: invite === null ? [] : [
        { name: "inviteWhen", icon: "clock", text: whenText },
        { name: "inviteLocation", icon: "pin", text: invite.location },
        { name: "inviteOrganizer", icon: "person", text: organizerText }
    ].filter(function(f) { return f.text !== "" })

    function answer(which) {
        core.answerInvite(view.id, which, note.text)
        note.text = ""
        noteOpen = false
        changing = false
    }
    // The same card re-emitted (images loaded, say) keeps the Change view and
    // the note; a new answer, state or message starts afresh.
    readonly property string cardKey: view === null || invite === null ? ""
        : view.id + "|" + invite.state + "|" + invite.answer
    onCardKeyChanged: changing = false
    readonly property var messageId: view !== null ? view.id : -1
    onMessageIdChanged: { note.text = ""; noteOpen = false }
    readonly property bool calendarShown: hubAvailable && invite !== null && invite.canAddToCalendar
    function addToCalendar() {
        calendarRequested()
        core.exportInvite(view.id, attachmentsDir)
    }
    readonly property bool cancelled: cardState === "cancelled"

    visible: invite !== null
    implicitHeight: column.implicitHeight + theme.gu(3.5)
    radius: theme.cardRadius
    color: theme.surface
    border.width: 1
    border.color: theme.hairline

    Column {
        id: column
        x: root.theme.gu(1.75)
        y: root.theme.gu(1.75)
        width: root.width - root.theme.gu(3.5)
        spacing: root.theme.gu(1)

        Text {
            objectName: "inviteEyebrow"
            textFormat: Text.PlainText
            text: root.eyebrow
            color: root.cardState === "updated" || root.cancelled ? root.theme.accent : root.theme.secondary
            font.family: root.theme.eyebrowFamily
            font.pixelSize: root.theme.eyebrowSize
            font.weight: root.theme.eyebrowWeight
            font.letterSpacing: root.theme.eyebrowSpacing
            font.capitalization: root.theme.eyebrowCaps ? Font.AllUppercase : Font.MixedCase
        }
        Text {
            objectName: "inviteSummary"
            textFormat: Text.PlainText
            width: parent.width
            text: root.invite !== null ? root.invite.summary : ""
            wrapMode: Text.Wrap
            color: root.theme.heading
            font.family: root.theme.fontTitle
            font.pixelSize: 18
            font.weight: Font.DemiBold
            font.strikeout: root.cancelled
        }
        Column {
            width: parent.width
            spacing: root.theme.gu(0.5)
            Repeater {
                model: root.facts
                delegate: Row {
                    id: fact
                    required property var modelData
                    width: parent.width
                    spacing: root.theme.gu(1.25)
                    Icon {
                        theme: root.theme
                        name: fact.modelData.icon
                        size: 18
                        color: root.theme.text
                        y: root.theme.gu(0.25)
                    }
                    Text {
                        objectName: fact.modelData.name
                        textFormat: Text.PlainText
                        width: parent.width - x
                        text: fact.modelData.text
                        wrapMode: Text.Wrap
                        color: root.cancelled ? root.theme.secondary : root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 14
                        font.strikeout: root.cancelled && fact.modelData.name !== "inviteOrganizer"
                    }
                }
            }
            Text {
                objectName: "inviteZoneNote"
                textFormat: Text.PlainText
                visible: root.invite !== null && root.invite.zoneUnknown
                width: parent.width
                text: qsTr("Time zone not recognised")
                wrapMode: Text.Wrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 13
            }
        }

        // The answer (ours or an attendee's), with Change, Sending and the failure.
        Row {
            visible: root.answeredShown || root.replyShown
            spacing: root.theme.gu(1)
            Icon {
                theme: root.theme
                name: "check"
                size: 18
                color: root.theme.text
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                objectName: "inviteState"
                textFormat: Text.PlainText
                text: root.invite !== null ? root.invite.stateText : ""
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }
            InviteLink {
                objectName: "inviteChange"
                theme: root.theme
                visible: root.answeredShown && root.invite !== null && root.invite.canAnswer && !root.invite.sending
                text: qsTr("Change")
                onClicked: root.changing = true
            }
        }
        Text {
            objectName: "inviteSending"
            textFormat: Text.PlainText
            visible: root.answeredShown && root.invite !== null && root.invite.sending
            text: qsTr("Sending…")
            color: root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 13
        }
        Rectangle {
            objectName: "inviteFailedBox"
            visible: root.answeredShown && root.invite !== null && root.invite.sendFailed
            width: parent.width
            height: failedRow.height + root.theme.gu(2.5)
            radius: root.theme.compactControlRadius
            color: Qt.rgba(root.theme.accent.r, root.theme.accent.g, root.theme.accent.b, 0.12)
            Row {
                id: failedRow
                x: root.theme.gu(1.5)
                anchors.verticalCenter: parent.verticalCenter
                spacing: root.theme.gu(1)
                Icon {
                    theme: root.theme
                    name: "alert"
                    size: 18
                    color: root.theme.accent
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    objectName: "inviteFailed"
                    textFormat: Text.PlainText
                    text: qsTr("Not sent, see Outbox")
                    color: root.theme.accent
                    font.family: root.theme.fontUi
                    font.pixelSize: 14
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }

        Row {
            visible: root.buttonsShown
            width: parent.width
            spacing: root.theme.gu(1)
            readonly property real each: (width - 2 * spacing) / 3
            AppButton {
                objectName: "inviteAccept"
                theme: root.theme
                primary: true
                width: parent.each
                text: qsTr("Accept")
                onClicked: root.answer("accepted")
            }
            AppButton {
                objectName: "inviteTentative"
                theme: root.theme
                width: parent.each
                text: qsTr("Maybe")
                onClicked: root.answer("tentative")
            }
            AppButton {
                objectName: "inviteDecline"
                theme: root.theme
                width: parent.each
                text: qsTr("Decline")
                onClicked: root.answer("declined")
            }
        }
        Row {
            visible: root.buttonsShown || root.calendarShown
            spacing: root.theme.gu(1)
            InviteLink {
                objectName: "inviteNoteLink"
                theme: root.theme
                visible: root.buttonsShown
                text: root.noteOpen ? qsTr("Hide note") : qsTr("Add a note")
                onClicked: root.noteOpen = !root.noteOpen
            }
            InviteLink {
                objectName: "inviteCalendar"
                theme: root.theme
                visible: root.calendarShown
                text: qsTr("Add to calendar")
                onClicked: root.addToCalendar()
            }
        }
        Field {
            id: note
            objectName: "inviteNote"
            theme: root.theme
            visible: root.buttonsShown && root.noteOpen
            width: parent.width
            label: qsTr("Note to %1").arg(root.invite !== null ? root.invite.organizer : "")
        }
    }
}
