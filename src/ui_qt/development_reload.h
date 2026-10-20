#pragma once

#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QFileSystemWatcher>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QTimer>
#include <QWindow>
#include <memory>

// Development only: the Rust singleton and its session lease remain owned by
// the existing engine. A failed candidate leaves the current window available.
inline void install_development_reload(QQmlApplicationEngine &engine, const QUrl &source)
{
    const auto path = source.toLocalFile();
    auto *watcher = new QFileSystemWatcher(&engine);
    auto *timer = new QTimer(&engine);
    timer->setSingleShot(true);
    auto fingerprint = [](const QString &file) {
        QFile input(file);
        if (!input.open(QIODevice::ReadOnly) || input.size() > 8 * 1024 * 1024)
            return QByteArray();
        return QCryptographicHash::hash(input.readAll(), QCryptographicHash::Sha256);
    };
    auto loaded = std::make_shared<QByteArray>(fingerprint(path));
    watcher->addPath(path);
    // Atomic editor saves replace the watched inode; the directory watch lets
    // us reattach the file watch without relying on an editor's save strategy.
    watcher->addPath(QFileInfo(path).absolutePath());
    QObject::connect(watcher, &QFileSystemWatcher::fileChanged, timer,
                     [timer](const QString &) { timer->start(150); });
    QObject::connect(watcher, &QFileSystemWatcher::directoryChanged, timer,
                     [timer](const QString &) { timer->start(150); });
    QObject::connect(timer, &QTimer::timeout, &engine,
        [&engine, source, path, watcher, timer, fingerprint, loaded] {
            if (!QFileInfo::exists(path)) {
                timer->start(250);
                return;
            }
            if (!watcher->files().contains(path))
                watcher->addPath(path);
            const auto changed = fingerprint(path);
            if (changed.isEmpty() || changed == *loaded)
                return;
            auto *backend = engine.singletonInstance<QObject *>(
                "org.modsync.desktop", "AuthoringBackend");
            if (!backend || backend->property("busy").toBool()) {
                timer->start(250);
                return;
            }
            const auto previous = engine.rootObjects();
            if (previous.isEmpty())
                return;
            engine.clearComponentCache();
            engine.load(source);
            const auto current = engine.rootObjects();
            if (current.size() <= previous.size()) {
                qWarning("Development QML reload failed; existing window retained");
                return;
            }
            auto *replacement = current.last();
            auto *old = previous.last();
            if (auto *window = qobject_cast<QWindow *>(replacement)) {
                if (auto *oldWindow = qobject_cast<QWindow *>(old))
                    window->setGeometry(oldWindow->geometry());
            }
            for (const auto *name : {"componentIndex", "folder", "presentation",
                                     "sortColumn", "sortAscending"})
                replacement->setProperty(name, old->property(name));
            QMetaObject::invokeMethod(replacement, "updateRows");
            QMetaObject::invokeMethod(replacement, "updateTree");
            *loaded = changed;
            // Unmap the candidate while removing the old window, then expose
            // it afresh. Software rendering otherwise may retain partial damage.
            auto *window = qobject_cast<QWindow *>(replacement);
            if (window)
                window->hide();
            const auto quitOnClose = QGuiApplication::quitOnLastWindowClosed();
            QGuiApplication::setQuitOnLastWindowClosed(false);
            for (auto *root : previous)
                delete root;
            if (window) {
                if (auto *quick = qobject_cast<QQuickWindow *>(window)) {
                    quick->setPersistentSceneGraph(false);
                    quick->setPersistentGraphics(false);
                    quick->releaseResources();
                    quick->show();
                    quick->update();
                } else {
                    window->show();
                    window->requestUpdate();
                }
            }
            QGuiApplication::setQuitOnLastWindowClosed(quitOnClose);
            qInfo("Development QML reloaded; Rust authoring session retained");
        });
}
