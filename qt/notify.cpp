// org.freedesktop.Notifications over QtDBus for desktops: calls and signal
// hookup, no logic. The Rust side (qt/src/notify/) decides what to send and
// keeps the card id per tag; this puts the arguments into Notify, hands the
// reply (the id, 0 on any failure) back through a function pointer, and
// reports NotificationClosed and ActionInvoked. Rust registers the pointers
// (envio_notify_init), so this file needs no Rust symbols at link time.
// The core calls from its worker thread, which has no event loop: the call
// is queued to the main thread and made there, asynchronously.

#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusPendingCall>
#include <QtDBus/QDBusPendingCallWatcher>
#include <QtDBus/QDBusPendingReply>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>

namespace {

const char kService[] = "org.freedesktop.Notifications";
const char kPath[] = "/org/freedesktop/Notifications";
const char kInterface[] = "org.freedesktop.Notifications";
// A probe never waits longer than this for the bus.
const int kProbeTimeoutMs = 1000;

// A UTF-8 slice, as in qt/src/notify/mod.rs.
struct Str {
    const char* ptr;
    int len;
};

QString text(Str s)
{
    return QString::fromUtf8(s.ptr, s.len);
}

using ReplyFn = void (*)(const char* tag, int tag_len, unsigned int id);
using CardFn = void (*)(unsigned int id);

ReplyFn g_onReply = nullptr;
CardFn g_onClosed = nullptr;
CardFn g_onClicked = nullptr;

// Always answers Rust, so a tag never waits for a reply that won't come.
void reply(const QString& tag, unsigned int id)
{
    if (g_onReply == nullptr) {
        return;
    }
    const QByteArray bytes = tag.toUtf8();
    g_onReply(bytes.constData(), static_cast<int>(bytes.size()), id);
}

class Signals : public QObject {
    Q_OBJECT
public Q_SLOTS:
    void onActionInvoked(uint id, const QString& key)
    {
        if (key == QStringLiteral("default") && g_onClicked != nullptr) {
            g_onClicked(id);
        }
    }
    void onClosed(uint id, uint)
    {
        if (g_onClosed != nullptr) {
            g_onClosed(id);
        }
    }
};

QDBusMessage callBus(const QString& method, const QVariantList& args)
{
    QDBusMessage message = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.DBus"), QStringLiteral("/org/freedesktop/DBus"),
        QStringLiteral("org.freedesktop.DBus"), method);
    message.setArguments(args);
    return QDBusConnection::sessionBus().call(message, QDBus::Block, kProbeTimeoutMs);
}

} // namespace

extern "C" {

// Registered or activatable on the session bus. Any failure: false.
bool envio_bus_has_service(const char* name, int len)
{
    const QString service = QString::fromUtf8(name, len);
    if (!QDBusConnection::sessionBus().isConnected()) {
        return false;
    }
    const QDBusMessage owner = callBus(QStringLiteral("NameHasOwner"), {service});
    if (owner.type() == QDBusMessage::ReplyMessage && !owner.arguments().isEmpty()
        && owner.arguments().first().toBool()) {
        return true;
    }
    const QDBusMessage activatable = callBus(QStringLiteral("ListActivatableNames"), {});
    return activatable.type() == QDBusMessage::ReplyMessage && !activatable.arguments().isEmpty()
        && activatable.arguments().first().toStringList().contains(service);
}

// Whether the server renders body markup. Any failure: false.
bool envio_notify_has_markup()
{
    QDBusMessage message = QDBusMessage::createMethodCall(
        QString::fromLatin1(kService), QString::fromLatin1(kPath),
        QString::fromLatin1(kInterface), QStringLiteral("GetCapabilities"));
    const QDBusMessage answer =
        QDBusConnection::sessionBus().call(message, QDBus::Block, kProbeTimeoutMs);
    return answer.type() == QDBusMessage::ReplyMessage && !answer.arguments().isEmpty()
        && answer.arguments().first().toStringList().contains(QStringLiteral("body-markup"));
}

// Once, on the main thread: the callbacks, and the two signals. The service
// name is given, so Qt follows the server across a restart.
void envio_notify_init(ReplyFn onReply, CardFn onClosed, CardFn onClicked)
{
    g_onReply = onReply;
    g_onClosed = onClosed;
    g_onClicked = onClicked;
    static Signals signalsObject;
    QDBusConnection bus = QDBusConnection::sessionBus();
    const QString service = QString::fromLatin1(kService);
    const QString path = QString::fromLatin1(kPath);
    const QString interface = QString::fromLatin1(kInterface);
    bus.connect(service, path, interface, QStringLiteral("ActionInvoked"), &signalsObject,
                SLOT(onActionInvoked(uint, QString)));
    bus.connect(service, path, interface, QStringLiteral("NotificationClosed"), &signalsObject,
                SLOT(onClosed(uint, uint)));
}

// Notify(app_name, replaces_id, icon, summary, body, actions, hints,
// timeout) = susssasa{sv}i; the reply is the card's id (u).
void envio_notify_send(Str tag, Str appName, Str icon, Str summary, Str body, Str category,
                       unsigned int replacesId)
{
    const QString tagText = text(tag);
    QCoreApplication* app = QCoreApplication::instance();
    if (app == nullptr) {
        // No loop to queue to: answer directly, so the tag is not stuck.
        reply(tagText, 0);
        return;
    }
    const QVariantList args{
        text(appName),
        QVariant::fromValue<quint32>(replacesId),
        text(icon),
        text(summary),
        text(body),
        QStringList{QStringLiteral("default"), QString()},
        QVariantMap{{QStringLiteral("category"), text(category)}},
        QVariant::fromValue<qint32>(-1),
    };
    QMetaObject::invokeMethod(
        app,
        [tagText, args]() {
            QDBusConnection bus = QDBusConnection::sessionBus();
            if (!bus.isConnected()) {
                reply(tagText, 0);
                return;
            }
            QDBusMessage message = QDBusMessage::createMethodCall(
                QString::fromLatin1(kService), QString::fromLatin1(kPath),
                QString::fromLatin1(kInterface), QStringLiteral("Notify"));
            message.setArguments(args);
            const QDBusPendingCall call = bus.asyncCall(message);
            if (call.isError()) {
                reply(tagText, 0);
                return;
            }
            auto* watcher = new QDBusPendingCallWatcher(call);
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, watcher,
                             [watcher, tagText]() {
                                 const QDBusPendingReply<quint32> answer = *watcher;
                                 if (answer.isError()) {
                                     qWarning().noquote()
                                         << "envio: Notify failed:" << answer.error().message();
                                     reply(tagText, 0);
                                 } else {
                                     reply(tagText, answer.value());
                                 }
                                 watcher->deleteLater();
                             });
        },
        Qt::QueuedConnection);
}

// Best effort: Wayland may refuse to raise a window without a token.
void envio_activate_window()
{
    const QWindowList windows = QGuiApplication::allWindows();
    if (windows.isEmpty()) {
        return;
    }
    QWindow* window = windows.first();
    window->showNormal();
    window->raise();
    window->requestActivate();
}

} // extern "C"

#include "notify.moc"
