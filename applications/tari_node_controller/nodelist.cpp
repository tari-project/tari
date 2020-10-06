#include "nodelist.h"
#include "ui_nodelist.h"

nodelist::nodelist(std::vector<basenode*>* nodes, config* preferences, QWidget *parent) :
    QWidget(parent, Qt::Popup | Qt:: Dialog),
    ui(new Ui::nodelist), m_nodes(nodes), m_preferences(preferences), m_nodeedit(nullptr)
{
    ui->setupUi(this);
    updateList();
}

void nodelist::updateList()
{
    ui->nodeList->clear();
    std::vector<basenode*>::iterator it;
    for (it = this->m_nodes->begin(); it != this->m_nodes->end(); it++)
    {
        ui->nodeList->addItem((*it)->name());
    }
}

nodelist::~nodelist()
{
    if (this->m_nodeedit)
    {
        delete m_nodeedit;
    }

    delete ui;
}

void nodelist::on_nodeList_itemClicked(QListWidgetItem *item)
{
    if (this->m_nodeedit)
    {
        delete m_nodeedit;
    }
    basenode* node = nullptr;
    std::vector<basenode*>::iterator it;
    for (it = this->m_nodes->begin(); it != this->m_nodes->end(); it++)
    {
        if ((*it)->name() == item->text())
        {
            node = (*it);
        }
    }

    this->m_nodeedit = new nodeEdit(node,*m_nodes,this);
    this->m_nodeedit->resize(600,300);
    this->m_nodeedit->setWindowModality(Qt::WindowModal);
    connect(this->m_nodeedit,SIGNAL(added(basenode*)),this,SLOT(addNode(basenode*)));
    connect(this->m_nodeedit,SIGNAL(removed(basenode*)),this,SLOT(removeNode(basenode*)));
    connect(this->m_nodeedit,SIGNAL(edited()),this,SLOT(editNode()));
    this->m_nodeedit->show();
}

void nodelist::on_addNode_clicked()
{
    if (this->m_nodeedit)
    {
        delete m_nodeedit;
    }
    this->m_nodeedit = new nodeEdit(nullptr,*m_nodes,this);
    this->m_nodeedit->resize(600,300);
    this->m_nodeedit->setWindowModality(Qt::WindowModal);
    connect(this->m_nodeedit,SIGNAL(added(basenode*)),this,SLOT(addNode(basenode*)));
    this->m_nodeedit->show();
}

void nodelist::addNode(basenode* node)
{
    if (node)
    {
        emit added(node);
        updateList();
    }
}

void nodelist::removeNode(basenode* node)
{
    if (node)
    {
        emit removed(node);
        updateList();
    }
}

void nodelist::editNode()
{
    emit edited();
    updateList();
}


void nodelist::on_dialogButtons_clicked(QAbstractButton *button)
{
    if (button->text().toLower() == "close" ||
            //linux automatically appends '&' shortcut character in some cases
            button->text().toLower() == "&close")
    {
        if (this->m_nodes)
        {
            this->m_preferences->save(*this->m_nodes);
        }
        this->close();
    }
}
