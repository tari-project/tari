#!/bin/bash
# generate self-signed certificate for Tari server
# usage: bash scripts/keygen.sh [hostname]
set -e

HOSTNAME=${1:-"tari.local"}
KEY_LOCATION="applications/minotari_app_grpc/keys"
mkdir --parent "${KEY_LOCATION}"

# Tari CA certificate configuration
cat > ca-cert.conf << EOT
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no
utf8 = yes

[req_distinguished_name]
C = US
ST = California
L = Oakland
O = Tari
CN = Tari CA Cert

[v3_req]
basicConstraints = critical, CA:TRUE, pathlen: 0
authorityKeyIdentifier = keyid, issuer
subjectKeyIdentifier = hash
keyUsage = critical, cRLSign, digitalSignature, keyCertSign
EOT

# Tari server certificate configuration
cat > server-csr.conf << EOT
[req]
distinguished_name = req_distinguished_name
prompt = no
utf8 = yes

[req_distinguished_name]
C = US
ST = California
L = Oakland 
O = Tari
CN = Tari Server Cert
EOT

# Standard server X509v3 extensions - https://www.openssl.org/docs/man3.0/man5/x509v3_config.html
cat > server-ext.conf << EOT
basicConstraints = critical, CA:FALSE
authorityKeyIdentifier = keyid, issuer
subjectKeyIdentifier = hash
keyUsage = critical, nonRepudiation, digitalSignature, keyEncipherment, keyAgreement
extendedKeyUsage = critical, serverAuth
EOT
echo "subjectAltName = DNS: ${HOSTNAME}" >> server-ext.conf

# Generate a Tari CA private key
openssl ecparam -name prime256v1 -genkey -noout -out "${KEY_LOCATION}/tari-ca-key.pem"
# Issue self-signed certificate for Tari CA
openssl req -new -x509 -key "${KEY_LOCATION}/tari-ca-key.pem" -out "${KEY_LOCATION}/tari-ca-cert.pem" -days 730 -config ca-cert.conf -nodes -extensions v3_req

# Generate a private key for Tari server
openssl ecparam -name prime256v1 -genkey -noout -out "${KEY_LOCATION}/tari-server-key-ec.pem"
openssl pkcs8 -topk8 -nocrypt -in "${KEY_LOCATION}/tari-server-key-ec.pem" -out "${KEY_LOCATION}/tari-server-key.pem"
rm "${KEY_LOCATION}/tari-server-key-ec.pem"
# Generate a CSR for Tari server using its private key and the server configuration file
openssl req -new -key "${KEY_LOCATION}/tari-server-key.pem" -out csr.pem -config server-csr.conf
# Generate a Tari server certificate using the CSR and the Tari CA priv and cert
openssl x509 -req -days 365 -in csr.pem -CA "${KEY_LOCATION}/tari-ca-cert.pem" -CAkey "${KEY_LOCATION}/tari-ca-key.pem" -CAcreateserial -out "${KEY_LOCATION}/tari-server-cert.pem" -extfile server-ext.conf

# Cleanup
rm ca-cert.conf server-csr.conf server-ext.conf csr.pem

