import QtQuick 2.15
import QtQuick.Controls 2.15
import Envio 1.0
import "platform"
import "pages"

ApplicationWindow {
    id: window
    width: 360
    height: 640
    visible: true
    title: "Envio"
    color: appTheme.ground

    EnvioCore { id: core }
    RowsModel { id: accountsModel; kind: "accounts" }
    RowsModel { id: foldersModel; kind: "folders" }
    RowsModel { id: messagesModel; kind: "messages" }
    RowsModel { id: outboxModel; kind: "outbox" }

    // The settings, once they arrive, pick the look; until then the
    // defaults.
    Theme {
        id: appTheme
        name: core.theme !== "" ? core.theme : "carbon"
        darkMode: core.darkMode !== "" ? core.darkMode : "system"
    }

    AppShell {
        anchors.fill: parent
        theme: appTheme
        core: core
        accounts: accountsModel
        folders: foldersModel
        messages: messagesModel
        outbox: outboxModel
    }

    Component.onCompleted: {
        console.info("envio " + BuildInfo.version + " (" + BuildInfo.commit + ")")
        // Models first: attachModel brings each up to date from what the
        // core already sent.
        for (const model of [accountsModel, foldersModel, messagesModel, outboxModel]) core.attachModel(model)
        core.start()
    }
}
