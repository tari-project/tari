@taskkill /im tor.exe /f > nul
@tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --clientuseipv6 1 --log "notice stdout"
@pause
