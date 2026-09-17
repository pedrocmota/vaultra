import os
import socket
import sys
import threading

import paramiko
from paramiko import SFTPAttributes, SFTPHandle, SFTPServer, SFTPServerInterface
from paramiko.sftp import SFTP_FAILURE, SFTP_OK, SFTP_NO_SUCH_FILE, SFTP_PERMISSION_DENIED

ROOT = os.path.abspath(sys.argv[1])
PORT = int(sys.argv[2])
USERNAME = "test"
PASSWORD = "secret"
CODE = "123456"


class Server(paramiko.ServerInterface):
    def check_auth_password(self, username, password):
        if username == USERNAME and password == PASSWORD:
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def check_auth_interactive(self, username, submethods):
        if username != "totp":
            return paramiko.AUTH_FAILED
        query = paramiko.InteractiveQuery()
        query.add_prompt("Verification code: ", False)
        return query

    def check_auth_interactive_response(self, responses):
        if responses and responses[0] == CODE:
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def get_allowed_auths(self, username):
        return "keyboard-interactive,password"

    def check_channel_request(self, kind, chanid):
        return paramiko.OPEN_SUCCEEDED


class Handle(SFTPHandle):
    def stat(self):
        try:
            return SFTPAttributes.from_stat(os.fstat(self.readfile.fileno()))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)

    def chattr(self, attr):
        try:
            SFTPServer.set_file_attr(self.filename, attr)
            return SFTP_OK
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)


class Sftp(SFTPServerInterface):
    def _real(self, path):
        path = self.canonicalize(path)
        return os.path.join(ROOT, path.lstrip("/").replace("/", os.sep))

    def list_folder(self, path):
        real = self._real(path)
        try:
            out = []
            for name in os.listdir(real):
                attr = SFTPAttributes.from_stat(os.lstat(os.path.join(real, name)))
                attr.filename = name
                out.append(attr)
            return out
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)

    def stat(self, path):
        try:
            return SFTPAttributes.from_stat(os.stat(self._real(path)))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)

    def lstat(self, path):
        try:
            return SFTPAttributes.from_stat(os.lstat(self._real(path)))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)

    def open(self, path, flags, attr):
        real = self._real(path)
        binary_flag = getattr(os, "O_BINARY", 0)
        flags |= binary_flag
        mode = getattr(attr, "st_mode", None) or 0o666
        try:
            fd = os.open(real, flags, mode)
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        if flags & os.O_WRONLY:
            fstr = "ab" if flags & os.O_APPEND else "wb"
        elif flags & os.O_RDWR:
            fstr = "a+b" if flags & os.O_APPEND else "r+b"
        else:
            fstr = "rb"
        try:
            f = os.fdopen(fd, fstr)
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        handle = Handle(flags)
        handle.filename = real
        handle.readfile = f
        handle.writefile = f
        return handle

    def remove(self, path):
        try:
            os.remove(self._real(path))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def rename(self, oldpath, newpath):
        try:
            os.rename(self._real(oldpath), self._real(newpath))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def posix_rename(self, oldpath, newpath):
        try:
            os.replace(self._real(oldpath), self._real(newpath))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def mkdir(self, path, attr):
        try:
            os.mkdir(self._real(path))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def rmdir(self, path):
        try:
            os.rmdir(self._real(path))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def chattr(self, path, attr):
        try:
            SFTPServer.set_file_attr(self._real(path), attr)
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def symlink(self, target_path, path):
        try:
            os.symlink(target_path, self._real(path))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)
        return SFTP_OK

    def readlink(self, path):
        try:
            return os.readlink(self._real(path))
        except OSError as e:
            return SFTPServer.convert_errno(e.errno)


def serve(client):
    transport = paramiko.Transport(client)
    transport.add_server_key(HOST_KEY)
    transport.set_subsystem_handler("sftp", SFTPServer, Sftp)
    transport.start_server(server=Server())
    while transport.is_active():
        transport.join(0.5)


HOST_KEY = paramiko.RSAKey.generate(2048)
os.makedirs(ROOT, exist_ok=True)
listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", PORT))
listener.listen(8)
print(f"READY {PORT}", flush=True)
while True:
    client, _ = listener.accept()
    threading.Thread(target=serve, args=(client,), daemon=True).start()
