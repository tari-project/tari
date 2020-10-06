#include <QObject>
#include <iostream>
#include <string>
#include <grpc++/grpc++.h>
#include "gen/base_node.pb.h"
#include "gen/base_node.grpc.pb.h"

using grpc::Channel;
using grpc::ChannelInterface;
using grpc::ClientContext;
using grpc::Status;
using tari::rpc::TipInfoResponse;
using tari::rpc::SyncInfoResponse;
using tari::rpc::BaseNode;

#ifndef GRPCCLIENT_H
#define GRPCCLIENT_H


class grpcclient: public QObject
{
    Q_OBJECT
public:
    explicit grpcclient(QObject *parent = nullptr, QString address = "");
    uint64_t max_height();
    uint64_t current_height();
    ~grpcclient();

private:
    std::unique_ptr<BaseNode::Stub> m_stub;

signals:
    void responsive(bool);
};

#endif // GRPCCLIENT_H
