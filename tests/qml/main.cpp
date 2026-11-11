// The QML test runner: qmltestrunner plus the `Envio` module (a static
// plugin that only a binary linking envio_qml can load). Same options as
// qmltestrunner, e.g. `-input tests/qml`.
//
// Tests also see:
//  - `corpusDir`, the directory tools/ci/render-corpus.sh filled
//    (ENVIO_CORPUS_DIR; empty without it);
//  - `netRequests`, which counts every request the QML engine's network
//    access manager is asked to make (rich Text and Image fetching remote
//    images go through it). Nothing leaves the machine: each request is
//    swapped for an empty `data:` URL before it is sent.
//    `netRequests.count()` and `netRequests.reset()`.

#include <QtCore/QAtomicInt>
#include <QtCore/QObject>
#include <QtCore/QUrl>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlContext>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#include <QtQuickTest/quicktest.h>

class NetRequests : public QObject {
    Q_OBJECT
public:
    QAtomicInt seen;
    Q_INVOKABLE int count() const { return seen.loadRelaxed(); }
    Q_INVOKABLE void reset() { seen.storeRelaxed(0); }
};

class CountingManager : public QNetworkAccessManager {
public:
    explicit CountingManager(NetRequests *counter) : counter(counter) {}

protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest &request,
                                 QIODevice *body) override {
        // Local files (the tests read the corpus with XMLHttpRequest) and
        // data: URLs are not the network.
        const QString scheme = request.url().scheme();
        if (scheme == QLatin1String("file") || scheme == QLatin1String("qrc")
            || scheme == QLatin1String("data"))
            return QNetworkAccessManager::createRequest(op, request, body);
        counter->seen.fetchAndAddRelaxed(1);
        QNetworkRequest local(QUrl(QStringLiteral("data:,")));
        return QNetworkAccessManager::createRequest(GetOperation, local, nullptr);
    }

private:
    NetRequests *counter;
};

class CountingFactory : public QQmlNetworkAccessManagerFactory {
public:
    explicit CountingFactory(NetRequests *counter) : counter(counter) {}
    QNetworkAccessManager *create(QObject *parent) override {
        auto *manager = new CountingManager(counter);
        manager->setParent(parent);
        return manager;
    }

private:
    NetRequests *counter;
};

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() : factory(&requests) {}

public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        engine->rootContext()->setContextProperty(
            QStringLiteral("corpusDir"), qEnvironmentVariable("ENVIO_CORPUS_DIR"));
        engine->rootContext()->setContextProperty(QStringLiteral("netRequests"), &requests);
        engine->setNetworkAccessManagerFactory(&factory);
    }

private:
    NetRequests requests;
    CountingFactory factory;
};

QUICK_TEST_MAIN_WITH_SETUP(envio, Setup)

#include "main.moc"
