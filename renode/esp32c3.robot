*** Settings ***
Suite Setup                   Setup
Suite Teardown                Teardown
Resource                      ${RENODEKEYWORDS}

*** Test Cases ***
Should Boot Dual-Core Kernel And Detect Sandboxed U-Mode Fault
    # Load the emulation script
    Execute Command           include @renode/esp32c3.resc
    
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
    
    # Verify logical tasks initialization
    Wait For Line On Uart     Watchdog Task (Priority 0) started.
    Wait For Line On Uart     Task A (High) loop starting. Trying to lock Mutex 0...
    Wait For Line On Uart     Task B (Medium) is active. Waiting for IPC telemetry...
    Wait For Line On Uart     Task C (Low) starting. Locking Mutex 0...
    
    # Verify Watchdog detects Task B hang and triggers safety halt
    Wait For Line On Uart     Task B (Medium) simulating software hang: stopping check-ins!
    Wait For Line On Uart     Safe-parking CPU. Disabling interrupts.
