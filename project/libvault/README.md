# RustyVault

![RustVault logo](docs/static/img/RV-logo-gold.png)

[![Crates.io Version](https://img.shields.io/crates/v/rusty_vault)](https://crates.io/crates/rusty_vault)

## Overview

RustyVault is a modern secret management system, written in Rust. RustyVault provides various features which support many scenarios including secure storage, cloud identity management, secret management, Kubernetes integration, PKI infrastructure, cryptographic computing, traditional key management, etc.

RustyVault can be deployed in either cloud or physical environments. Depending on different requirements, RustyVault may run as standalone application with a set of RESTful APIs provided, and it can also be used as a crate thus you can easily integrate it into your own Rust application.

The core cryptographic module which provides cryptography functionality to RustVault can be configurable, for instance it could be [OpenSSL](https://github.com/openssl/openssl) or [Tongsuo](https://github.com/Tongsuo-Project/Tongsuo) project depending on the actual scenarios.

One of the goals of RustyVault is to replace Hashicorp Vault seamlessly if you are seeking for an OSI-approved open-source license and enterprise level features.

## Feature

Part of the features provided by RustyVault are as follows:

* Working Mode
  * standalone process w/HTTP APIs
  * Rust crate that can be easily integrated with other applications
* Configurable underlying Cryptographic Module
  * OpenSSL library
  * Tongsuo library
  * native Rust crypto libraries
* API
  * RESTful API, compatible with Hashicorp Vault
* Authentication & Authorization
  * X.509 certificate
  * username/password
  * basic ACL
* Secure Storage
  * on-disk
  * remote storage (etcd, etc)
* Configuration
  * HCL compatible
* PKI/CA Infrastructure
  * X.509 certificate signing: RSA/ECC/SM2
  * X.509 certificate revocation: OCSP, CRL
* Key Management
  * symmetric key: create/rotate/store
  * public key: RSA/ECC/SM2
* Cryptography Algorithm
  * encryption: AES, SM4
  * public Key:
      * Signature: RSA/ECDSA/EdDSA/SM2
      * Encryption: RSA/SM2
  * hash: SHA1/SHA2/SM3
  * PRNG
* Cryptographic Computing
  * PHE: Paillier, EC-ElGamal
  * ZKP: Bulletproofs w/Twisted-ElGamal
* Hardware Support
  * cryptography accelerator
  * TEE
* Cluster & HA
  * support "active/active" mode
* Logging & Audit
  * log to file

## Design

Read the [design](./docs/docs/design.md) document.
