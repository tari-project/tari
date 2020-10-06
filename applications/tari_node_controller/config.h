#include <QtXml>
#include <QFile>
#include "basenode.h"

#ifndef CONFIG_H
#define CONFIG_H


class config
{
public:
    config(QString filename);
    ~config();
    std::vector<basenode*> get_nodes();
    void save(std::vector<basenode*> nodes);

private:
    QDomDocument m_preferences;
    QString m_filename;
    void set_dir();
};

#endif // CONFIG_H
