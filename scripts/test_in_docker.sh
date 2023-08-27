#!/bin/bash

# Run the Taiji test suite locally inside a suitable docker container

IMAGE=quay.io/taijilabs/rust_taiji-build-with-deps:nightly-2023-06-04
TOOLCHAIN_VERSION=nightly-2023-06-04
CONTAINER=taiji_test

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
