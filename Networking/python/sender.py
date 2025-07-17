import socket
import struct
import random
import time

mock_data = [
 {"type":"price_update","symbol":"BTC/USD","old_price":67203.00,"new_price":67210.50},
 {"type":"price_update","symbol":"ETH/USD","old_price":3212.45,"new_price":3217.10},
 {"type":"price_update","symbol":"SPY","old_price":528.75,"new_price":529.00},
 {"type":"match","side":"BUY","quantity":100,"symbol":"AAPL","price":172.34},
 {"type":"match","side":"SELL","quantity":5,"symbol":"BTC","price":67200.00},
 {"type":"match","side":"BUY","quantity":50,"symbol":"TSLA","price":187.90},
 {"type":"log","level":"INFO","message":"TCP server started on port 9001"},
 {"type":"log","level":"DEBUG","message":"Received order: ID#14352 (BUY 25 ETH @ $3,200)"},
 {"type":"log","level":"INFO","message":"Trade executed: Order#14352 matched with Order#14349"}
]

log_type_to_num = {"log": 1, "match": 2, "price_update": 3}

def send_log(sock, data):
    level = data["level"].encode("utf-8")
    message = data["message"].encode("utf-8")
    header = struct.pack("!BHH", log_type_to_num["log"], len(level), len(message))
    sock.sendall(header + level + message)

def send_match(sock, data):
    side = data["side"].encode("utf-8")
    symbol = data["symbol"].encode("utf-8")
    quantity = data["quantity"]
    price = data["price"]
    header = struct.pack("!BHHIf", log_type_to_num["match"], len(side), len(symbol), quantity, price)
    sock.sendall(header + side + symbol)

def send_price_update(sock, data):
    symbol = data["symbol"].encode("utf-8")
    old_price = data["old_price"]
    new_price = data["new_price"]
    header = struct.pack("!B Hff", log_type_to_num["price_update"], len(symbol), old_price, new_price)
    sock.sendall(header + symbol)

def main():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.connect(('localhost', 12345))
        except socket.error:
            print("Error: Cannot connect to server.")
            return

        for _ in range(100):
            data = random.choice(mock_data)
            try:
                if data["type"] == "log":
                    send_log(s, data)
                elif data["type"] == "match":
                    send_match(s, data)
                elif data["type"] == "price_update":
                    send_price_update(s, data)
                time.sleep(0.1)
            except Exception as e:
                print("Send failed:", e)
                break

if __name__ == "__main__":
    main()
