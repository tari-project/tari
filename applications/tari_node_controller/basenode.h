#include <QObject>
#include <QDebug>
#include <QThread>
#include <QTimer>
#include "math.h" // for linux to find fabs function
#include "grpcclient.h"

#ifndef BASENODE_H
#define BASENODE_H

const int timeout = 60000;
#define TIMER_MILLISECONDS timeout

class basenode : public QObject
{
    Q_OBJECT
public:
    explicit basenode(QObject *parent = nullptr);
    Q_PROPERTY(QString name READ name WRITE setName NOTIFY nameChanged)
    Q_PROPERTY(QString address READ address WRITE setAddress NOTIFY addressChanged)
    Q_PROPERTY(int height READ height NOTIFY nameChanged)
    Q_PROPERTY(double percentage READ percentage NOTIFY percentageChanged)
    Q_PROPERTY(bool responsive READ responsive NOTIFY responsiveChanged)
    ~basenode();

public:
    QString name() const;
    QString address() const;
    uint64_t height() const;
    double percentage() const;
    bool responsive() const;
    void setName(QString name);
    void setAddress(QString address);

private:
    void setHeight(uint64_t height);
    void setPercentage(double percentage);
    void setResponsive(bool responsive);

    QString m_name;
    QString m_address;
    uint64_t m_height;
    double m_percentage;
    bool m_responsive;
    grpcclient* m_client;
    QThread* m_thread;
    QTimer* m_updater;

signals:
    void nameChanged(basenode*,QString);
    void addressChanged(basenode*);
    void heightChanged(basenode*);
    void percentageChanged(basenode*);
    void responsiveChanged(basenode*);

private slots:
    void updateHeight();
    void updatePercentage();
    void updateResponsive(bool);

};

#endif // BASENODE_H
