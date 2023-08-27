#! /bin/bash

# logs are in /var/log/install.log
TARI_LOG=/tmp/taiji.log
TARI_DIR="$HOME/.taiji/"

date >$TARI_LOG
if [ -d "$TARI_DIR" ]; then
    echo "removing ~/.taiji" >>$TARI_LOG
    rm -rf "$TARI_DIR"
fi
echo "creating ~/.taiji" >>$TARI_LOG
mkdir "$HOME"/.taiji/ || exit 1
{
    echo "whoami: $(whoami)"
    echo "home dir: $HOME"
    echo "user: $USER"
    echo "copy /tmp/taiji/ to ~/.taiji"
} >>$TARI_LOG
cp -R /tmp/taiji/ "$HOME"/.taiji/ || exit 1
echo "chown" >>$TARI_LOG
chown -R "$USER":staff "$HOME"/.taiji/ || exit 1
echo "rm scripts" >>$TARI_LOG
rm -rf "$HOME"/.taiji/scripts/ || exit 1

echo "done! exiting" >>$TARI_LOG
exit 0
