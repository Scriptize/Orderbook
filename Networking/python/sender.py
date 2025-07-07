import socket
import struct
import random

# hard coded the mock_data for testing
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

num_to_log_types = {
    1: "log",
    2: "match",
    3: "price_update"
}

log_type_to_num = {
    "log": 1,
    "match": 2,
    "price_update": 3
}

def main():
    print("This will be our python sender! (client)")
    
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        # connects to a TCP stream
        try:
            s.connect(('localhost', 12345))
        except socket.error:
            print("Error: Cannot connect to a TCP stream.")
        
        # TODO: need to work on the boolean condition for the while loop
        while True:
            # generates the log data
            random_log_event = random.choice(mock_data)

            # Pack data using struct (network byte order: '!' + 'H' (ushort), 'f' (float), '?' (bool))
            # packs it into a buffer
            if random_log_event["type"] == "log":
                log_type = "log" # type is string
                level = random_log_event["level"] # type is a string
                message = random_log_event["message"] # type is a string
                try:
                    buffer = struct.pack('!B3s', 1, log_type, level, message)
                except struct.error:
                    print("Error: Cannot pack data into buffer.")

            
            if random_log_event["type"] == "match":
                log_type = "match" # type is string
                side = random_log_event["side"] # type is string
                quantity = random_log_event["quantity"] # type is int?
                symbol = random_log_event["symbol"] # type is string
                price = random_log_event["price"] # type is float
                try:
                    buffer = struct.pack('!Bssisf', 2, log_type, side, quantity, symbol, price)
                except struct.error:
                    print("Error: Cannot pack data into buffer.")

            if random_log_event["type"] == "price_update":
                log_type = "price_update" # type is string
                symbol = random_log_event["symbol"] # type is string
                old_price = random_log_event["old_price"] # type is float
                new_price = random_log_event["new_price"] # type is float
                try:
                    buffer = struct.pack('!Bssff', 3, log_type, symbol, old_price, new_price)
                except struct.error:
                    print("Error: Cannot pack data into buffer.")

            # sends it over a TCP stream
            try:
                s.sendall(buffer)
            except socket.error:
                print("Error: Cannot send buffer.")

main()