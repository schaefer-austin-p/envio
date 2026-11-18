// Lomiri's Postal over QtDBus: two calls, no logic. The Rust side
// (qt/src/postal.rs) decides what to post; this only sends it. The core
// calls from its worker thread, which has no event loop, so the call is
// queued to the main thread and made there, asynchronously: the worker
// never waits on the bus, and a failed call is logged, never raised.

#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtCore/QMetaObject>
#include <QtCore/QString>
#include <QtCore/QVariantList>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusError>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusPendingCall>
#include <QtDBus/QDBusPendingCallWatcher>
#include <QtDBus/QDBusPendingReply>

namespace {

// Postal's name for the app: in the path the app id with its dots escaped
// (`envio.aps` -> `envio_2eaps`), in the argument package_app.
const char kService[] = "com.lomiri.Postal";
const char kPath[] = "/com/lomiri/Postal/envio_2eaps";
const char kInterface[] = "com.lomiri.Postal";
const char kApp[] = "envio.aps_envio";

void call(const QString& method, const QVariantList& args)
{
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
        return;
    }
    QMetaObject::invokeMethod(
        app,
        [method, args]() {
            QDBusMessage message = QDBusMessage::createMethodCall(
                QString::fromLatin1(kService), QString::fromLatin1(kPath),
                QString::fromLatin1(kInterface), method);
            message.setArguments(args);
            auto* watcher =
                new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message));
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, watcher, [watcher, method]() {
                const QDBusPendingReply<> reply = *watcher;
                if (reply.isError()) {
                    // No Postal on this system (desktop, dev): say so once.
                    static bool missingLogged = false;
                    if (reply.error().type() == QDBusError::ServiceUnknown) {
                        if (missingLogged) {
                            watcher->deleteLater();
                            return;
                        }
                        missingLogged = true;
                    }
                    qWarning().noquote() << "envio: Postal" << method << "failed:"
                                         << reply.error().message();
                }
                watcher->deleteLater();
            });
        },
        Qt::QueuedConnection);
}

} // namespace

extern "C" {

// `json` is UTF-8, `len` bytes, not necessarily NUL-terminated.
void envio_postal_post(const char* json, int len)
{
    call(QStringLiteral("Post"),
         {QString::fromLatin1(kApp), QString::fromUtf8(json, len)});
}

void envio_postal_set_counter(int count)
{
    call(QStringLiteral("SetCounter"), {QString::fromLatin1(kApp), count, count > 0});
}

} // extern "C"
