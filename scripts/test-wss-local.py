#!/usr/bin/env python3
"""Local Rust server -> WSS -> Rust HTTP proxy -> controlled SSE/WS origins.
No Tencent account, TUN privileges, npm dependencies, or public network endpoints.
"""
import argparse
import base64
import concurrent.futures
import contextlib
import hashlib
import http.server
import json
import os
from pathlib import Path
import secrets
import socket
import socketserver
import ssl
import struct
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def wait_port(port, process):
    for _ in range(200):
        if process.poll() is not None:
            raise RuntimeError('Rust process exited; inspect the captured test output')
        try:
            with socket.create_connection(('127.0.0.1', port), timeout=.1):
                return
        except OSError:
            time.sleep(.05)
    raise TimeoutError('Local Rust listener did not start')


def read_exact(sock, size):
    data = b''
    while len(data) < size:
        part = sock.recv(size - len(data))
        if not part:
            raise EOFError('Truncated stream')
        data += part
    return data


def read_headers(sock):
    data = b''
    while not data.endswith(b'\r\n\r\n'):
        data += read_exact(sock, 1)
    return data.decode()


class Origin(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    request_queue_size = 128

    def finish_request(self, conn, addr):
        conn.settimeout(100)
        try:
            if conn.recv(1, socket.MSG_PEEK) == b'\x16':
                conn = self.tls.wrap_socket(conn, server_side=True)
            self.RequestHandlerClass(conn, addr, self)
        finally:
            conn.close()


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *_):
        pass

    def do_GET(self):
        if self.path.startswith('/sse'):
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Transfer-Encoding', 'chunked')
            self.end_headers()
            idle = self.server.idle if self.path.endswith('idle') else .2
            for i in range(2 if self.path.endswith('idle') else 5):
                if i:
                    time.sleep(idle)
                data = f'data: event-{i}\n\n'.encode()
                self.wfile.write(f'{len(data):x}\r\n'.encode() + data + b'\r\n')
                self.wfile.flush()
            self.wfile.write(b'0\r\n\r\n')
            self.wfile.flush()
            return
        if self.path != '/ws' or self.headers.get('Upgrade', '').lower() != 'websocket':
            self.send_error(400)
            return
        key = self.headers['Sec-WebSocket-Key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
        self.send_response(101)
        self.send_header('Connection', 'Upgrade')
        self.send_header('Upgrade', 'websocket')
        self.send_header('Sec-WebSocket-Accept', base64.b64encode(hashlib.sha1(key.encode()).digest()).decode())
        self.end_headers()
        try:
            while True:
                a, b = self.rfile.read(2)
                size = b & 127
                if size == 126:
                    size = struct.unpack('!H', self.rfile.read(2))[0]
                if size == 127:
                    size = struct.unpack('!Q', self.rfile.read(8))[0]
                if size > 1024 * 1024:
                    raise ValueError('Oversized frame')
                mask = self.rfile.read(4) if b & 128 else None
                data = self.rfile.read(size)
                if mask:
                    data = bytes(value ^ mask[i % 4] for i, value in enumerate(data))
                opcode = a & 15
                if opcode == 9:
                    opcode = 10
                self.wfile.write(frame(opcode, data, False))
                self.wfile.flush()
                if opcode == 8:
                    break
        except (OSError, ValueError):
            pass
        self.close_connection = True


def frame(opcode, data, masked):
    size = len(data)
    flag = 128 if masked else 0
    header = bytes([128 | opcode, flag | size]) if size < 126 else bytes([128 | opcode, flag | 126]) + struct.pack('!H', size) if size < 65536 else bytes([128 | opcode, flag | 127]) + struct.pack('!Q', size)
    if not masked:
        return header + data
    mask = secrets.token_bytes(4)
    return header + mask + bytes(value ^ mask[i % 4] for i, value in enumerate(data))


def receive_frame(sock):
    a, b = read_exact(sock, 2)
    size = b & 127
    if size == 126:
        size = struct.unpack('!H', read_exact(sock, 2))[0]
    if size == 127:
        size = struct.unpack('!Q', read_exact(sock, 8))[0]
    return a & 15, read_exact(sock, size)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--quick', action='store_true', help='Use a 1-second idle interval instead of 70 seconds')
    args = parser.parse_args()
    env = os.environ.copy()
    if os.name != 'nt' and os.uname().sysname == 'Darwin' and not env.get('LIBCLANG_PATH'):
        developer = subprocess.check_output(['xcode-select', '-p'], text=True).strip()
        env['LIBCLANG_PATH'] = developer + '/Toolchains/XcodeDefault.xctoolchain/usr/lib'
    if not args.skip_build:
        subprocess.run(['cargo', '+nightly', 'build', '--manifest-path', 'src-server/Cargo.toml', '--bin', 'cloudtun-server'], cwd=ROOT, env=env, check=True)
        subprocess.run(['cargo', '+nightly', 'build', '--manifest-path', 'src-proxy/Cargo.toml', '--features', 'bin', '--bin', 'cloudtun'], cwd=ROOT, env=env, check=True)
    with tempfile.TemporaryDirectory(prefix='cloudtun-wss-e2e-') as temporary:
        directory = Path(temporary)
        processes = []
        origin = None
        failed = True
        with contextlib.ExitStack() as stack:
            try:
                cloud_port, proxy_port = free_port(), free_port()
                env['CLOUDTUN_ACCESS_TOKEN'] = secrets.token_hex(32)
                def spawn(command, name):
                    log = stack.enter_context(open(directory / (name + '.log'), 'w+'))
                    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
                    processes.append(process)
                    return process
                server = spawn([str(ROOT/'src-server/target/debug/cloudtun-server'), '--standalone', '--ip', '127.0.0.1', '--port', str(cloud_port), '--data-dir', str(directory/'server')], 'server')
                wait_port(cloud_port, server)
                identity = json.loads((directory/'server/identity.json').read_text())
                cert, key = directory/'origin.pem', directory/'origin.key'
                cert.write_text(ssl.DER_cert_to_PEM_cert(bytes(identity['certificate'])))
                key.write_text('-----BEGIN PRIVATE KEY-----\n' + base64.encodebytes(bytes(identity['key'])).decode() + '-----END PRIVATE KEY-----\n')
                key.chmod(0o600)
                context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
                context.load_cert_chain(cert, key)
                origin = Origin(('127.0.0.1', 0), Handler)
                origin.tls = context
                origin.idle = 1 if args.quick else 70
                threading.Thread(target=origin.serve_forever, daemon=True).start()
                origin_port = origin.server_address[1]
                client = spawn([str(ROOT/'src-proxy/target/debug/cloudtun'), '--server-ip', '127.0.0.1', '--server-port', str(cloud_port), '--instance-id', 'local-test-instance', '--trust-dir', str(directory/'trust'), '--local-ip', '127.0.0.1', '--local-port', str(proxy_port)], 'client')
                wait_port(proxy_port, client)
                # This context is only for the controlled origin's self-signed test certificate.
                # The CloudTun WSS hop uses the Rust TOFU verifier, not this Python context.
                origin_context = ssl._create_unverified_context()
                def sse(scheme, idle=False):
                    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=origin_context))
                    request = urllib.request.Request(f'{scheme}://127.0.0.1:{origin_port}/sse' + ('idle' if idle else ''))
                    request.set_proxy(f'127.0.0.1:{proxy_port}', 'http')
                    started = time.monotonic()
                    with opener.open(request, timeout=100) as response:
                        times = [time.monotonic() - started for line in response if line.startswith(b'data:')]
                    assert len(times) == (2 if idle else 5)
                    assert times[-1] - times[0] >= (origin.idle - .5 if idle else .3), times
                    print(f'PASS {scheme} SSE' + (' idle' if idle else '') + ': ' + str([round(t, 2) for t in times]), flush=True)
                def websocket(scheme, idle=False):
                    sock = socket.create_connection(('127.0.0.1', proxy_port), timeout=10)
                    sock.settimeout(100)
                    if scheme == 'wss':
                        sock.sendall(f'CONNECT 127.0.0.1:{origin_port} HTTP/1.1\r\nHost: 127.0.0.1:{origin_port}\r\n\r\n'.encode())
                        assert read_headers(sock).startswith('HTTP/1.1 200')
                        sock = origin_context.wrap_socket(sock, server_hostname='127.0.0.1')
                    target = '/ws' if scheme == 'wss' else f'{scheme}://127.0.0.1:{origin_port}/ws'
                    sock.sendall(f'GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{origin_port}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n'.encode())
                    assert read_headers(sock).startswith('HTTP/1.1 101')
                    for opcode, data in [(1, b'hello'), (2, secrets.token_bytes(128*1024)), (9, b'heartbeat')]:
                        sock.sendall(frame(opcode, data, True))
                        assert receive_frame(sock) == (10 if opcode == 9 else opcode, data)
                    if idle:
                        time.sleep(origin.idle)
                        sock.sendall(frame(1, b'after-idle', True))
                        assert receive_frame(sock) == (1, b'after-idle')
                    sock.sendall(frame(8, b'\x03\xe8', True))
                    assert receive_frame(sock)[0] == 8
                    sock.close()
                    print(f'PASS {scheme} WebSocket' + (' idle' if idle else '') + ': text, 128 KiB binary, pong, close', flush=True)
                jobs = [(sse, ('http',)), (sse, ('https',)), (websocket, ('http',)), (websocket, ('ws',)), (websocket, ('wss',)), (sse, ('http', True)), (sse, ('https', True)), (websocket, ('ws', True)), (websocket, ('wss', True))]
                with concurrent.futures.ThreadPoolExecutor(max_workers=9) as executor:
                    futures = [executor.submit(fn, *values) for fn, values in jobs]
                    for future in concurrent.futures.as_completed(futures):
                        future.result()
                assert list((directory/'trust').glob('*.json')), 'WSS TOFU trust was not persisted'
                print('All 9 local full-chain tests passed. No cloud resources used.', flush=True)
                failed = False
            finally:
                for process in reversed(processes):
                    if process.poll() is None:
                        process.terminate()
                for process in processes:
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if origin:
                    origin.shutdown()
                    origin.server_close()
                if failed or any(process.returncode not in (None, 0, -15) for process in processes):
                    for log in directory.glob('*.log'):
                        print(log.name + ':\n' + log.read_text(), flush=True)


if __name__ == '__main__':
    main()
