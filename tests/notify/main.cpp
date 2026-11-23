// Test of the D-Bus glue in qt/notify.cpp (#442), against a fake
// org.freedesktop.Notifications on a private session bus. tests/notify/run.sh
// starts that bus (dbus-run-session), so DBUS_SESSION_BUS_ADDRESS is the
// private one the glue's sessionBus() talks to. The fake is a second
// connection to the same bus, in this process. No Rust is linked: the
// callbacks the Rust side registers are recorded here instead.
//
// Checks: Notify's signature, typed arguments and reply; the async
// GetCapabilities answer (body-markup, none, error); and that ActionInvoked,
// ActivationToken, NotificationClosed and an owner change of the service
// reach the callbacks.

#include <QtCore/QCoreApplication>
#include <QtCore/QDebug>
#include <QtCore/QElapsedTimer>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusArgument>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusConnectionInterface>
#include <QtDBus/QDBusError>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusVirtualObject>

#include <tuple>
#include <utility>
#include <vector>

namespace {

struct Str {
    const char* ptr;
    int len;
};
// Mirrors `Card` in qt/notify.cpp.
struct Card {
    Str tag, appName, icon, summary, body, category, desktopEntry, imagePath;
    unsigned int replacesId;
    unsigned int generation;
    unsigned char urgency;
};

Str str(const char* s)
{
    return Str{s, static_cast<int>(qstrlen(s))};
}

using ReplyFn = void (*)(const char*, int, unsigned int, unsigned int);
using CardFn = void (*)(unsigned int);
using TokenFn = void (*)(unsigned int, const char*, int);
using CapsFn = void (*)(int, unsigned int);
using OwnerFn = void (*)(bool);

} // namespace

extern "C" {
bool envio_bus_has_service(const char* name, int len);
void envio_notify_init(ReplyFn, CardFn, CardFn, TokenFn, CapsFn, OwnerFn);
void envio_notify_query_caps(unsigned int generation);
void envio_notify_send(const Card* card);
}

namespace {

const char kService[] = "org.freedesktop.Notifications";
const char kPath[] = "/org/freedesktop/Notifications";
const int kServiceLen = static_cast<int>(sizeof kService - 1);

int g_failures = 0;

void check(bool ok, const char* what)
{
    if (!ok) {
        ++g_failures;
    }
    qInfo("%s: %s", ok ? "ok  " : "FAIL", what);
}

// What the glue reported to "Rust".
struct Seen {
    std::vector<std::tuple<QString, unsigned int, unsigned int>> replies;
    std::vector<unsigned int> closed, clicked;
    std::vector<std::pair<unsigned int, QString>> tokens;
    std::vector<std::pair<int, unsigned int>> caps;
    std::vector<bool> owner;
} g_seen;

void onReply(const char* tag, int len, unsigned int id, unsigned int generation)
{
    g_seen.replies.emplace_back(QString::fromUtf8(tag, len), id, generation);
}
void onClosed(unsigned int id) { g_seen.closed.push_back(id); }
void onClicked(unsigned int id) { g_seen.clicked.push_back(id); }
void onToken(unsigned int id, const char* t, int len)
{
    g_seen.tokens.emplace_back(id, QString::fromUtf8(t, len));
}
void onCaps(int answer, unsigned int generation) { g_seen.caps.emplace_back(answer, generation); }
void onOwner(bool hadOwner) { g_seen.owner.push_back(hadOwner); }

// The fake server: records Notify, answers GetCapabilities and Notify.
class Fake : public QDBusVirtualObject {
public:
    QStringList caps{QStringLiteral("body"), QStringLiteral("body-markup")};
    bool fail = false;
    unsigned int nextId = 42;
    std::vector<QDBusMessage> notifies;

