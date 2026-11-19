import QtQuick 2.15
import "fileUrl.js" as FileUrl

// Files shared into Envio from other apps. The hub's receiver is
// `ut/ShareReceiver.qml`, loaded here only where it exists (not on the
// desktop or in tests). `filesShared` carries the paths of a transfer's
// items; they are in the hub's temporary inbox, so whoever takes them copies
// them at once (the core's attach does).
Item {
    id: root

    // Main turns this on; the tests hand transfers in with `accept`.
    property bool listening: false
    signal filesShared(var paths, var transfer)
    // Every file of the transfer is copied (or failed): its hub items may go.
    signal settled(var transfer)

    // compose id -> { left, transfers }
    property var tracking: ({})
    // `count` attach results (Attached or AttachFailed) are due for `compose`.
    function track(compose, count, transfers) {
        if (count <= 0) { finish(transfers); return }
        const t = tracking
        t[compose] = { left: count, transfers: transfers }
        tracking = t
    }
    // Attached and AttachFailed do not say which attach they answer (the
    // copy may be renamed, a failure has no path), so every attach of the
    // compose is counted: the files the user picks meanwhile add to what is
    // due, and the transfer is finalized only once all of them settled,
    // never early.
    function expect(compose, count) {
        const t = tracking[compose]
        if (t !== undefined) t.left += count
    }
    // The user picked `count` files with the attach picker: `transfer` joins
    // whatever the compose already waits for (or starts the tracking).
    function expectPick(compose, count, transfer) {
        if (!transfer) { expect(compose, count); return }
        if (count <= 0) { finish([transfer]); return }
        const t = tracking[compose]
        if (t === undefined) { track(compose, count, [transfer]); return }
        t.left += count
        t.transfers.push(transfer)
    }
    // The compose ended before anything was attached.
    function cancel(compose) {
        const t = tracking[compose]
        if (t === undefined) return
        delete tracking[compose]
        finish(t.transfers)
    }
    function settle(compose) {
        const t = tracking[compose]
        if (t === undefined) return
        if (--t.left > 0) return
        delete tracking[compose]
        finish(t.transfers)
    }
    function finish(transfers) {
        for (const tr of transfers) {
            if (tr && typeof tr.finalize === "function") tr.finalize()
            root.settled(tr)
        }
    }

    // A transfer: anything with `items`, each with a `url`.
    function accept(transfer) {
        const urls = []
        const items = transfer.items || []
        for (let i = 0; i < items.length; i++)
            urls.push(items[i].url.toString())
        const paths = FileUrl.pathsOfUrls(urls)
        if (paths.length > 0) root.filesShared(paths, transfer)
        else finish([transfer])
    }

    Loader {
        id: receiver
        // Off until told: the hub registers a handler on the bus as soon as it
        // loads, which the tests (whose image has Lomiri.Content) must not do.
        source: root.listening ? "ut/ShareReceiver.qml" : ""
        onStatusChanged: {
            if (status === Loader.Error) {
                console.info("Content Hub is not available here; shares are not received")
                source = ""
            }
        }
    }
    Connections {
        target: receiver.item
        ignoreUnknownSignals: true
        function onReceived(transfer) { root.accept(transfer) }
    }
}
