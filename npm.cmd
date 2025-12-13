@echo off
REM Helper script to run npm commands with the correct PATH

set PATH=C:\Program Files\nodejs;C:\Program Files\Git\cmd;%PATH%

npm %*
