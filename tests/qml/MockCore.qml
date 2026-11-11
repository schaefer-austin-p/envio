import QtQuick 2.15

// A stand-in for EnvioCore with the same properties, signals and
// invokables (names, argument order and types, see qt/src/core_object.rs).
// Every invokable records `{ name, args }` in `calls`; the tests emit the
// signals themselves. The models are ListModels.
Item {
    id: root

    property bool ready: false
    property string theme: "carbon"
    property string darkMode: "system"
    property bool imagesAlways: false
    property string defaultAccount: ""

    // What the page called, in order.
    property var calls: []
    property string note: "Only the part of your address after the @ is sent."
    // The model roles are those of envio-ui's rows (crates/envio-ui/src/rows.rs).
    // Tests fill them; the page gets them as properties, as Main hands over
    // the RowsModels it attached.
    property ListModel accounts: ListModel {}
    property ListModel folders: ListModel {}
    property ListModel messages: ListModel {}
    property ListModel outbox: ListModel {}
    property ListModel suggestions: ListModel {}
    // The models handed to attachModel.
    property var models: []

    function record(name, args) { calls.push({ name: name, args: args }) }
    function called(name) { return calls.filter(function(c) { return c.name === name }) }
    function reset() { calls = [] }

    signal notice(string text)
    signal storeProblem(string text)
    signal workerRestarted(string text)
    signal detected(string email, string text, bool needsConfirm, string setupJson)
    signal setupProblem(string email, string text)
    signal trustPrompt(string email, string service, string host, string fingerprint, string hex, string text)
    signal accountAdded(string account, string email)
    signal confirmRemove(string account, string text)
    signal messageOpened(var view)
    signal messageUnavailable(real id, string text)
    signal attachmentSaved(string path)
    signal attachmentFailed()
    signal undoOffer(real undo, string text)
    signal undoClosed()
    signal composeReady(real compose, var draft, string quoted)
    signal draftSaved(real compose)
    signal composeRefused(real compose, string text)
    signal queued(real compose)
    signal attached(real compose, string path, string name, real size)
    signal attachFailed(real compose, string text)
    signal searchFailed(string text)
    signal statusText(string account, string text)
    signal startFailed(string text)

    function start() { record("start", []) }
    function stop() { record("stop", []) }
    function setActive(active) { record("setActive", [active]) }
    function refresh(account) { record("refresh", [account]) }
    function ispdbNote() { record("ispdbNote", []); return note }
    function detect(email) { record("detect", [email]) }
    function connect(setupJson, password, serversConfirmed) {
        record("connect", [setupJson, password, serversConfirmed])
    }
    function acceptSetupCertificate(service, hex) { record("acceptSetupCertificate", [service, hex]) }
    function acceptCertificate(account, service, hex) { record("acceptCertificate", [account, service, hex]) }
    function updatePassword(account, password) { record("updatePassword", [account, password]) }
    function removeAccount(account, confirmed) { record("removeAccount", [account, confirmed]) }
    function listFolders(account) { record("listFolders", [account]) }
    function openList(kind, id) { record("openList", [kind, id]) }
    function more() { record("more", []) }
    function select(id, on) { record("select", [id, on]) }
    function clearSelection() { record("clearSelection", []) }
    function act(action, folder) { record("act", [action, folder]) }
    function undo(id) { record("undo", [id]) }
    function open(id, loadImages, widthPx) { record("open", [id, loadImages, widthPx]) }
    function allowImagesFrom(address) { record("allowImagesFrom", [address]) }
    function saveAttachment(id, index, dir) { record("saveAttachment", [id, index, dir]) }
    function startCompose(kind, id, account, mailto) { record("startCompose", [kind, id, account, mailto]); return 1 }
    function draftChanged(compose, draftJson) { record("draftChanged", [compose, draftJson]) }
    function closeCompose(compose, discard) { record("closeCompose", [compose, discard]) }
    function send(compose, draftJson) { record("send", [compose, draftJson]) }
    function attach(compose, path) { record("attach", [compose, path]) }
    function listOutbox() { record("listOutbox", []) }
    function decideOutbox(item, decision) { record("decideOutbox", [item, decision]) }
    function editOutbox(item) { record("editOutbox", [item]); return 1 }
    function search(scope, query, server) { record("search", [scope, query, server]) }
    function autocomplete(query) { record("autocomplete", [query]) }
    function addContact(name, address) { record("addContact", [name, address]) }
    function saveSettings(theme, darkMode, imagesAlways, defaultAccount) {
        record("saveSettings", [theme, darkMode, imagesAlways, defaultAccount])
    }
    function countText(kind, n) { return kind + " " + n }
    function dateBucket(unixSecs) { return "time" }
    function attachModel(model) { models.push(model) }
}
