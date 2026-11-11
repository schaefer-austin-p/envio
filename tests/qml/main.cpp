// The QML test runner: qmltestrunner plus the `Envio` module (a static
// plugin that only a binary linking envio_qml can load). Same options as
// qmltestrunner, e.g. `-input tests/qml`.
//
// Tests also see `corpusDir`, the directory tools/ci/render-corpus.sh filled
// (ENVIO_CORPUS_DIR; empty without it).

#include <QtCore/QObject>
#include <QtQml/QQmlContext>
#include <QtQml/QQmlEngine>
#include <QtQuickTest/quicktest.h>

class Setup : public QObject {
    Q_OBJECT
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        engine->rootContext()->setContextProperty(
            QStringLiteral("corpusDir"), qEnvironmentVariable("ENVIO_CORPUS_DIR"));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(envio, Setup)

#include "main.moc"
