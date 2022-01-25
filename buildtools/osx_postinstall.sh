#! /bin/bash

# logs are in /var/log/install.log
TARI_LOG=/tmp/tari.log
TARI_DIR="$HOME/.tari/"

date >$TARI_LOG
if [ -d "$TARI_DIR" ]; then
    echo "removing ~/.tari" >>$TARI_LOG
    rm -rf "$TARI_DIR"
fi
echo "creating ~/.tari" >>$TARI_LOG
mkdir "$HOME"/.tari/ || exit 1
{
    echo "whoami: $(whoami)"
    echo "home dir: $HOME"
    echo "user: $USER"
    echo "copy /tmp/tari/ to ~/.tari"
} >>$TARI_LOG
cp -R /tmp/tari/ "$HOME"/.tari/ || exit 1
echo "chown" >>$TARI_LOG
chown -R "$USER":staff "$HOME"/.tari/ || exit 1
echo "rm scripts" >>$TARI_LOG
rm -rf "$HOME"/.tari/scripts/ || exit 1

echo "done! exiting" >>$TARI_LOG
exit 0
