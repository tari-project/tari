#include <QObject>
#include <QWindow>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QApplication>
#include <QToolTip>
#include <QtGui>
#include <QProxyStyle>
#include "config.h"
#include "nodelist.h"

#ifndef SYSTRAY_H
#define SYSTRAY_H

class proxystyle : public QProxyStyle
{
public:
    using QProxyStyle::QProxyStyle;
    int styleHint(StyleHint hint, const QStyleOption* option = nullptr, const QWidget* widget = nullptr, QStyleHintReturn* returnData = nullptr) const override
    {
        if (hint == QStyle::SH_ToolTip_WakeUpDelay)
            return 0;
        return QProxyStyle::styleHint(hint, option, widget, returnData);
    }
};

class menu : public QMenu
{
    Q_OBJECT
public:
    explicit menu(): QMenu() {}
    explicit menu(QWidget *parent = nullptr): QMenu(parent) {}
    explicit menu(const QString &title, QWidget *parent = nullptr): QMenu(title, parent) {}

    bool event (QEvent * e)
    {
        const QHelpEvent *helpEvent = static_cast <QHelpEvent *>(e);
         if (helpEvent->type() == QEvent::ToolTip && activeAction() != nullptr)
         {
              QToolTip::showText(helpEvent->globalPos(), activeAction()->toolTip());
         } else
         {
              QToolTip::hideText();
         }
         return QMenu::event(e);
    }
};

class systray : public QObject
{
    Q_OBJECT
public:
    explicit systray(QObject *parent = nullptr, QApplication* app = nullptr, config* preferences = nullptr);
    void show();
    ~systray();

private:
    void init_system_tray(config *preferences);
    void init_main_menu(menu *mainmenu, config *preferences);
    void init_submenu(menu* submenu, config *preferences);
    void init_submenu_item(menu* submenu, basenode* node);
    void set_action_tooltip(QAction* action, basenode* node);
    QIcon status_icon(bool responsive);

    QSystemTrayIcon* m_tray;
    menu* m_menu;
    menu* m_submenu;
    QCoreApplication* m_app;
    config* m_preferences;
    std::vector<basenode*> m_nodes;
    nodelist* m_nodelist;

public slots:
    void updateSubmenuItem(basenode*);
    void updateSubmenuItem(basenode*, QString);
    void addSubmenuItem(basenode*);
    void removeSubmenuItem(basenode*);
    void openPreferences();

signals:

};

#endif // SYSTRAY_H
