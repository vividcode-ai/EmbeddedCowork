@echo off
rem EmbeddedCowork update bootstrapper
rem Arguments: %1 = old app PID to wait for, %2 = installer path
rem Waits for the old app process to fully exit, then runs the installer.

:WAIT_LOOP
rem tasklist exit code is unreliable (0 even if PID not found). Use find on output.
tasklist /FI "PID eq %1" /NH 2>NUL | find /I "%1" >NUL
if %errorlevel% EQU 0 (
    timeout /t 1 /nobreak >NUL
    goto WAIT_LOOP
)

rem Old app is gone. Run the installer in update mode.
"%~2" /S --updated

rem Self-cleanup
del "%~f0" 2>NUL