    QString introspect(const QString&) const override { return QString(); }
    bool handleMessage(const QDBusMessage& message, const QDBusConnection& connection) override
    {
        QDBusMessage out;
        if (message.member() != QStringLiteral("GetCapabilities")
            && message.member() != QStringLiteral("Notify")) {
            return false;
        }
        if (message.member() == QStringLiteral("Notify")) {
            notifies.push_back(message);
        }
        if (fail) {
            out = message.createErrorReply(QDBusError::Failed, QStringLiteral("fake failure"));
        } else if (message.member() == QStringLiteral("GetCapabilities")) {
            out = message.createReply(QVariant(caps));
        } else {
            out = message.createReply(QVariant::fromValue<quint32>(nextId));
        }
        connection.send(out);
        return true;
    }
};

// Runs the event loop until `done` or two seconds.
template <typename F>
bool waitFor(F done)
{
    QElapsedTimer t;
    t.start();
    while (!done() && t.elapsed() < 2000) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    }
    return done();
}

// For "nothing arrives" checks.
void settle()
{
    QElapsedTimer t;
    t.start();
    while (t.elapsed() < 300) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 20);
    }
}

void emitSignal(QDBusConnection& bus, const char* name, const QVariantList& args)
{
    QDBusMessage m = QDBusMessage::createSignal(QString::fromLatin1(kPath),
                                                QString::fromLatin1(kService),
                                                QString::fromLatin1(name));
    m.setArguments(args);
    bus.send(m);
}

QVariant u32(unsigned int v)
{
    return QVariant::fromValue<quint32>(v);
}

QVariantMap hintsOf(const QDBusMessage& m)
{
    return qdbus_cast<QVariantMap>(m.arguments()[6].value<QDBusArgument>());
}

// Notify's arguments and their D-Bus types.
void checkNotifyCall(const QDBusMessage& m)
{
    check(m.signature() == QStringLiteral("susssasa{sv}i"), "notify: signature susssasa{sv}i");
    const QVariantList a = m.arguments();
    check(a.size() == 8, "notify: 8 arguments");
    if (a.size() != 8) {
        return;
    }
    check(a[0].toString() == QStringLiteral("Envio") && a[2].toString() == QStringLiteral("/icon.svg")
              && a[3].toString() == QStringLiteral("Summary") && a[4].toString() == QStringLiteral("Body"),
          "notify: app_name, icon, summary, body");
    check(a[1].userType() == QMetaType::UInt && a[1].toUInt() == 5u, "notify: replaces_id is u = 5");
    check(a[5].toStringList() == QStringList({QStringLiteral("default"), QString()}),
          "notify: actions are [default, \"\"]");
    check(a[7].userType() == QMetaType::Int && a[7].toInt() == -1, "notify: timeout is i = -1");
    const QVariantMap hints = hintsOf(m);
    check(hints.value(QStringLiteral("urgency")).userType() == QMetaType::UChar
              && hints.value(QStringLiteral("urgency")).toUInt() == 1u,
          "notify: urgency hint is y = 1");
    check(hints.value(QStringLiteral("category")).toString() == QStringLiteral("email.arrived")
              && hints.value(QStringLiteral("desktop-entry")).toString() == QStringLiteral("envio.aps"),
          "notify: category and desktop-entry hints");
    check(!hints.contains(QStringLiteral("image-path")), "notify: no image-path when empty");
}

} // namespace

