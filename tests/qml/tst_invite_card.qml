import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/reader"

// The meeting invite card (Plan 10, B2) over the `invite` map of the opened
// message (the card contract in the plan). Synthetic data only.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: mock }
    property var card: null

    Component {
        id: cardComponent
        InviteCard {
            theme: thm
            core: mock
            width: 360
            hubAvailable: true
            attachmentsDir: "/tmp/envio-test/attachments"
        }
    }

    TestCase {
        name: "InviteCard"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            for (let i = 0; i < item.children.length; i++) everything(item.children[i], out)
            return out
        }
        function find(name) {
            const hit = everything(root.card, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function shown(name) {
            const hit = everything(root.card, []).filter(function(o) { return o.objectName === name })
            return hit.length > 0 && hit[0].visible
        }
        // 2026-11-20 10:00 to 11:00 UTC.
        function invite(over) {
            const v = { method: "request", state: "open", stateText: "", answer: "", sending: false,
                        sendFailed: false, summary: "Quarterly planning", location: "Room 4, Example Office",
                        allDay: false, floating: false, start: 1795168800, end: 1795172400,
                        startDate: "", endDate: "", zoneUnknown: false, repeats: false,
                        organizer: "Grace Example", canAnswer: true, icsIndex: 0 }
            for (const k in over) v[k] = over[k]
            return v
        }
        // Layout runs on the next frame; clicks need the settled positions.
        function settle() { waitForRendering(root.card) }
        function show(over) {
            root.card.view = { id: 7, invite: invite(over) }
            settle()
        }
        function init() {
            mock.reset()
            root.card = cardComponent.createObject(root, { view: { id: 7 } })
        }
        function cleanup() {
            root.card.destroy()
            root.card = null
        }

        function test_open_request_shows_three_buttons() {
            show({})
            compare(find("inviteEyebrow").text, "Invitation")
            compare(find("inviteSummary").text, "Quarterly planning")
            compare(find("inviteLocation").text, "Room 4, Example Office")
            compare(find("inviteOrganizer").text, "Grace Example")
            verify(shown("inviteAccept"))
            verify(shown("inviteTentative"))
            verify(shown("inviteDecline"))
            verify(!shown("inviteState"))
            const s = new Date(1795168800 * 1000)
            const e = new Date(1795172400 * 1000)
            verify(find("inviteWhen").text.indexOf(s.toLocaleString(Qt.locale(), Locale.ShortFormat)) === 0)
            verify(find("inviteWhen").text.indexOf(e.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)) > 0)
        }

        function test_accept_calls_core_with_note() {
            show({})
            mouseClick(find("inviteNoteLink"))
            verify(shown("inviteNote"))
            find("inviteNote").text = "Running five minutes late"
            settle()
            mouseClick(find("inviteAccept"))
            const calls = mock.called("answerInvite")
            compare(calls.length, 1)
            compare(calls[0].args, [7, "accepted", "Running five minutes late"])
            compare(find("inviteNote").text, "")
            settle()
            mouseClick(find("inviteTentative"))
            mouseClick(find("inviteDecline"))
            compare(mock.called("answerInvite").map(function(c) { return c.args[1] }),
                    ["accepted", "tentative", "declined"])
        }

        function test_answered_shows_state_text_and_change() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted" })
            compare(find("inviteState").text, "You accepted")
            verify(shown("inviteChange"))
            verify(!shown("inviteAccept"))
            verify(!shown("inviteDecline"))
        }

        function test_change_shows_buttons_again() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted" })
            mouseClick(find("inviteChange"))
            settle()
            verify(shown("inviteAccept"))
            verify(shown("inviteTentative"))
            verify(shown("inviteDecline"))
            verify(!shown("inviteChange"))
        }

        function test_updated_shows_buttons_again() {
            show({ state: "updated", stateText: "Updated" })
            compare(find("inviteEyebrow").text, "Updated")
            verify(shown("inviteAccept"))
            verify(!shown("inviteState"))
        }

        function test_cancelled_has_no_buttons() {
            show({ method: "cancel", state: "cancelled", stateText: "Cancelled", canAnswer: false })
            compare(find("inviteEyebrow").text, "Cancelled")
            verify(!shown("inviteAccept"))
            verify(!shown("inviteNoteLink"))
            verify(!shown("inviteChange"))
            verify(find("inviteSummary").font.strikeout)
        }

        function test_reply_has_no_buttons() {
            show({ method: "reply", state: "replied", stateText: "Grace accepted", answer: "accepted", canAnswer: false })
            compare(find("inviteEyebrow").text, "Reply")
            compare(find("inviteState").text, "Grace accepted")
            verify(!shown("inviteAccept"))
            verify(!shown("inviteChange"))
            verify(!find("inviteSummary").font.strikeout)
        }

        function test_all_day_shows_dates_only() {
            show({ allDay: true, start: 0, end: 0, startDate: "2026-11-20", endDate: "2026-11-21" })
            const day = Date.fromLocaleString(Qt.locale(), "2026-11-20", "yyyy-MM-dd")
            compare(find("inviteWhen").text, day.toLocaleDateString(Qt.locale(), Locale.ShortFormat))
            // Several days: the end is shown as the last day, not the exclusive one.
            show({ allDay: true, start: 0, end: 0, startDate: "2026-11-20", endDate: "2026-11-23" })
            const last = Date.fromLocaleString(Qt.locale(), "2026-11-22", "yyyy-MM-dd")
            const t = find("inviteWhen").text
            verify(t.indexOf(day.toLocaleDateString(Qt.locale(), Locale.ShortFormat)) === 0)
            verify(t.indexOf(last.toLocaleDateString(Qt.locale(), Locale.ShortFormat)) > 0)
        }

        function test_floating_time_shows_as_local() {
            show({ floating: true, start: 0, end: 0, startDate: "2026-11-20T10:00:00", endDate: "2026-11-20T11:30:00" })
            const s = new Date(2026, 10, 20, 10, 0)
            const e = new Date(2026, 10, 20, 11, 30)
            const t = find("inviteWhen").text
            verify(t.indexOf(s.toLocaleString(Qt.locale(), Locale.ShortFormat)) === 0)
            verify(t.indexOf(e.toLocaleTimeString(Qt.locale(), Locale.ShortFormat)) > 0)
        }

        function test_zone_unknown_and_repeats_notes() {
            show({ zoneUnknown: true, repeats: true })
            verify(shown("inviteZoneNote"))
            verify(find("inviteWhen").text.indexOf("Repeats") > 0)
            show({})
            verify(!shown("inviteZoneNote"))
        }

        function test_sending_shows_sending() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted", sending: true })
            verify(shown("inviteSending"))
            verify(!shown("inviteFailed"))
        }

        function test_send_failed_shows_note() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted", sendFailed: true })
            compare(find("inviteFailed").text, "Not sent, see Outbox")
            verify(shown("inviteChange"))
            verify(!shown("inviteSending"))
        }

        function test_no_change_while_sending() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted", sending: true })
            verify(!shown("inviteChange"))
        }

        function test_same_card_again_keeps_the_change_view() {
            const answered = { state: "answered", stateText: "You accepted", answer: "accepted" }
            show(answered)
            mouseClick(find("inviteChange"))
            settle()
            verify(shown("inviteAccept"))
            // The core re-emits the message (images loaded): the buttons stay.
            show(answered)
            verify(shown("inviteAccept"))
            // A different answer or state is a new card.
            show({ state: "answered", stateText: "You declined", answer: "declined" })
            verify(!shown("inviteAccept"))
            verify(shown("inviteChange"))
        }

        function test_cancelled_strikes_the_facts() {
            show({ method: "cancel", state: "cancelled", stateText: "Cancelled", canAnswer: false })
            verify(find("inviteWhen").font.strikeout)
            verify(find("inviteLocation").font.strikeout)
            verify(!find("inviteOrganizer").font.strikeout)
            show({})
            verify(!find("inviteWhen").font.strikeout)
        }

        function test_organizer_shows_the_address_an_answer_goes_to() {
            show({ organizer: "Grace Example", organizerEmail: "x@evil.example" })
            compare(find("inviteOrganizer").text, "Grace Example · x@evil.example")
            // A name that is the address is not shown twice.
            show({ organizer: "grace@example.org", organizerEmail: "Grace@Example.org" })
            compare(find("inviteOrganizer").text, "grace@example.org")
            show({ organizer: "Grace Example", organizerEmail: "" })
            compare(find("inviteOrganizer").text, "Grace Example")
        }

        function test_send_failed_is_in_a_box() {
            show({ state: "answered", stateText: "You accepted", answer: "accepted", sendFailed: true })
            verify(shown("inviteFailedBox"))
            show({})
            verify(!shown("inviteFailedBox"))
        }

        function test_add_to_calendar_exports_the_invite() {
            show({ icsIndex: 2 })
            const asked = []
            root.card.calendarRequested.connect(function() { asked.push(true) })
            mouseClick(find("inviteCalendar"))
            compare(mock.called("exportInvite").map(function(c) { return c.args }),
                    [[7, "/tmp/envio-test/attachments"]])
            compare(mock.called("saveAttachment").length, 0)
            compare(asked.length, 1)
        }

        // An inline-only calendar part (no attachment index) exports too.
        function test_add_to_calendar_is_shown_for_an_inline_part_and_hidden_without_a_hub() {
            show({ icsIndex: -1 })
            verify(shown("inviteCalendar"))
            root.card.hubAvailable = false
            verify(!shown("inviteCalendar"))
            root.card.hubAvailable = true
            show({ method: "cancel", state: "cancelled", stateText: "Cancelled", canAnswer: false })
            verify(!shown("inviteCalendar"))
            show({ state: "answered", stateText: "You accepted", answer: "accepted" })
            verify(shown("inviteCalendar"))
        }

        function test_cannot_answer_has_no_buttons() {
            show({ canAnswer: false })
            verify(!shown("inviteAccept"))
        }

        function test_no_invite_hides_card() {
            root.card.view = { id: 7 }
            verify(!root.card.visible)
            show({})
            verify(root.card.visible)
        }
    }
}
