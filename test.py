# Send a message to /tmp/example.sock
import socket
import sys
import time
from datetime import timedelta

# Create a UDS socket
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)

# Connect the socket to the port where the server is listening
server_address = '/tmp/example.sock'
print('connecting to {}'.format(server_address))
try:
    sock.connect(server_address)
except socket.error as msg:
    print(msg)
    sys.exit(1)

try:
    # Send data
    message = 'This is the message.  It will be repeated.'
    print('sending {!r}'.format(message))
    start_time = time.monotonic()
    sock.sendall(message.encode())

    # Look for the response
    amount_received = 0
    amount_expected = len(message)

    # while amount_received < amount_expected:
    data = sock.recv(16)
    amount_received += len(data)
    print('received {!r}'.format(data))

    end_time = time.monotonic()

finally:
    print('closing socket')
    sock.close()

print('Elapsed time: {}'.format(timedelta(seconds=end_time - start_time)))