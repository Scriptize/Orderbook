import socket
import struct

def handle_log(conn):
    header = conn.recv(4)  # HH (2 bytes each) = 2+2 = 4 bytes
    if len(header) < 4: return False
    len_level, len_msg = struct.unpack("!HH", header)
    payload = conn.recv(len_level + len_msg)
    if len(payload) < len_level + len_msg: return False
    level = payload[:len_level].decode()
    message = payload[len_level:].decode()
    print(f"[LOG] {level}: {message}")
    return True

def handle_match(conn):
    header = conn.recv(12)  # HHIf = 2+2+4+4 = 12 bytes
    if len(header) < 12: return False
    len_side, len_symbol, quantity, price = struct.unpack("!HHIf", header)
    payload = conn.recv(len_side + len_symbol)
    if len(payload) < len_side + len_symbol: return False
    side = payload[:len_side].decode()
    symbol = payload[len_side:].decode()
    print(f"[MATCH] {side} {quantity} {symbol} @ ${price:.2f}")
    return True

def handle_price_update(conn):
    header = conn.recv(10)  # Hff = 2+4+4 = 10 bytes
    if len(header) < 10: return False
    len_symbol, old_price, new_price = struct.unpack("!Hff", header)
    symbol = conn.recv(len_symbol).decode()
    print(f"[PRICE] {symbol}: ${old_price:.2f} -> ${new_price:.2f}")
    return True

def main():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('localhost', 12345))
        s.listen(1)
        print("Server listening on port 12345...")
        conn, addr = s.accept()
        with conn:
            print(f"Connected by {addr}")
            while True:
                msg_type = conn.recv(1)
                if not msg_type:
                    break
                msg_type = struct.unpack("!B", msg_type)[0]
                try:
                    if msg_type == 1:
                        if not handle_log(conn): break
                    elif msg_type == 2:
                        if not handle_match(conn): break
                    elif msg_type == 3:
                        if not handle_price_update(conn): break
                    else:
                        print("Unknown message type:", msg_type)
                        break
                except Exception as e:
                    print("Error handling message:", e)
                    break

if __name__ == "__main__":
    main()
