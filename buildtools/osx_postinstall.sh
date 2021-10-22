#! /bin/bash

whoami
echo "$HOME"
echo "$USER"
mkdir "$HOME"/.tari/ || exit 1
cp -R /tmp/tari/ "$HOME"/.tari/ || exit 1
chown -R "$USER":staff "$HOME"/.tari/ || exit 1

exit 0