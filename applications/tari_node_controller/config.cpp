#include "config.h"
#include <QDebug>


config::config(QString filename) : m_filename(filename)
{
    set_dir();
    qDebug()<<"File"<<m_filename;
    QFile f(m_filename);
    if (!f.open(QIODevice::ReadWrite))
    {
        qDebug() << "failed to create/open file";
    }

    QDomDocument xmlConfig;
    xmlConfig.setContent(&f);
    f.close();
    this->m_preferences = xmlConfig;
}

void config::set_dir()
{
    QDir currentWorkingDir = QDir::currentPath();
    /*
       On Mac, working directory is inside the .app folder, need to move 3 levels up

        TODO: Use standard application data storage directory
    */
    #if defined(Q_OS_DARWIN)
        currentWorkingDir.cdUp();
        currentWorkingDir.cdUp();
        currentWorkingDir.cdUp();
    #endif
    QDir::setCurrent(currentWorkingDir.absolutePath());
    qDebug()<<"Path: "<<currentWorkingDir.absolutePath();
}

std::vector<basenode*> config::get_nodes() {
    std::vector<basenode*> result;
    QDomNodeList nodes = this->m_preferences.elementsByTagName("BASENODE");
    qDebug() << "# nodes = " << nodes.count();

    for(int i = 0; i < nodes.count(); i++)
        {
            QDomNode elm = nodes.at(i);
            if(elm.isElement())
            {
                QString name;
                QString address;
                QString port;

                QDomElement e = elm.toElement();
                int childCount = e.childNodes().count();
                for (int j = 0; j < childCount; j++)
                {
                    QDomNode prop = e.childNodes().at(j);
                    if (prop.isElement())
                    {
                       QDomElement f = prop.toElement();
                       qDebug() << f.text();
                       if (f.tagName() == "NAME")
                       {
                           name = f.text();
                       } else if (f.tagName() == "ADDRESS")
                       {
                           address = f.text();
                       } else if (f.tagName() == "PORT")
                       {
                           port = f.text();
                       } else {

                       }
                    }
                }
                QString fulladdress = QString(address).append(":").append(port);
                qDebug() << fulladdress;
                basenode* node = new basenode();
                node->setName(name);
                node->setAddress(fulladdress);
                result.push_back(node);

            }
        }
    return result;
}

void config::save(std::vector<basenode*> nodes)
{
    qDebug()<<"File"<<m_filename;
    QFile f(m_filename);
    if (!f.open(QIODevice::QIODevice::WriteOnly))
    {
        qDebug() << "failed to create/open file";
    }
    QDomDocument preferences;
    QDomElement root = preferences.createElement("CONFIG");
    root.setAttribute("Version","1.0");
    preferences.appendChild(root);
    std::vector<basenode*>::iterator it;
    for (it = nodes.begin(); it != nodes.end(); it++)
    {
        QDomElement basenode = preferences.createElement("BASENODE");
        root.appendChild(basenode);
        QDomElement nameTag = preferences.createElement("NAME");
        basenode.appendChild(nameTag);
        QDomText name = preferences.createTextNode((*it)->name());
        nameTag.appendChild(name);
        QStringList addressDetail = (*it)->address().split(":");
        QDomElement addressTag = preferences.createElement("ADDRESS");
        basenode.appendChild(addressTag);
        QDomText address = preferences.createTextNode(addressDetail[0]);
        addressTag.appendChild(address);
        QDomElement portTag = preferences.createElement("PORT");
        basenode.appendChild(portTag);
        QDomText port = preferences.createTextNode(addressDetail[1]);
        portTag.appendChild(port);
    }
    QTextStream output(&f);
    output << preferences.toString();
    f.close();
    this->m_preferences = preferences;
}

config::~config()
{}
