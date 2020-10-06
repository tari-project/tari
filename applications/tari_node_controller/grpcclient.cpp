#include "grpcclient.h"

grpcclient::grpcclient(QObject* parent, QString address) : QObject(parent) {
    std::shared_ptr<Channel> channel = grpc::CreateChannel(address.toUtf8().constData(), grpc::InsecureChannelCredentials());
    this->m_stub = BaseNode::NewStub(channel);
}

grpcclient::~grpcclient() {
    this->m_stub.release();
}

uint64_t grpcclient::max_height() {
    SyncInfoResponse response;
    ClientContext context;
    Status status = m_stub->GetSyncInfo(&context,tari::rpc::Empty(),&response);
    if (status.ok()) {
        emit responsive(true);
        return response.tip_height();
    } else {
        emit responsive(false);
        return 0;
    }
}

uint64_t grpcclient::current_height() {
    SyncInfoResponse response;
    ClientContext context;
    Status status = m_stub->GetSyncInfo(&context,tari::rpc::Empty(),&response);
    if (status.ok()) {
        emit responsive(true);
        return response.local_height();
    } else {
        emit responsive(false);
        return 0;
    }
}
