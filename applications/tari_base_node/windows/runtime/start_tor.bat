@set file_1=%TEMP%\tor1.txt
@set file_2=%TEMP%\tor2.txt
@set file_3=%TEMP%\tor3.txt

@call :QUERY_TOR_SERVICE
@if not defined TOR_RUNNING (
    @start tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --clientuseipv6 1 --log "notice stdout"
    @echo Tor service started on ports 9050 and 9051.
    @ping -n 10 localhost>nul
) else (
    @goto :END
)

@if not defined TOR_RUNNING (
    @call :QUERY_TOR_SERVICE
    @if not defined TOR_RUNNING (
        @echo Problem starting Tor services. Check if Tor is in the path.
        @pause
        @exit /b 10101
    )
)
@goto :END

:QUERY_TOR_SERVICE
@if exist %file_1% (
    del %file_1% /f/q
)
@if exist %file_2% (
    del %file_2% /f/q
)
@for /f "tokens=1,2,3,4,5*" %%i in ('netstat -aon ^| findstr ":9050" ^| findstr /i listening') do @echo %j %l & @tasklist | findstr %%m > %file_1%
@for /f "tokens=1,2,3,4,5*" %%i in ('netstat -aon ^| findstr ":9051" ^| findstr /i listening') do @echo %j %l & @tasklist | findstr %%m > %file_2%
@if exist %file_1% (
    @if exist %file_2% (
        @echo Found tor service listening on ports 9050 and 9051. Good.
        @set TOR_RUNNING=1
    ) else (
        @taskkill /im tor.exe /f > nul
        @set TOR_RUNNING=
    )
)
@goto :eof

:QUERY_TOR_STARTED
@if exist %file_3% (
    del %file_3% /f/q
)
@tasklist /fi "imagename eq tor.exe" > %file_3%
@if exist %file_3% (
    @set TOR_RUNNING=1
) else (
    @set TOR_RUNNING=
) 
@goto :eof

:END
@echo.
