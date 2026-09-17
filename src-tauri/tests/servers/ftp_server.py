import datetime
import os
import sys

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import FTPHandler, TLS_FTPHandler
from pyftpdlib.servers import FTPServer

if os.environ.get("VAULTRA_FTP_DEBUG"):
    import logging
    from pyftpdlib.log import config_logging
    config_logging(level=logging.DEBUG)

ROOT = os.path.abspath(sys.argv[1])
PLAIN_PORT = int(sys.argv[2])
TLS_PORT = int(sys.argv[3])
os.makedirs(ROOT, exist_ok=True)


def write_self_signed(directory):
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost"), x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Vaultra Test")])
    now = datetime.datetime.now(datetime.timezone.utc)
    cert = (
        x509.CertificateBuilder()
        .subject_name(name)
        .issuer_name(name)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(now - datetime.timedelta(days=1))
        .not_valid_after(now + datetime.timedelta(days=30))
        .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost")]), critical=False)
        .sign(key, hashes.SHA256())
    )
    pem_path = os.path.join(directory, "test.pem")
    with open(pem_path, "wb") as f:
        f.write(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.TraditionalOpenSSL, serialization.NoEncryption()))
        f.write(cert.public_bytes(serialization.Encoding.PEM))
    return pem_path


authorizer = DummyAuthorizer()
authorizer.add_user("test", "secret", ROOT, perm="elradfmwMT")
authorizer.add_anonymous(ROOT)

plain_handler = FTPHandler
plain_handler.authorizer = authorizer
plain_handler.passive_ports = range(30000, 30100)

pem = write_self_signed(os.path.dirname(ROOT))
tls_handler = TLS_FTPHandler
tls_handler.certfile = pem
tls_handler.authorizer = authorizer
tls_handler.tls_control_required = True
tls_handler.tls_data_required = True
tls_handler.passive_ports = range(30100, 30200)

plain = FTPServer(("127.0.0.1", PLAIN_PORT), plain_handler)
tls = FTPServer(("127.0.0.1", TLS_PORT), tls_handler)
print(f"READY {PLAIN_PORT} {TLS_PORT}", flush=True)
plain.serve_forever()
