#!/bin/sh
# example notify script

# 1.
# For transaction received, mined(unconfirmed), and mined events:
#  $1 = "received", "confirmation", or "mined"
#  $2 = amount,
#  $3 = tx_id
#  $4 = message
#  $5 = source address public key
#  $6 = destination address public key
#  $7 = status
#  $8 = excess,
#  $9 = public_nonce,
# $10 = signature,
# $11 = number of confirmations (if applicable, otherwise empty string)
# $12 = direction

# 2.
# For transaction "sent" event, we only have the pending outbound transaction:
# $1 = "sent"
# $2 = amount,
# $3 = tx_id
# $4 = message
# $5 = destination address public key
# $6 = status,
# $7 = direction,

# 3.
# For a transaction "cancelled" event, if it was still pending - it would have the same args as 2. (with $5 as source address public key if inbound).
# If the cancelled tx was already out of pending state, the cancelled event will have the same args as 1.

# append the arguments to a log file
echo "$@" >>notify.log

# post to a webhook url
webhook_url=

# user
notify_user=

case $1 in
received)
    # msg="transaction $1 $notify_user \namount: **$2** \nmessage: **$4** \n*tx_id: ${3}* \n*excess: ${8}* \n[link](https://explore.tari.com/kernel/${9}/${10})"
    # curl -i -X POST -H 'Content-Type: application/json' -d '{"text": "'"${msg}"'"}' $webhook_url
    ;;
confirmation) ;;

mined) ;;

sent) ;;

cancelled) ;;
esac
