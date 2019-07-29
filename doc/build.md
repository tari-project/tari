# Build Instructions and Notes

## Prerequisites

Install ZeroMQ

### Mac OS
```brew install pkg-config zmq```

###  Debian, Ubuntu, Fedora, CentOS, RHEL, SUSE

```
echo "deb http://download.opensuse.org/repositories/network:/messaging:/zeromq:/release-stable/Debian_9.0/ ./" >> /etc/apt/sources.list
wget https://download.opensuse.org/repositories/network:/messaging:/zeromq:/release-stable/Debian_9.0/Release.key -O- | sudo apt-key add
apt-get install libzmq3-dev
```

### Other platforms
See [ZeroMQ documentation](http://zeromq.org/intro:get-the-software)

## Build

Make sure you are running nightly version: `nightly-2019-03-08`

To build run
`cargo build`