*** Settings ***
Suite Setup                   Setup
Suite Teardown                Teardown
Resource                      ${RENODEKEYWORDS}

*** Test Cases ***
Should Boot Dual-Core Kernel And Detect Sandboxed U-Mode Fault
    # Load the emulation script
    Execute Command           $workspace = @${CURDIR}/..
    Execute Command           include @${CURDIR}/esp32c3.resc

    # Attach the terminal tester to the SEGGER RTT virtual console
    Create Terminal Tester    sysbus.segger_rtt    timeout=15
    
    # Start emulation
    Start Emulation
    
    # Verify Core 0 Boot and Secure Boot validation
    Wait For Line On Uart     Booting Cerberus-OS kernel on Core 0...
    Wait For Line On Uart     SBL: Secure Boot Verification SUCCESSFUL.
    Wait For Line On Uart     Core 0: Heartbeat timer armed. Releasing Core 1...
    
    # Verify Core 1 Boot and Scheduler launch
    Wait For Line On Uart     Core 1: Booted and timer armed. Launching scheduler...

    # Verify logical tasks initialization. Core 1's tasks (B, C) and Core 0's
    # (Watchdog, A) boot concurrently and interleave on the shared console;
    # this is the order the current build reliably produces, not a strict
    # cross-core ordering guarantee.
    Wait For Line On Uart     Task B (Medium) is active. Waiting for IPC telemetry...
    Wait For Line On Uart     Task C (Low) starting. Locking Mutex 0...
    Wait For Line On Uart     Watchdog Task (Priority 0) started.
    Wait For Line On Uart     Task A (High) loop starting. Trying to lock Mutex 0...

    # Verify Day 7 sequential fault injection and containment. Fault messages
    # match the kernel's actual containment log format, not a placeholder.
    Wait For Line On Uart     [TEST RUNNER] Running Test 1: PMP Stack Violation
    Wait For Line On Uart     Task 'Task C' triggered exception 'Load Access Fault'
    Wait For Line On Uart     [WATCHDOG] Resetting Task C stack and restarting to run Test 2

    Wait For Line On Uart     [TEST RUNNER] Running Test 2: Privilege Violation
    Wait For Line On Uart     Task 'Task C' triggered exception 'Illegal Instruction'
    Wait For Line On Uart     [WATCHDOG] Resetting Task C stack and restarting to run Test 3

    Wait For Line On Uart     [TEST RUNNER] Running Test 3: Watchdog Timeout
    Wait For Line On Uart     WATCHDOG FAILURE: Task 'Task C' failed to check in!
    Wait For Line On Uart     WATCHDOG: Terminating task 'Task C' to restore availability.
    Wait For Line On Uart     Syscall 8: Terminating task 'Task C'