int main(int argc, char** argv)
{
    QCoreApplication app(argc, argv);
    const QString address = QString::fromLocal8Bit(qgetenv("DBUS_SESSION_BUS_ADDRESS"));
    if (address.isEmpty()) {
        qCritical("DBUS_SESSION_BUS_ADDRESS is empty: run through tests/notify/run.sh");
        return 2;
    }
    QDBusConnection fakeBus = QDBusConnection::connectToBus(address, QStringLiteral("fake"));
    if (!fakeBus.isConnected()) {
        qCritical("no private bus");
        return 2;
    }
    Fake fake;
    fakeBus.registerVirtualObject(QString::fromLatin1(kPath), &fake,
                                  QDBusConnection::VirtualObjectRegisterOption::SubPath);

    check(!envio_bus_has_service(kService, kServiceLen), "no server: the probe finds none");
    envio_notify_init(onReply, onClosed, onClicked, onToken, onCaps, onOwner);
    settle();

    // Owner changes: taken (no owner before), released, taken again.
    check(fakeBus.interface()->registerService(QString::fromLatin1(kService)).isValid(),
          "fake takes the name");
    check(waitFor([] { return g_seen.owner.size() == 1; }) && !g_seen.owner[0],
          "owner change: first take reports hadOwner=false");
    check(envio_bus_has_service(kService, kServiceLen), "the probe finds the server");

    // GetCapabilities: markup, no markup, failure.
    envio_notify_query_caps(7);
    check(waitFor([] { return g_seen.caps.size() == 1; }) && g_seen.caps[0] == std::make_pair(2, 7u),
          "caps: body-markup -> 2, generation 7 echoed");
    fake.caps = QStringList{QStringLiteral("body")};
    envio_notify_query_caps(8);
    check(waitFor([] { return g_seen.caps.size() == 2; }) && g_seen.caps[1] == std::make_pair(1, 8u),
          "caps: no body-markup -> 1");
    fake.fail = true;
    envio_notify_query_caps(9);
    check(waitFor([] { return g_seen.caps.size() == 3; }) && g_seen.caps[2] == std::make_pair(0, 9u),
          "caps: error reply -> 0");

    // Notify: the call, then the reply.
    fake.fail = false;
    Card card{str("tag-1"), str("Envio"), str("/icon.svg"), str("Summary"), str("Body"),
              str("email.arrived"), str("envio.aps"), str(""), 5u, 3u, 1};
    envio_notify_send(&card);
    check(waitFor([] { return g_seen.replies.size() == 1; }), "notify: a reply arrives");
    check(fake.notifies.size() == 1, "notify: the server got one call");
    if (!fake.notifies.empty()) {
        checkNotifyCall(fake.notifies[0]);
    }
    check(g_seen.replies.size() == 1
              && g_seen.replies[0] == std::make_tuple(QStringLiteral("tag-1"), 42u, 3u),
          "notify: the reply id, tag and generation reach Rust");

    card.imagePath = str("/avatar.png");
    envio_notify_send(&card);
    check(waitFor([&] { return fake.notifies.size() == 2 && g_seen.replies.size() == 2; }),
          "notify: a second card");
    if (fake.notifies.size() == 2) {
        check(hintsOf(fake.notifies[1]).value(QStringLiteral("image-path")).toString()
                  == QStringLiteral("/avatar.png"),
              "notify: image-path hint when given");
    }

    fake.fail = true;
    envio_notify_send(&card);
    check(waitFor([] { return g_seen.replies.size() == 3; }) && std::get<1>(g_seen.replies[2]) == 0u
              && std::get<2>(g_seen.replies[2]) == 3u,
          "notify: an error reply gives id 0");
    fake.fail = false;

    // Signals from the server.
    emitSignal(fakeBus, "ActionInvoked", {u32(42), QStringLiteral("other")});
    emitSignal(fakeBus, "ActionInvoked", {u32(42), QStringLiteral("default")});
    check(waitFor([] { return !g_seen.clicked.empty(); }), "ActionInvoked default reaches Rust");
    settle();
    check(g_seen.clicked == std::vector<unsigned int>{42}, "ActionInvoked with another key is ignored");
    emitSignal(fakeBus, "ActivationToken", {u32(42), QStringLiteral("tok")});
    check(waitFor([] { return !g_seen.tokens.empty(); })
              && g_seen.tokens[0] == std::make_pair(42u, QStringLiteral("tok")),
          "ActivationToken reaches Rust");
    emitSignal(fakeBus, "NotificationClosed", {u32(42), u32(2)});
    check(waitFor([] { return !g_seen.closed.empty(); }) && g_seen.closed[0] == 42u,
          "NotificationClosed reaches Rust");

    // The server goes away and a new one takes the name.
    fakeBus.interface()->unregisterService(QString::fromLatin1(kService));
    check(waitFor([] { return g_seen.owner.size() == 2; }) && g_seen.owner[1],
          "owner change: release reports hadOwner=true");
    fakeBus.interface()->registerService(QString::fromLatin1(kService));
    check(waitFor([] { return g_seen.owner.size() == 3; }) && !g_seen.owner[2],
          "owner change: a new server reports hadOwner=false");
    emitSignal(fakeBus, "NotificationClosed", {u32(43), u32(1)});
    check(waitFor([] { return g_seen.closed.size() == 2; }), "signals still arrive after a restart");

    qInfo("%s", g_failures == 0 ? "notify glue: all passed" : "notify glue: FAILED");
    return g_failures == 0 ? 0 : 1;
}
