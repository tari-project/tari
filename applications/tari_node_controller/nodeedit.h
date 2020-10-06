#ifndef NODEEDIT_H
#define NODEEDIT_H

#include "basenode.h"
#include <QWidget>
#include <QRegExpValidator>

namespace Ui {
class nodeEdit;
}

class nodeEdit : public QWidget
{
    Q_OBJECT

public:
    explicit nodeEdit(basenode* node, std::vector<basenode*> nodes, QWidget *parent = nullptr);
    ~nodeEdit();

signals:
    void added(basenode*);
    void removed(basenode*);
    void edited();

private slots:
    void on_dialogButtons_accepted();
    void on_dialogButtons_rejected();
    void on_deleteNode_clicked();

private:
    Ui::nodeEdit *ui;
    basenode* m_node;
    std::vector<basenode*> m_nodes;

};

#endif // NODEEDIT_H
