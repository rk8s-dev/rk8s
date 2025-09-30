---
sidebar_position: 4
title: Motivation
---
# Motivation

HashiCorp Vault is the most widely used secret management product in cloud native realm. But in practice, it has some disadvantages:

1. Open-source license is not OSI-approved any more;
2. Lack of cryptography compliance ability except FIPS, including:
  * cryptography algorithms
  * cryptography validations in other countries and regions
3. Inadequate cryptography performance especially in  critical scenarios;
4. Many useful features are not open-sourced
5. ...

And compared to Hashicorp Vault, there is rare open source key/secret management project available in the market. Thus, we started a new open source project to address the issues.

The new project needs to fulfill most features the a traditional KMS has. It also needs to be a replacement for Hashicorp Vault, with the features that even are not included in the open source versions of Vault. As such, the new project should be:

1. Written in Rust to achieve memory safe
2. Fully compatible with Hashicorp Vault on APIs and data format
3. Configurable underlying cryptographic module
4. High performance on cryptography operations
5. High availability
6. Support for underlying cryptography hardware
7. OSI-approved open-source license

# Requirements List

Language: Rust

Project Name: RustyVault

Features:

* API
  * RESTful
     * Compatible with Hashicorp Vault
  * gRPC (low priority)
* User and Authentication
  * X.509 based authentication
  * Password based authentication
  * Basic ACL
  * Role based secret management
* Configuration
  * Support configuration file
  * Dynamic reload
* PKI/CA
  * X.509 issuing: RSA/ECC/SM2
  * X.509 revocation: OCSP, CRL
* Key Management
  * Symmetric: generation/storage/rotation
  * Public key type: RSA/ECC/SM2
* Cryptography Algorithm
  * Symmetric ciphers: AES, SM4
  * Public key algorithms:
      * Signature: RSA/ECDSA/EdDSA/SM2/Ring Signature
      * Encryption: RSA/SM2
  * Digest: SHA1/SHA2/SM3
* Advanced Cryptography Algorithm
  * PHE: Paillier, EC-ElGamal
  * ZKP: Bulletproofs
  * Post Quantum Cryptography
* Hardware Support
  * Acceleration card or CPU instruction sets
  * HSMs
* Cluster and HA
  * Active - Active mode
* Storage
  * local disk
  * etcd/consul...
* Logging and Audit
  * TBD
