#include "nodeedit.h"
#include "ui_nodeedit.h"

nodeEdit::nodeEdit(basenode* node, std::vector<basenode*> nodes, QWidget *parent) :
    QWidget(parent, Qt::Popup | Qt:: Dialog),
    ui(new Ui::nodeEdit), m_node(node), m_nodes(nodes)
{
    ui->setupUi(this);
    if (this->m_node)
    {
        ui->nodeName->setText(m_node->name());
        ui->nodeAddress->setText(m_node->address());
    } else
    {
        ui->deleteNode->setEnabled(false);
    }
    ui->nodeAddress->setInputMask(QString("000.000.000.000:00000"));
}

nodeEdit::~nodeEdit()
{
    delete ui;
}

void nodeEdit::on_dialogButtons_accepted()
{
    if (ui->nodeName->text().simplified().trimmed().length() > 0 && ui->nodeAddress->text().length() > 0)
    {
        // TODO: Rewrite address validation into custom QValidator
        QStringList addressParts = ui->nodeAddress->text().split(":");
        if (addressParts.length() == 2 )
        {
            QStringList ipParts = addressParts[0].split(".");
            int count = 0;
            int i = std::stoi(addressParts[1].toUtf8().constData());
            if (i > 0 && i <= 65535)
            {
                count++;
            }
            for (QString octet : ipParts)
            {
                int i = std::stoi(octet.toUtf8().constData());
                if (i >= 0 && i <= 255)
                {
                    count++;
                }
            }

            if (count == 5)
            {
                QStringList nodeNames;
                QStringList nodeIPs;
                std::vector<basenode*>::iterator it;
                for (it = m_nodes.begin(); it != m_nodes.end(); it++)
                {
                    nodeNames.append((*it)->name());
                    nodeIPs.append((*it)->address());
                }

                if (!nodeNames.contains(ui->nodeName->text()) && !nodeIPs.contains(ui->nodeAddress->text()))
                {
                    if (!this->m_node)
                    {
                        m_node = new basenode(nullptr);
                        m_node->setName(ui->nodeName->text());
                        m_node->setAddress(ui->nodeAddress->text());
                        emit added(m_node);
                    }
                    else if ( m_node->name() != ui->nodeName->text() || m_node->address() != ui->nodeAddress->text()){
                        m_node->setName(ui->nodeName->text());
                        m_node->setAddress(ui->nodeAddress->text());
                        emit edited();
                    }
                    this->close();
                }
            }
        }
    }
}

void nodeEdit::on_dialogButtons_rejected()
{
    this->close();
}

void nodeEdit::on_deleteNode_clicked()
{
    if (this->m_node)
    {
        emit removed(m_node);
    }
    this->close();
}
