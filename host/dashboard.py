import streamlit as st
import socket
import threading
import json
import time
import pandas as pd
import plotly.express as px
from streamlit.runtime.scriptrunner import add_script_run_ctx

st.set_page_config(
    page_title="Cerberus-OS Telemetry Dashboard",
    layout="wide",
    initial_sidebar_state="expanded"
)

# Dark theme styling custom CSS
st.markdown("""
<style>
    .reportview-container {
        background-color: #0e1117;
        color: #ffffff;
    }
    .metric-card {
        background-color: #1f2937;
        border-radius: 8px;
        padding: 15px;
        border: 1px solid #374151;
        margin-bottom: 10px;
    }
    .metric-title {
        color: #9ca3af;
        font-size: 14px;
        font-weight: 500;
    }
    .metric-value {
        color: #f3f4f6;
        font-size: 24px;
        font-weight: 700;
        margin-top: 5px;
    }
</style>
""", unsafe_allow_html=True)

# Initialize Session State
if "events" not in st.session_state:
    st.session_state.events = []
if "connected" not in st.session_state:
    st.session_state.connected = False

# Background socket reader
def socket_reader():
    while True:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.connect(("127.0.0.1", 8765))
            st.session_state.connected = True
            print("Connected to Telemetry Broker!")
            
            f = s.makefile('r')
            for line in f:
                try:
                    event = json.loads(line.strip())
                    st.session_state.events.append(event)
                    if len(st.session_state.events) > 5000:
                        st.session_state.events.pop(0)
                except Exception as e:
                    print("Error parsing event line:", e)
            st.session_state.connected = False
            s.close()
        except Exception as e:
            st.session_state.connected = False
            print("Broker connection error, retrying in 2s...", e)
            time.sleep(2)

# Start reader thread if not running. Streamlit only attaches its script-run
# context to threads it knows about; without add_script_run_ctx, this
# thread's st.session_state writes silently don't reach the session that
# actually renders in the browser, and the dashboard hangs on "Waiting for
# telemetry data..." forever even with a live broker connection.
if "reader_thread" not in st.session_state:
    t = threading.Thread(target=socket_reader)
    add_script_run_ctx(t)
    t.daemon = True
    t.start()
    st.session_state.reader_thread = t

# Dashboard Layout
st.title("Cerberus-OS Real-time Telemetry Dashboard")
st.markdown("---")

# Connection status in sidebar
with st.sidebar:
    st.header("Connection Settings")
    if st.session_state.connected:
        st.success("Connected to Broker on localhost:8765")
    else:
        st.error("Disconnected from Broker. Retrying...")
        
    st.markdown("### System Configuration")
    st.write("**Architecture:** Dual-Core RISC-V (ESP32-C3)")
    st.write("**Scheduler:** ARINC 653 Partitioned")
    st.write("**Cores:** Hart 0, Hart 1")
    
    st.markdown("### Controls")
    if st.button("Clear Event History"):
        st.session_state.events = []
        st.rerun()

# Process events
events = st.session_state.events
df = pd.DataFrame(events)

if df.empty:
    st.info("Waiting for telemetry data... Run the Cerberus-OS emulator to stream events.")
    time.sleep(1)
    st.rerun()

# ----------------- Metrics & Overview -----------------
col1, col2, col3, col4 = st.columns(4)

# Total events
with col1:
    st.markdown('<div class="metric-card"><div class="metric-title">Total Events</div><div class="metric-value">{}</div></div>'.format(len(df)), unsafe_allow_html=True)

# Context switches
df_swaps = df[df["type"] == "TaskSwap"]
with col2:
    st.markdown('<div class="metric-card"><div class="metric-title">Context Switches</div><div class="metric-value">{}</div></div>'.format(len(df_swaps)), unsafe_allow_html=True)

# IPC Transfers
df_ipc = df[df["type"] == "IpcTransfer"]
with col3:
    st.markdown('<div class="metric-card"><div class="metric-title">IPC Flows</div><div class="metric-value">{}</div></div>'.format(len(df_ipc)), unsafe_allow_html=True)

# Fault Containments
df_faults = df[df["type"] == "FaultInterception"]
with col4:
    st.markdown('<div class="metric-card"><div class="metric-title">Faults Contained</div><div class="metric-value">{}</div></div>'.format(len(df_faults)), unsafe_allow_html=True)

st.markdown("### CPU Scheduling & Task Swaps")

# Context switch cycle metrics
if not df_swaps.empty:
    task_names = {
        0: "Task A (Secure Signing)",
        1: "Task B (Sensor/CAN RX)",
        2: "vHSM Partition",
        3: "Task C (Fault Injector)",
        10: "Watchdog Task",
        31: "Idle Task"
    }
    
    avg_cycles = int(df_swaps["cycles"].diff().dropna().mean()) if len(df_swaps) > 1 else 0
    st.metric(label="Average Context Switch Cycles", value=f"{avg_cycles:,} cycles")
    
    df_plot = df_swaps.copy()
    df_plot["task_name"] = df_plot["to"].map(task_names).fillna("Unknown")
    df_plot["timestamp_relative"] = df_plot["timestamp"] - df_plot["timestamp"].iloc[0]
    
    fig = px.scatter(
        df_plot,
        x="timestamp_relative",
        y="task_name",
        color="task_name",
        title="Real-time Task Schedule Timeline",
        labels={"timestamp_relative": "Time (seconds)", "task_name": "Active Task"},
        height=350,
        template="plotly_dark"
    )
    st.plotly_chart(fig, use_container_width=True)

# ----------------- IPC Flows & Faults -----------------
col_left, col_right = st.columns(2)

with col_left:
    st.markdown("### Zero-Copy IPC Channels")
    if not df_ipc.empty:
        df_ipc_plot = df_ipc.copy()
        df_ipc_plot["endpoint_name"] = df_ipc_plot["endpoint"].map({
            0: "EP 0 (vHSM Requests)",
            1: "EP 1 (vHSM Signatures)",
            2: "EP 2 (CAN Data Flow)",
        }).fillna("Endpoint")
        
        fig_ipc = px.bar(
            df_ipc_plot,
            x="endpoint_name",
            y="bytes",
            color="endpoint_name",
            title="IPC Payload Transfers",
            template="plotly_dark",
            height=300
        )
        st.plotly_chart(fig_ipc, use_container_width=True)
    else:
        st.info("No IPC messages recorded yet.")

with col_right:
    st.markdown("### U-Mode Containment Logs")
    if not df_faults.empty:
        df_faults_plot = df_faults.copy()
        df_faults_plot["exception_name"] = df_faults_plot["cause"].map({
            1: "Instruction Access Fault (PMP)",
            2: "Illegal Instruction (Privilege)",
            5: "Load Access Fault (PMP)",
            7: "Store Access Fault (PMP)",
        }).fillna("Other Fault")
        
        st.dataframe(
            df_faults_plot[["timestamp", "exception_name", "pc"]].tail(5),
            use_container_width=True
        )
        
        fig_faults = px.pie(
            df_faults_plot,
            names="exception_name",
            title="Distribution of Intercepted Faults",
            template="plotly_dark",
            height=250
        )
        st.plotly_chart(fig_faults, use_container_width=True)
    else:
        st.success("Zero faults detected. Core integrity is nominal.")

# Auto-refresh loop
time.sleep(0.5)
st.rerun()
