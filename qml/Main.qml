import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Window 2.15
import Envio 1.0
import "platform"
import "pages"

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    visibility: windowMode.visibility
    title: "Envio"
    color: appTheme.ground

    EnvioCore { id: core }
    // Fullscreen on the phone, hiding the indicator panel (#383).
    WindowMode { id: windowMode; fullscreen: core.fullscreen }
    RowsModel { id: accountsModel; kind: "accounts" }
    RowsModel { id: foldersModel; kind: "folders" }
    RowsModel { id: messagesModel; kind: "messages" }
    RowsModel { id: outboxModel; kind: "outbox" }
    RowsModel { id: suggestionsModel; kind: "suggestions" }
    RowsModel { id: searchModel; kind: "search" }

    // The settings, once they arrive, pick the look; until then the
    // defaults.
    Theme {
        id: appTheme
        name: core.theme !== "" ? core.theme : "carbon"
        darkMode: core.darkMode !== "" ? core.darkMode : "system"
        roundedDisplay: displayCorners.rounded
    }
    DisplayCorners {
        id: displayCorners
        windowWidth: window.width
        windowHeight: window.height
        screenWidth: Screen.width
        screenHeight: Screen.height
        panelAllowance: appTheme.gu(6)
    }

    AppShell {
        anchors.fill: parent
        theme: appTheme
        core: core
        accounts: accountsModel
        folders: foldersModel
        messages: messagesModel
        outbox: outboxModel
        suggestions: suggestionsModel
        searchResults: searchModel
        receiveShares: true
        fullscreenAvailable: windowMode.available
    }

    Component.onCompleted: {
        // "envio 0.1.0 (master@0123abcdef built 2026-…Z installed 2026-…Z)";
        // an official release logs only the version (#198).
        const iso = function(ms) { return ms > 0 ? new Date(ms).toISOString() : "unknown" }
        console.info("envio " + BuildInfo.version + (BuildInfo.dev
            ? " (" + BuildInfo.source + " built " + iso(BuildInfo.built)
              + " installed " + iso(BuildInfo.installed) + ")" : ""))
        // Models first: attachModel brings each up to date from what the
        // core already sent.
        for (const model of [accountsModel, foldersModel, messagesModel, outboxModel, suggestionsModel, searchModel]) core.attachModel(model)
        core.start()
    }
}
