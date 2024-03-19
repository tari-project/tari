#!/bin/bash

# Run the Tari test suite locally inside a suitable docker container

IMAGE=quay.io/tarilabs/rust_tari-build-with-deps:nightly-2023-12-12
TOOLCHAIN_VERSION=nightly-2023-12-12
CONTAINER=tari_test

echo "Deleting old container"
docker rm -f $CONTAINER
echo "Checking for docker image"
docker pull $IMAGE

echo "Creating docker container.."
# sleep infinity is used to keep the container alive forever
docker run -dv`pwd`:/src --name $CONTAINER $IMAGE /bin/sleep infinity
echo "Container is ready"

CMD="cd src; cargo build; cargo test"

docker exec -ti $CONTAINER /bin/bash -c "$CMD"

echo "Tests complete. You can play with the build interactively by running this command:"
echo "docker exec -ti -w /src $CONTAINER /bin/bash"
