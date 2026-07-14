# host/telemetry_broker.py
import socket
import threading
import time
import json
import os
import io

PORT = 8765
FILE_PATH = "telemetry.bin"

clients = []
clients_lock = threading.Lock()

def read_varint(stream):
    value = 0
    shift = 0
    while True:
        b = stream.read(1)
        if not b:
            raise EOFError
        byte = ord(b)
        value |= (byte & 0x7F) << shift
        if (byte & 0x80) == 0:
            break
        shift += 7
    return value

def parse_event(stream):
    try:
        tag = read_varint(stream)
    except EOFError:
        return None

    if tag == 0:
        from_b = stream.read(1)
        if not from_b: return None
        from_val = ord(from_b)
        
        to_b = stream.read(1)
        if not to_b: return None
        to_val = ord(to_b)
        
        cycles = read_varint(stream)
        return {
            "type": "TaskSwap",
            "from": from_val,
            "to": to_val,
            "cycles": cycles
        }
    elif tag == 1:
        ep_b = stream.read(1)
        if not ep_b: return None
        ep_val = ord(ep_b)
        
        bytes_b = stream.read(1)
        if not bytes_b: return None
        bytes_val = ord(bytes_b)
        return {
            "type": "IpcTransfer",
            "endpoint": ep_val,
            "bytes": bytes_val
        }
    elif tag == 2:
        cause = read_varint(stream)
        pc = read_varint(stream)
        return {
            "type": "FaultInterception",
            "cause": cause,
            "pc": pc
        }
    return None

def broadcast(event_json):
    with clients_lock:
        disconnected = []
        for client in clients:
            try:
                client.sendall((event_json + "\n").encode('utf-8'))
            except Exception:
                disconnected.append(client)
        for client in disconnected:
            clients.remove(client)

def tail_file():
    # Wait for the file to be created
    while not os.path.exists(FILE_PATH):
        time.sleep(0.1)
        
    print(f"Tracking telemetry from {FILE_PATH}...")
    with open(FILE_PATH, "rb") as f:
        stream = io.BytesIO()
        while True:
            chunk = f.read(4096)
            if not chunk:
                time.sleep(0.05)
                if f.tell() > os.path.getsize(FILE_PATH):
                    f.seek(0)
                continue
                
            stream.write(chunk)
            stream.seek(0)
            
            while True:
                pos = stream.tell()
                try:
                    event = parse_event(stream)
                except Exception:
                    event = None
                    
                if event is not None:
                    event["timestamp"] = time.time()
                    event_json = json.dumps(event)
                    print("Event:", event_json)
                    broadcast(event_json)
                else:
                    stream.seek(pos)
                    remaining = stream.read()
                    stream = io.BytesIO()
                    stream.write(remaining)
                    stream.seek(0)
                    break

def handle_client(conn, addr):
    print(f"Client connected from {addr}")
    with clients_lock:
        clients.append(conn)

def server_loop():
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", PORT))
    s.listen(5)
    print(f"Telemetry Broker listening on port {PORT}...")
    while True:
        conn, addr = s.accept()
        t = threading.Thread(target=handle_client, args=(conn, addr))
        t.daemon = True
        t.start()

if __name__ == "__main__":
    t_tail = threading.Thread(target=tail_file)
    t_tail.daemon = True
    t_tail.start()
    
    server_loop()
