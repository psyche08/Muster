@echo off
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy RemoteSigned -File "%~dp0Apply-Update.ps1" %*
exit /b %ERRORLEVEL%
