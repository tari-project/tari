#include <QApplication>
#include <QMessageBox>
#include "systray.h"
#include "grpcclient.h"

int main(int argc, char *argv[])
{
    GOOGLE_PROTOBUF_VERIFY_VERSION;
    Q_INIT_RESOURCE(resources);
    QApplication::setAttribute(Qt::AA_UseHighDpiPixmaps);
    QApplication app(argc, argv);
    app.setActiveWindow(nullptr);

    if (!QSystemTrayIcon::isSystemTrayAvailable()) {
        QMessageBox::critical(nullptr, QObject::tr("Systray"),
                              QObject::tr("System Tray Unavailable."));
        return 1;
    }

    // Prevent early exit
    QApplication::setQuitOnLastWindowClosed(false);
    config* preferences = new config("config.xml");
    systray* systray = new class systray(nullptr, &app, preferences);
    systray->show();
    return app.exec();
}
