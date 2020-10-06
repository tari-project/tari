#include "systray.h"

systray::systray(QObject *parent, QApplication *app, config *preferences) : QObject(parent), m_tray(nullptr), m_menu(nullptr),
    m_submenu(nullptr), m_app(app), m_preferences(preferences), m_nodelist(nullptr)
{
    init_system_tray(preferences);
}

void systray::init_system_tray(config *preferences) {
    QIcon icon = QIcon(":/images/splash_gem_small.png");
    QSystemTrayIcon* tray = new QSystemTrayIcon(icon,this);
    menu* mainmenu = new menu(nullptr);
    init_main_menu(mainmenu,preferences);
    tray->setContextMenu(mainmenu);
    this->m_tray = tray;
    this->m_menu = mainmenu;
}

void systray::init_main_menu(menu *mainmenu, config *preferences) {
    menu *submenu = new menu(mainmenu);
    init_submenu(submenu, preferences);
    mainmenu->addMenu(submenu);
    mainmenu->addSeparator();
    mainmenu->addAction("Preferences",this,SLOT(openPreferences()));
    mainmenu->addSeparator();
    mainmenu->addAction("Quit",this->m_app,SLOT(quit()));
    mainmenu->setToolTipsVisible(true);
    this->m_submenu = submenu;
}

QIcon systray::status_icon(bool responsive)
{
    if (responsive)
    {
        return QIcon(":/images/green_dot.png");
    }
    return QIcon(":/images/red_dot.png");
}

void systray::set_action_tooltip(QAction* action, basenode* node)
{
    QString status = QString("Height: ").append(QString::number(node->height())).append(", Percentage Synced: ").append(QString::number(node->percentage()));
    action->setToolTip(status);
    #if defined(Q_OS_DARWIN)
    // Mac receives the hover signal
        connect(action, &QAction::hovered, [=]{
            QToolTip::showText(QCursor::pos(), action->toolTip().toUtf8().constData(), nullptr);
        });
    #elif defined(Q_OS_LINUX)
    // Linux does not
        connect(action, &QAction::triggered, [=]{
            QToolTip::showText(QCursor::pos(), action->toolTip().toUtf8().constData(), nullptr);
        });

    #endif
}

void systray::init_submenu_item(menu* submenu, basenode* node) {
    QAction* action = new QAction(node->name());
    action->setIcon(status_icon(node->responsive()));
    action->setIconVisibleInMenu(true);
    set_action_tooltip(action,node);
    submenu->addAction(action);
    connect(node,SIGNAL(nameChanged(basenode*,QString)),this,SLOT(updateSubmenuItem(basenode*,QString)));
    connect(node,SIGNAL(addressChanged(basenode*)),this,SLOT(updateSubmenuItem(basenode*)));
    connect(node,SIGNAL(responsiveChanged(basenode*)),this,SLOT(updateSubmenuItem(basenode*)));
    connect(node,SIGNAL(heightChanged(basenode*)),this,SLOT(updateSubmenuItem(basenode*)));
    connect(node,SIGNAL(percentageChanged(basenode*)),this,SLOT(updateSubmenuItem(basenode*)));
}


void systray::init_submenu(menu* submenu, config *preferences) {
    m_nodes = preferences->get_nodes();
    std::vector<basenode*>::iterator it;
    qDebug() << m_nodes.size();
    submenu->setTitle("Nodes");
    submenu->setToolTipsVisible(true);
    for (it = m_nodes.begin(); it != m_nodes.end(); it++)
    {
        init_submenu_item(submenu,(*it));
    }
    submenu->setStyle(new proxystyle(submenu->style()));
}

void systray::show() {
    if (this->m_tray)
    {
        this->m_tray->show();
    }
}

systray::~systray() {

    if (this->m_menu)
    {
        delete m_menu;
    }

    if (this->m_submenu)
    {
        delete m_menu;
    }

    if (this->m_tray)
    {
        delete m_tray;
    }

    if (this->m_nodelist)
    {
        delete m_nodelist;
    }

    if(this->m_app)
    {
        delete m_app;
    }

    std::vector<basenode*>::iterator it;
    for (it = m_nodes.begin(); it != m_nodes.end(); it++)
    {
        basenode* node = (*it);
        delete node;
    }
}

void systray::updateSubmenuItem(basenode* node) {
     QList<QAction*> list = this->m_submenu->actions();
     for(int i=0; i < list.count();i++)
     {
        QAction* action = list.at(i);
        if (action->text() == node->name())
        {
                action->setText(node->name());
                action->setIcon(status_icon(node->responsive()));
                set_action_tooltip(action,node);
        }
     }
}

void systray::updateSubmenuItem(basenode* node, QString previousName) {
     QList<QAction*> list = this->m_submenu->actions();
     for(int i=0; i < list.count();i++)
     {
        QAction* action = list.at(i);
        if (action->text() == previousName)
        {
                action->setText(node->name());
                action->setIcon(status_icon(node->responsive()));
                set_action_tooltip(action,node);
        }
     }
}

void systray::removeSubmenuItem(basenode* node) {
     QList<QAction*> list = this->m_submenu->actions();
     for(int i=0; i < list.count();i++)
     {
        QAction* action = list.at(i);
        if (action->text() == node->name())
        {
           m_submenu->removeAction(action);
           delete action;
           std::vector<basenode*>::iterator it;
           for ( it = this->m_nodes.begin(); it != this->m_nodes.end(); ) {
                if( (*it)->name() == node->name() ) {
                    delete *it;
                    it = this->m_nodes.erase(it);
                }
                else {
                    ++it;
                }
           }
        }
     }
}

void systray::addSubmenuItem(basenode* node) {
     this->m_nodes.push_back(node);
     init_submenu_item(this->m_submenu, node);
}

void systray::openPreferences()
{
    if (this->m_nodelist)
    {
        delete m_nodelist;
    }
    this->m_nodelist = new nodelist(&this->m_nodes,this->m_preferences,nullptr);
    this->m_nodelist->resize(400,600);
    this->m_nodelist->setWindowModality(Qt::WindowModal);
    connect(this->m_nodelist,SIGNAL(added(basenode*)),this,SLOT(addSubmenuItem(basenode*)));
    connect(this->m_nodelist,SIGNAL(removed(basenode*)),this,SLOT(removeSubmenuItem(basenode*)));
    this->m_nodelist->show();
}
