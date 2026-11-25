@echo off
setlocal
set "MUSTER_PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if exist "%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe" set "MUSTER_PS=%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
"%MUSTER_PS%" -NoProfile -ExecutionPolicy RemoteSigned -File "%~dp0Apply-Update.ps1" %*
exit /b %ERRORLEVEL%
