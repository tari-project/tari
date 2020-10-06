#include "basenode.h"

inline bool areEqualRel(double a, double b, double epsilon) {
    return (fabs(a - b) <= epsilon * std::max(fabs(a), fabs(b)));
}

basenode::basenode(QObject *parent) : QObject(parent), m_name(""), m_address(""), m_height(0), m_percentage(0),
    m_responsive(true), m_client(nullptr), m_thread(nullptr), m_updater(nullptr)
{
    this->m_updater = new QTimer(this);
    connect(m_updater, SIGNAL(timeout()), this, SLOT(updateHeight()));
    m_updater->start(TIMER_MILLISECONDS);
    connect(this,SIGNAL(addressChanged(basenode*)),this,SLOT(updateHeight()));
    connect(this,SIGNAL(heightChanged(basenode*)),this,SLOT(updatePercentage()));
    this->m_thread = new QThread(this);
}

QString basenode::name() const
{
    return m_name;
}

QString basenode::address() const
{
    return m_address;
}

uint64_t basenode::height() const
{
    return m_height;
}

double basenode::percentage() const
{
    return m_percentage;
}

bool basenode::responsive() const
{
    return m_responsive;
}

void basenode::setName(QString name)
{
    if (m_name != name)
    {
        qDebug()<<"Updating Name";
        QString previousName = m_name;
        m_name = name;
        emit nameChanged(this,previousName);
    }
}

void basenode::setAddress(QString address)
{
    if (m_address != address)
    {
        qDebug()<<"Updating Address";
        m_address = address;

        if (this->m_thread->isRunning())
        {
            this->m_thread->quit();
            this->m_thread->wait();
        }

        if (m_client)
        {
            delete m_client;
        }

        this->m_client = new grpcclient(nullptr, address);
        connect(this->m_client,SIGNAL(responsive(bool)),this,SLOT(updateResponsive(bool)));
        this->m_client->moveToThread(this->m_thread);
        this->m_thread->start();
        emit addressChanged(this);
    }
}

void basenode::setHeight(uint64_t height)
{
    if (m_height != height)
    {
        m_height = height;
        emit heightChanged(this);
    }
}

void basenode::setPercentage(double percentage)
{
    if (!areEqualRel(m_percentage,percentage,std::numeric_limits<double>::epsilon()))
    {
        m_percentage = percentage;
        emit percentageChanged(this);
    }
}

void basenode::setResponsive(bool responsive)
{
    if (m_responsive != responsive)
    {
        m_responsive = responsive;
        emit responsiveChanged(this);
    }
}

basenode::~basenode()
{

    if (m_updater)
    {
       m_updater->stop();
       delete m_updater;
    }

    if (m_thread)
    {
        this->m_thread->quit();
        this->m_thread->wait();
        delete m_thread;
    }

    if (m_client)
    {
        delete m_client;
    }
}

void basenode::updateHeight()
{
    qDebug() << "Slot updateHeight() called";
    if (m_client)
    {
        setHeight(this->m_client->current_height());
    } else {
        setResponsive(false);
    }
}

void basenode::updatePercentage()
{
    qDebug() << "Slot updatePercentage() called";
    if (m_client)
    {
        uint64_t max = this->m_client->max_height();
        if (max == 0)
        {
            setResponsive(false);
        } else {
            uint64_t current = this->m_client->current_height();
            double percentage = static_cast<double>(current)/static_cast<double>(max)*static_cast<double>(100);
            setPercentage(percentage);
        }
    } else {
        setResponsive(false);
    }
}

void basenode::updateResponsive(bool responsive)
{
    setResponsive(responsive);
}
