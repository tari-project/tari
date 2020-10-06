greaterThan(QT_MAJOR_VERSION, 4): QT += widgets core gui xml

CONFIG += c++11 sdk_no_version_check

# Comment this out when writing new code
# Silence unused parameter warnings from generated grpc
# Silence Q_PROPERTY macro warning since it is only complaining about an old style c++ cast (which is still valid).
QMAKE_CXXFLAGS_WARN_ON += -Wno-unused-parameter -Wno-string-conversion

# Disables all the APIs deprecated before Qt 6.0.0
DEFINES += QT_DISABLE_DEPRECATED_BEFORE=0x060000

# TODO: Windows.
unix:!android: {
    INCLUDEPATH += \
        /usr/local/include \
        gen/

    LIBS += -L/usr/local/lib -lgrpc++ -lprotobuf
}

RESOURCES += \
    resources.qrc

HEADERS += \
    basenode.h \
    config.h \
    grpcclient.h \
    nodeedit.h \
    nodelist.h \
    systray.h \
    gen/types.pb.h \
    gen/types.grpc.pb.h \
    gen/base_node.grpc.pb.h \
    gen/base_node.pb.h

SOURCES += \
    basenode.cpp \
    config.cpp \
    grpcclient.cpp \
    main.cpp \
    nodeedit.cpp \
    nodelist.cpp \
    systray.cpp \
    gen/types.pb.cc \
    gen/types.grpc.pb.cc \
    gen/base_node.grpc.pb.cc \
    gen/base_node.pb.cc

FORMS += \
    nodeedit.ui \
    nodelist.ui

TRANSLATIONS += \
    en_US.ts

# TODO: Default rules for deployment.
qnx: target.path = /tmp/$${TARGET}/bin
else: unix:!android: target.path = /opt/$${TARGET}/bin
!isEmpty(target.path): INSTALLS += target

# Custom Info.plist is needed to disable application icon appearing in dock and alt-tab
# in macOS, it is an OS level feature which is not accessible programatically
macx {
    QMAKE_INFO_PLIST = Info.plist
}

# TODO: Default rules for deployment (TODO: Windows, Linux, Mac).
DISTFILES +=
