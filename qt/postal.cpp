// Lomiri's Postal over QtDBus: three calls, no logic. The Rust side
// (qt/src/postal.rs) decides what to post; this only sends it. A local
// Post does not replace an earlier card with the same tag (found on the
// phone, Plan 9a Task 8); ClearPersistent(app, tag) does remove it. So a
// post first clears its tag and sends the Post from the clear's finished
// handler, whether the clear worked or not: the clear can't land after the
// new card, and a failed clear still shows the card. The core
// calls from its worker thread, which has no event loop, so the call is
// queued to the main thread and made there, asynchronously: the worker
// never waits on the bus, and a failed call is logged, never raised.

#include <functional>

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

// `then`, if given, runs on the main thread once the call has finished, with
// or without an error.
void call(const QString& method, const QVariantList& args,
          std::function<void()> then = nullptr)
{
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
        return;
    }
    QMetaObject::invokeMethod(
        app,
        [method, args, then]() {
            QDBusMessage message = QDBusMessage::createMethodCall(
                QString::fromLatin1(kService), QString::fromLatin1(kPath),
                QString::fromLatin1(kInterface), method);
            message.setArguments(args);
            auto* watcher =
                new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message));
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, watcher, [watcher, method, then]() {
                const QDBusPendingReply<> reply = *watcher;
                if (reply.isError()) {
                    // No Postal on this system (desktop, dev): say so once.
                    static bool missingLogged = false;
                    if (reply.error().type() == QDBusError::ServiceUnknown) {
                        if (!missingLogged) {
                            missingLogged = true;
                            qWarning().noquote() << "envio: Postal" << method << "failed:"
                                                 << reply.error().message();
                        }
                    } else {
                        qWarning().noquote() << "envio: Postal" << method << "failed:"
                                             << reply.error().message();
                    }
                }
                watcher->deleteLater();
                if (then) {
                    then();
                }
            });
        },
        Qt::QueuedConnection);
}

} // namespace

extern "C" {

// `json` and `tag` are UTF-8 with their byte lengths, not necessarily
// NUL-terminated. ClearPersistent takes the app id and then the tags as
// separate string arguments (checked on the phone).
void envio_postal_post(const char* json, int len, const char* tag, int tag_len)
{
    const QString app = QString::fromLatin1(kApp);
    const QString card = QString::fromUtf8(json, len);
    const QString tagText = QString::fromUtf8(tag, tag_len);
    call(QStringLiteral("ClearPersistent"), {app, tagText},
         [app, card]() { call(QStringLiteral("Post"), {app, card}); });
}

void envio_postal_set_counter(int count)
{
    call(QStringLiteral("SetCounter"), {QString::fromLatin1(kApp), count, count > 0});
}

} // extern "C"
