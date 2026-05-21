@echo off
rem EmbeddedCowork update bootstrapper
rem Arguments: %1 = old app PID to wait for, %2 = installer path
rem Waits for the old app process to fully exit, then runs the installer.

:WAIT_LOOP
rem tasklist exits 0 if the PID exists, 1 if not
tasklist /FI "PID eq %1" >NUL 2>NUL
if %errorlevel% EQU 0 (
    timeout /t 1 /nobreak >NUL
    goto WAIT_LOOP
)

rem Old app is gone. Run the new installer.
"%~2" /S

rem Self-cleanup
del "%~f0" 2>NUL
