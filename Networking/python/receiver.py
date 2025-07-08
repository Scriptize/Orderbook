import socket
import struct
import random
import math

def main():
    print("This will be our python reciever! (server)")

    # format_char_size = { me when i dont know theres a calcsize()
    #     "X": 0,
    #     "c": 1,
    #     "b": 1,
    #     "B": 1,
    #     "?": 1,
    #     "h": 2,
    #     "H": 2,
    #     "i": 4,
    #     "I": 4,
    #     "l": 4,
    #     "L": 4,
    #     "q": 8,
    #     "Q": 8,
    #     "n": 0,
    #     "N": 0,
    #     "e": 2,
    #     "f": 4,
    #     "d": 8,
    #     "s": 0,
    #     "p": 0,
    #     "P": 0
    # }
    
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        # creates/binds to a TCP stream
        try:
            s.bind(('localhost', 12345))
        except socket.error:
            print("Error: Cannot bind to a TCP stream.")
        
        s.listen(1) # 1 pending connections that the socket can queue
        print("Server is listening...")

        # accepts a TCP connection
        conn, addr = s.accept()
        with conn:
            print('Connected by', addr)
            while True:
                log_format_str = "!B3s"
                match_format_str = "!Bssisf"
                price_update_format_str = "!Bssff"

                log_buffer_size = struct.calcsize(log_format_str)
                match_buffer_size = struct.calcsize(match_format_str)
                price_update_buffer_size = struct.calcsize(price_update_format_str)

                # buffer_size = max(log_buffer_size, match_buffer_size, price_update_buffer_size)

                # unpacks a structured buffer
                type_buffer = conn.recv(1)
                if not type_buffer: break

                type_buffer.unpack("!B", type_buffer)
                if type_buffer == "1":
                    whole_buffer = conn.recv(log_buffer_size)
                    data = struct.unpack(log_format_str, whole_buffer)
                    # data = (1, log_type_str, level, message)
                    print(f"Log type: {data[1]}, level: {data[2]}, message: {data[3]}")
                elif type_buffer == "2":
                    whole_buffer = conn.recv(match_buffer_size)
                    data = struct.unpack(match_format_str, whole_buffer)
                    # data = (2, log_type_str, side, quantity, symbol, price)
                    print(f"Log type: {data[1]}, side: {data[2]}, quantity: {data[3]}, symbol: {data[4]}, price: {data[5]}")
                elif type_buffer == "3":
                    whole_buffer = conn.recv(price_update_buffer_size)
                    data = struct.unpack(price_update_format_str, whole_buffer)
                    # data = (3, log_type_str, symbol, old_price, new_price)
                    print(f"Log type: {data[1]}, symbol: {data[2]}, old_price: {data[3]}, new_price: {data[4]}")
                else:
                    print("Error: log type is invalid.")

                # ts better work as expected darren..

main()



