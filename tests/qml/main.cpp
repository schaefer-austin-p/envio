// The QML test runner: qmltestrunner plus the `Envio` module (a static
// plugin that only a binary linking envio_qml can load). Same options as
// qmltestrunner, e.g. `-input tests/qml`.
//
// Tests also see:
//  - `corpusDir`, the directory tools/ci/render-corpus.sh filled
//    (ENVIO_CORPUS_DIR; empty without it);
//  - `shotsDir`, where a test may save screenshots to compare with the
//    design boards (ENVIO_SHOTS_DIR; empty without it, then none are saved);
//  - `fixtures`, the app's saved settings file under XDG_DATA_HOME, which a
//    test writes before it makes a core and removes in `cleanup`, so the
//    other tests see no file: `writeSettings(text)`, `removeSettings()`,
//    `exists(name)`;
//  - `netRequests`, which counts every request the QML engine's network
//    access manager is asked to make (rich Text and Image fetching remote
//    images go through it). Nothing leaves the machine: each request is
//    swapped for an empty `data:` URL before it is sent.
//    `netRequests.count()` and `netRequests.reset()`.

#include <QtCore/QAtomicInt>
#include <QtCore/QDir>
#include <QtCore/QFile>
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

// The data folder the Rust core uses: <XDG_DATA_HOME>/<app id> (run.sh sets
// XDG_DATA_HOME to a temporary tree).
class Fixtures : public QObject {
    Q_OBJECT
public:
    Q_INVOKABLE void writeSettings(const QString &text) const {
        QDir().mkpath(dir());
        QFile file(dir() + QStringLiteral("/settings.json"));
        if (file.open(QIODevice::WriteOnly | QIODevice::Truncate))
            file.write(text.toUtf8());
    }
    // The file and any copy the core set aside as damaged.
    Q_INVOKABLE void removeSettings() const {
        QDir folder(dir());
        for (const QString &name : folder.entryList({QStringLiteral("settings.json*")}, QDir::Files))
            folder.remove(name);
    }
    Q_INVOKABLE bool exists(const QString &name) const { return QFile::exists(dir() + QLatin1Char('/') + name); }

private:
    static QString dir() {
        const QString home = qEnvironmentVariable("XDG_DATA_HOME");
        return (home.isEmpty() ? QDir::homePath() + QStringLiteral("/.local/share") : home)
            + QStringLiteral("/envio.aps");
    }
};

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() : factory(&requests) {}

public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        engine->rootContext()->setContextProperty(
            QStringLiteral("corpusDir"), qEnvironmentVariable("ENVIO_CORPUS_DIR"));
        engine->rootContext()->setContextProperty(
            QStringLiteral("shotsDir"), qEnvironmentVariable("ENVIO_SHOTS_DIR"));
        engine->rootContext()->setContextProperty(QStringLiteral("fixtures"), &fixtures);
        engine->rootContext()->setContextProperty(QStringLiteral("netRequests"), &requests);
        engine->setNetworkAccessManagerFactory(&factory);
    }

private:
    NetRequests requests;
    Fixtures fixtures;
    CountingFactory factory;
};

QUICK_TEST_MAIN_WITH_SETUP(envio, Setup)

#include "main.moc"
