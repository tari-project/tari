#ifndef NODELLIST_H
#define NODELLIST_H
#include "config.h"
#include "basenode.h"
#include "nodeedit.h"
#include <QWidget>
#include <QListWidget>
#include <QAbstractButton>
#include <QListWidgetItem>

namespace Ui {
class nodelist;
}

class nodelist : public QWidget
{
    Q_OBJECT

public:
    explicit nodelist(std::vector<basenode*>* nodes, config* preferences, QWidget *parent = nullptr);
    ~nodelist();

signals:
    void added(basenode*);
    void edited();
    void removed(basenode*);

private slots:
    void on_nodeList_itemClicked(QListWidgetItem *item);

    void on_addNode_clicked();

    void addNode(basenode*);
    void removeNode(basenode*);
    void editNode();

    void on_dialogButtons_clicked(QAbstractButton *button);

private:
    Ui::nodelist *ui;
    std::vector<basenode*>* m_nodes;
    config* m_preferences;
    nodeEdit* m_nodeedit;
    void updateList();
};

#endif // NODELLIST_H
